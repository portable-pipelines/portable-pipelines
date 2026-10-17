//! JOIN — Author's Edition 1.1.11, pp. 399–401.
//!
//! Syntax (fixed order):
//! `[COUNT] [number | * | KEYLENgth number] [delimitedString [TERMinate]] [number]`

use pipe_core::{Record, StageCtx, StreamId};

pub fn join(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let parsed = match parse_join(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("JOIN: {e}"));
                return;
            }
        };

        if ctx.output_connected(StreamId::SECONDARY) {
            ctx.fail("JOIN: no output stream other than the primary may be connected");
            return;
        }

        let secondary = ctx.input_connected(StreamId::SECONDARY);
        let spec = parsed.resolve(secondary);

        if !ctx.output_connected(StreamId::PRIMARY) {
            sever_inputs(ctx, secondary);
            return;
        }

        run_join(ctx, spec, secondary);
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ParsedMode {
    /// Default append-1, or unlimited when a secondary input is defined.
    Default,
    Append(u64),
    Unlimited,
    KeyLength(usize),
}

#[derive(Clone, Debug)]
struct ParsedJoin {
    prefix_count: bool,
    mode: ParsedMode,
    insert: Vec<u8>,
    terminate: bool,
    max_len: Option<usize>,
}

#[derive(Clone, Debug)]
enum JoinMode {
    Append(Option<u64>),
    KeyLength(usize),
}

#[derive(Clone, Debug)]
struct JoinSpec {
    prefix_count: bool,
    mode: JoinMode,
    insert: Vec<u8>,
    terminate: bool,
    max_len: Option<usize>,
}

impl ParsedJoin {
    fn resolve(self, secondary: bool) -> JoinSpec {
        let mode = match self.mode {
            ParsedMode::Default => {
                if secondary {
                    JoinMode::Append(None)
                } else {
                    JoinMode::Append(Some(1))
                }
            }
            ParsedMode::Append(n) => JoinMode::Append(Some(n)),
            ParsedMode::Unlimited => JoinMode::Append(None),
            ParsedMode::KeyLength(n) => JoinMode::KeyLength(n),
        };
        JoinSpec {
            prefix_count: self.prefix_count,
            mode,
            insert: self.insert,
            terminate: self.terminate,
            max_len: self.max_len,
        }
    }
}

fn parse_join(args: &[String]) -> Result<ParsedJoin, String> {
    let mut i = 0;

    // [COUNT] — whole word; diagram shows COUNT with no abbreviation.
    let mut prefix_count = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("COUNT") {
        prefix_count = true;
        i += 1;
    }

    // [number | * | KEYLENgth number]
    let mut mode = ParsedMode::Default;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if args[i] == "*" {
            mode = ParsedMode::Unlimited;
            i += 1;
        } else if matches_abbrev(&upper, "KEYLENGTH", 6) {
            i += 1;
            if i >= args.len() {
                return Err("KEYLENGTH requires a number".into());
            }
            let n = parse_usize(&args[i], "KEYLENGTH")?;
            mode = ParsedMode::KeyLength(n);
            i += 1;
        } else if let Ok(n) = args[i].parse::<u64>() {
            mode = ParsedMode::Append(n);
            i += 1;
        }
    }

    // [delimitedString [TERMinate]]
    let mut insert = Vec::new();
    let mut terminate = false;
    if i < args.len() {
        if let Some(inner) = delimited_inner(&args[i]) {
            insert = inner.as_bytes().to_vec();
            i += 1;
            if i < args.len() {
                let upper = args[i].to_ascii_uppercase();
                if matches_abbrev(&upper, "TERMINATE", 4) {
                    terminate = true;
                    i += 1;
                }
            }
        }
    }

    // [number] — maximum output record length; not valid as the first word.
    let mut max_len = None;
    if i < args.len() {
        let n = args[i]
            .parse::<usize>()
            .map_err(|_| unexpected_operand(&args[i]))?;
        max_len = Some(n);
        i += 1;
    }

    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    if terminate && insert.is_empty() && !args.iter().any(|a| delimited_inner(a).is_some()) {
        return Err("TERMINATE requires a delimited string".into());
    }

    Ok(ParsedJoin {
        prefix_count,
        mode,
        insert,
        terminate,
        max_len,
    })
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if matches_abbrev(&upper, "KEYLENGTH", 6)
        || upper == "COUNT"
        || matches_abbrev(&upper, "TERMINATE", 4)
    {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn matches_abbrev(s: &str, full: &str, min_len: usize) -> bool {
    s.len() >= min_len && s.len() <= full.len() && full.starts_with(s)
}

fn parse_usize(s: &str, what: &str) -> Result<usize, String> {
    s.parse::<usize>()
        .map_err(|_| format!("{what} requires a number, got {s}"))
}

/// Inner text of a delimited string (`/*/`, `% %`, …). `None` if not delimited.
fn delimited_inner(s: &str) -> Option<&str> {
    if s.len() < 2 {
        return None;
    }
    let mut chars = s.chars();
    let first = chars.next()?;
    let last = chars.next_back()?;
    if first == last && !first.is_alphanumeric() && first != '-' && first != '.' {
        let start = first.len_utf8();
        let end = s.len() - last.len_utf8();
        if end >= start {
            return Some(&s[start..end]);
        }
    }
    None
}

struct Buffer {
    bytes: Vec<u8>,
    contributors: u64,
    key: Option<Vec<u8>>,
}

impl Buffer {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            contributors: 0,
            key: None,
        }
    }

    fn is_empty(&self) -> bool {
        self.contributors == 0
    }

    fn clear(&mut self) {
        self.bytes.clear();
        self.contributors = 0;
        self.key = None;
    }
}

fn run_join(ctx: &mut StageCtx, spec: JoinSpec, secondary: bool) {
    let mut buf = Buffer::new();
    ctx.select_output(StreamId::PRIMARY);

    if secondary {
        loop {
            match ctx.read_any(&[StreamId::PRIMARY, StreamId::SECONDARY]) {
                Ok(Some((StreamId::PRIMARY, rec))) => {
                    if !add_record(ctx, &spec, &mut buf, rec) {
                        sever_inputs(ctx, true);
                        return;
                    }
                }
                Ok(Some((_, _))) => {
                    if !flush(ctx, &spec, &mut buf) {
                        sever_inputs(ctx, true);
                        return;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = flush(ctx, &spec, &mut buf);
                    return;
                }
            }
        }
    } else {
        loop {
            match ctx.read() {
                Ok(Some(rec)) => {
                    if !add_record(ctx, &spec, &mut buf, rec) {
                        return;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = flush(ctx, &spec, &mut buf);
                    return;
                }
            }
        }
    }

    let _ = flush(ctx, &spec, &mut buf);
}

fn add_record(ctx: &mut StageCtx, spec: &JoinSpec, buf: &mut Buffer, rec: Record) -> bool {
    let payload = match &spec.mode {
        JoinMode::KeyLength(key_len) => {
            let key = record_key(rec.as_bytes(), *key_len);
            if !buf.is_empty() {
                if buf.key.as_deref() != Some(key.as_slice()) {
                    if !flush(ctx, spec, buf) {
                        return false;
                    }
                }
            }
            if buf.is_empty() {
                buf.key = Some(key);
                rec.as_bytes().to_vec()
            } else {
                strip_key(rec.as_bytes(), *key_len)
            }
        }
        JoinMode::Append(_) => rec.as_bytes().to_vec(),
    };

    if let Some(max) = spec.max_len {
        let added = if buf.is_empty() {
            payload.len()
        } else {
            spec.insert.len() + payload.len()
        };
        if !buf.is_empty() && buf.bytes.len() + added > max {
            if !flush(ctx, spec, buf) {
                return false;
            }
            return start_or_pass(ctx, spec, buf, rec, payload);
        }
        if buf.is_empty() && payload.len() >= max {
            return write_unchanged(ctx, spec, &rec);
        }
    }

    if !buf.is_empty() {
        buf.bytes.extend_from_slice(&spec.insert);
    }
    buf.bytes.extend_from_slice(&payload);
    buf.contributors += 1;

    if set_complete(spec, buf) {
        return flush(ctx, spec, buf);
    }
    true
}

fn start_or_pass(
    ctx: &mut StageCtx,
    spec: &JoinSpec,
    buf: &mut Buffer,
    rec: Record,
    payload: Vec<u8>,
) -> bool {
    if let Some(max) = spec.max_len {
        if payload.len() >= max {
            return write_unchanged(ctx, spec, &rec);
        }
    }
    if let JoinMode::KeyLength(key_len) = spec.mode {
        buf.key = Some(record_key(rec.as_bytes(), key_len));
        buf.bytes.extend_from_slice(rec.as_bytes());
    } else {
        buf.bytes.extend_from_slice(&payload);
    }
    buf.contributors = 1;
    if set_complete(spec, buf) {
        return flush(ctx, spec, buf);
    }
    true
}

fn set_complete(spec: &JoinSpec, buf: &Buffer) -> bool {
    match spec.mode {
        JoinMode::Append(Some(n)) => buf.contributors >= n.saturating_add(1),
        JoinMode::Append(None) | JoinMode::KeyLength(_) => false,
    }
}

fn record_key(bytes: &[u8], key_len: usize) -> Vec<u8> {
    bytes[..key_len.min(bytes.len())].to_vec()
}

fn strip_key(bytes: &[u8], key_len: usize) -> Vec<u8> {
    if bytes.len() <= key_len {
        Vec::new()
    } else {
        bytes[key_len..].to_vec()
    }
}

fn write_unchanged(ctx: &mut StageCtx, spec: &JoinSpec, rec: &Record) -> bool {
    let mut out = Vec::new();
    if spec.prefix_count {
        out.extend_from_slice(&count_prefix(1));
    }
    out.extend_from_slice(rec.as_bytes());
    emit(ctx, out)
}

fn flush(ctx: &mut StageCtx, spec: &JoinSpec, buf: &mut Buffer) -> bool {
    if buf.is_empty() {
        return true;
    }
    if spec.terminate {
        buf.bytes.extend_from_slice(&spec.insert);
    }
    let mut out = Vec::new();
    if spec.prefix_count {
        out.extend_from_slice(&count_prefix(buf.contributors));
    }
    out.extend_from_slice(&buf.bytes);
    buf.clear();
    emit(ctx, out)
}

fn count_prefix(n: u64) -> Vec<u8> {
    format!("{n:>10}").into_bytes()
}

fn emit(ctx: &mut StageCtx, bytes: Vec<u8>) -> bool {
    ctx.select_output(StreamId::PRIMARY);
    match ctx.write(Record::from(bytes)) {
        Ok(()) => {
            if !ctx.output_connected(StreamId::PRIMARY) {
                false
            } else {
                true
            }
        }
        Err(_) => false,
    }
}

fn sever_inputs(ctx: &mut StageCtx, secondary: bool) {
    ctx.select_input(StreamId::PRIMARY);
    ctx.short_input();
    if secondary {
        ctx.select_input(StreamId::SECONDARY);
        ctx.short_input();
    }
}
