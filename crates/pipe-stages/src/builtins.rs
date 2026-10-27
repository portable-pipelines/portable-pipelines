use std::io::{BufRead, Write};
use std::process::Command;

use pipe_core::{PipeError, Record, StageCtx, StreamId};

use crate::common::{
    decode_dstring, decode_dstring_text, is_anycase, is_anycase_min4, is_delimited, matches_abbrev,
    strip_delim,
};

/// LITERAL — write the argument string, then pass input to output (1.1.11 p.412).
pub fn literal(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let rec = if args.is_empty() {
            Record::empty()
        } else {
            Record::from(args[0].as_str())
        };
        crate::common::write_then_copy(ctx, rec);
    })
}

/// CONSOLE — read the terminal when first; otherwise write records and copy them (p.303).
pub fn console(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let first = !ctx.input_connected(StreamId::PRIMARY);
        if first {
            match parse_console_read(&args) {
                Ok(spec) => console_read(ctx, spec),
                Err(e) => ctx.fail(format!("CONSOLE: {e}")),
            }
        } else {
            if !args.is_empty() {
                ctx.fail("CONSOLE: no argument is allowed when CONSOLE is not first");
                return;
            }
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            while let Ok(Some(r)) = ctx.read() {
                let _ = write_host_record(&mut lock, &r);
                if ctx.output_connected(StreamId::PRIMARY) && ctx.write(r).is_err() {
                    break;
                }
            }
        }
    })
}

struct ConsoleRead {
    eof: Option<String>,
}

fn parse_console_read(args: &[String]) -> Result<ConsoleRead, String> {
    let mut eof = Some(String::new());
    let mut i = 0;
    while i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u == "EOF" {
            i += 1;
            let s = args.get(i).ok_or("EOF requires a delimited string")?;
            eof = Some(decode_dstring_text(s)?);
            i += 1;
        } else if u == "NOEOF" {
            eof = None;
            i += 1;
        } else if matches_abbrev(&u, "ASYNCHRONOUSLY", 5)
            || u == "DIRECT"
            || u == "DARK"
            || matches_abbrev(&u, "INVISIBLE", 3)
        {
            return Err(format!("{u} is CMS-specific and is not implemented"));
        } else {
            return Err(format!("unrecognized operand {}", args[i]));
        }
    }
    Ok(ConsoleRead { eof })
}

fn console_read(ctx: &mut StageCtx, spec: ConsoleRead) {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    loop {
        let line = match read_host_record(&mut reader) {
            Ok(Some(l)) => l,
            _ => break,
        };
        if let Some(eof) = &spec.eof {
            if line.as_slice() == eof.as_bytes() {
                break;
            }
        }
        if ctx.write(Record::from(line)).is_err() {
            break;
        }
    }
}

/// DISK — portable stand-in for CMS `disk` / `diskfast` (p.328).
///
/// First stage: read records. A missing file is a null file.
/// Not first: append/create. The operand is a host path (not CMS `fn ft fm`).
pub fn disk(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let path = match file_path(&args, "DISK") {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(e);
                return;
            }
        };
        if let Err(e) = reject_invented_disk_mode(&path) {
            ctx.fail(format!("DISK: {e}"));
            return;
        }
        if !ctx.input_connected(StreamId::PRIMARY) {
            file_read(ctx, &path, true);
        } else {
            file_write(ctx, &path, true);
        }
    })
}

/// `<` — read a file (1.1.11 p.230). Must be first. Host path operand.
pub fn file_in(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.fail("< must be a first stage");
            return;
        }
        let path = match file_path(&args, "<") {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(e);
                return;
            }
        };
        file_read(ctx, &path, false);
    })
}

/// `>` — replace or create a file (1.1.11 p.237). Must not be first.
pub fn file_replace(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !ctx.input_connected(StreamId::PRIMARY) {
            ctx.fail("> must not be a first stage");
            return;
        }
        let path = match file_path(&args, ">") {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(e);
                return;
            }
        };
        file_write(ctx, &path, false);
    })
}

/// `>>` — append to or create a file (1.1.11 p.245). Must not be first.
pub fn file_append(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !ctx.input_connected(StreamId::PRIMARY) {
            ctx.fail(">> must not be a first stage");
            return;
        }
        let path = match file_path(&args, ">>") {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(e);
                return;
            }
        };
        file_write(ctx, &path, true);
    })
}

fn file_path(args: &[String], verb: &str) -> Result<String, String> {
    if args.is_empty() || args[0].is_empty() {
        return Err(format!("{verb}: a path is required"));
    }
    Ok(args[0].clone())
}

fn reject_invented_disk_mode(path: &str) -> Result<(), String> {
    let mut parts = path.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("");
    if (first.eq_ignore_ascii_case("OUTPUT") || first.eq_ignore_ascii_case("APPEND"))
        && parts.next().is_some()
    {
        return Err("OUTPUT/APPEND are not IBM operands; use > or >>".into());
    }
    Ok(())
}

/// Split one host-stream record: LF ends the record; a single CR immediately
/// before that LF is stripped. Bytes are otherwise passed through unchanged.
fn read_host_record<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
    Ok(Some(buf))
}

fn write_host_record<W: Write>(writer: &mut W, rec: &Record) -> std::io::Result<()> {
    writer.write_all(rec.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn file_read(ctx: &mut StageCtx, path: &str, missing_is_empty: bool) {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && missing_is_empty => return,
        Err(e) => {
            ctx.fail(format!("file read {path}: {e}"));
            return;
        }
    };
    let mut reader = std::io::BufReader::new(f);
    loop {
        match read_host_record(&mut reader) {
            Ok(Some(line)) => {
                if ctx.write(Record::from(line)).is_err() {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                ctx.fail(format!("file read: {e}"));
                return;
            }
        }
    }
}

fn file_write(ctx: &mut StageCtx, path: &str, append: bool) {
    let mut f = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => {
            ctx.fail(format!("file write {path}: {e}"));
            return;
        }
    };
    // > / >> / diskfast copy through (1.1.11: strictly do not delay the record).
    while let Ok(Some(r)) = ctx.read() {
        if let Err(e) = write_host_record(&mut f, &r) {
            ctx.fail(format!("file write {path}: {e}"));
            return;
        }
        if ctx.output_connected(StreamId::PRIMARY) && ctx.write(r).is_err() {
            break;
        }
    }
}

/// COMMAND — run a shell command; stdout records become pipeline records.
///
/// Portable subset: IBM COMMAND issues CMS/TSO commands into the pipeline.
pub fn command(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let mut cmds = Vec::new();
        let joined = args.join(" ");
        if !joined.trim().is_empty() {
            cmds.push(joined);
        }
        while let Ok(Some(r)) = ctx.read() {
            let line = r.to_string_lossy();
            if line.trim().is_empty() {
                continue;
            }
            cmds.push(line);
        }
        for cmd in cmds {
            if !run_host_command(ctx, &cmd) {
                break;
            }
        }
    })
}

fn run_host_command(ctx: &mut StageCtx, cmd: &str) -> bool {
    let output = if cfg!(windows) {
        Command::new("cmd").args(["/C", cmd]).output()
    } else {
        Command::new("sh").args(["-c", cmd]).output()
    };
    match output {
        Ok(out) => {
            let mut cursor = std::io::Cursor::new(out.stdout);
            loop {
                match read_host_record(&mut cursor) {
                    Ok(Some(line)) => {
                        if ctx.write(Record::from(line)).is_err() {
                            return false;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        ctx.fail(format!("COMMAND: {e}"));
                        return false;
                    }
                }
            }
            true
        }
        Err(e) => {
            ctx.fail(format!("COMMAND: {e}"));
            false
        }
    }
}

/// COPY — copy records, consuming each input before writing (1.1.11 p.305).
///
/// Arcane filter. Each record is read into a buffer (consumed) before the
/// buffer is written, so COPY has the potential to delay one record and
/// decouples a producer from a waiting consumer. Premature termination when
/// the output stream is not connected.
pub fn copy(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !args.is_empty() {
            ctx.fail("COPY: no operands are allowed");
            return;
        }
        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("COPY: no input stream other than the primary may be connected");
                return;
            }
        }
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
            }
            return;
        }
        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);
        run_copy(ctx);
    })
}

fn run_copy(ctx: &mut StageCtx) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return;
        }
        match ctx.read() {
            Ok(Some(r)) => {
                if ctx.write(r).is_err() {
                    if ctx.input_connected(StreamId::PRIMARY) {
                        ctx.short_input();
                    }
                    return;
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

/// LOCATE — select records that contain a string (1.1.11 p.414).
pub fn locate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_locate(&args) {
        Ok(spec) => run_locate(ctx, spec, false),
        Err(e) => ctx.fail(format!("LOCATE: {e}")),
    })
}

/// CHANGE — substitute string occurrences (1.1.11 p.286). Default: all.
pub fn change(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_change(&args) {
        Ok(spec) => run_change(ctx, spec),
        Err(e) => ctx.fail(format!("CHANGE: {e}")),
    })
}

/// COUNT — characters, words, lines, min/max length (1.1.11 p.306).
pub fn count(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_count(&args) {
        Ok(spec) => run_count(ctx, spec),
        Err(e) => ctx.fail(format!("COUNT: {e}")),
    })
}

/// SORT — order records (1.1.11 p.493). Default: ascending, whole record.
pub fn sort(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_sort(&args) {
        Ok(spec) => run_sort(ctx, spec),
        Err(e) => ctx.fail(format!("SORT: {e}")),
    })
}

/// BUFFER — store records until EOF (and optional null-record subfiles, p.279).
pub fn buffer(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_buffer(&args) {
        Ok(spec) => run_buffer(ctx, spec),
        Err(e) => ctx.fail(format!("BUFFER: {e}")),
    })
}

/// FANOUT — copy each input record to every connected output (1.1.11 p.341).
pub fn fanout(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_fanout(&args) {
        Ok(stop) => run_fanout(ctx, stop),
        Err(e) => ctx.fail(format!("FANOUT: {e}")),
    })
}

/// FANIN — read listed (or all connected) input streams in order (1.1.11 p.338).
pub fn fanin(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_fanin(&args) {
        Ok(listed) => {
            let order = if listed.is_empty() {
                ctx.connected_inputs()
            } else {
                listed
            };
            for s in order {
                ctx.select_input(s);
                while let Ok(Some(r)) = ctx.read() {
                    if ctx.write(r).is_err() {
                        return;
                    }
                }
            }
        }
        Err(e) => ctx.fail(format!("FANIN: {e}")),
    })
}

fn parse_fanin(args: &[String]) -> Result<Vec<StreamId>, String> {
    let mut out = Vec::new();
    for a in args {
        if let Ok(n) = a.parse::<u32>() {
            out.push(StreamId(n));
        } else {
            return Err(format!("stream identifier {a} is not implemented"));
        }
    }
    Ok(out)
}

/// FANINANY — copy from whichever input stream has a record (1.1.11 p.339).
pub fn faninany(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !args.is_empty() {
            let u = args[0].to_ascii_uppercase();
            if u != "STRICT" {
                ctx.fail(format!("FANINANY: unrecognized operand {}", args[0]));
                return;
            }
        }
        let streams = ctx.connected_inputs();
        if streams.is_empty() {
            return;
        }
        while let Ok(Some((_, r))) = ctx.read_any(&streams) {
            ctx.select_output(StreamId::PRIMARY);
            if ctx.write(r).is_err() {
                break;
            }
        }
    })
}

/// HOLE — read and discard every connected input stream (1.1.11 p.382).
pub fn hole(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !args.is_empty() {
            ctx.fail("HOLE: no operands are allowed");
            return;
        }
        for s in ctx.connected_inputs() {
            ctx.select_input(s);
            while let Ok(Some(_)) = ctx.read() {}
        }
    })
}

/// TAKE — select records from the beginning or end of the file.
///
/// Record counts only. `BYTES` is rejected.
pub fn take(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_ends(&args) {
        Ok(spec) => run_ends(ctx, spec, StreamId::PRIMARY, StreamId::SECONDARY),
        Err(e) => ctx.fail(format!("TAKE: {e}")),
    })
}

/// DROP — discard records from the beginning or end of the file.
///
/// Converse of TAKE. Record counts only. `BYTES` is rejected.
pub fn drop(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_ends(&args) {
        Ok(spec) => run_ends(ctx, spec, StreamId::SECONDARY, StreamId::PRIMARY),
        Err(e) => ctx.fail(format!("DROP: {e}")),
    })
}

/// NLOCATE — converse of LOCATE (1.1.11 p.439).
pub fn nlocate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_locate(&args) {
        Ok(spec) => run_locate(ctx, spec, true),
        Err(e) => ctx.fail(format!("NLOCATE: {e}")),
    })
}

/// STRIP — remove leading and/or trailing characters.
///
/// Default is both ends, blank. `STRING`, `ANYOF`, xrange, `NOT` / `TO`,
/// `ANYCASE`, and an optional per-side count are implemented.
pub fn strip(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_strip(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("STRIP: {e}"));
                return;
            }
        };
        while let Ok(Some(r)) = ctx.read() {
            let out = apply_strip(&r.to_string_lossy(), &spec);
            if ctx.write(Record::from(out)).is_err() {
                break;
            }
        }
    })
}

#[derive(Clone, Copy)]
enum WhichEnd {
    First,
    Last,
}

#[derive(Clone, Copy)]
enum Span {
    Records(u64),
    All,
}

struct EndSpec {
    which: WhichEnd,
    span: Span,
}

fn parse_ends(args: &[String]) -> Result<EndSpec, String> {
    let mut which: Option<WhichEnd> = None;
    let mut span: Option<Span> = None;
    for a in args {
        let upper = a.to_ascii_uppercase();
        if upper == "FIRST" || upper == "LAST" {
            if which.is_some() {
                return Err("duplicate FIRST/LAST".into());
            }
            which = Some(if upper == "LAST" {
                WhichEnd::Last
            } else {
                WhichEnd::First
            });
        } else if upper == "BYTES" {
            return Err("BYTES is not implemented".into());
        } else if a == "*" {
            if span.is_some() {
                return Err("duplicate count".into());
            }
            span = Some(Span::All);
        } else if let Ok(n) = a.parse::<u64>() {
            if span.is_some() {
                return Err("duplicate count".into());
            }
            span = Some(Span::Records(n));
        } else {
            return Err(format!("unrecognized operand {a}"));
        }
    }
    Ok(EndSpec {
        which: which.unwrap_or(WhichEnd::First),
        span: span.unwrap_or(Span::Records(1)),
    })
}

/// `counted` receives the selected span. `rest` receives the other records.
fn run_ends(ctx: &mut StageCtx, spec: EndSpec, counted: StreamId, rest: StreamId) {
    match spec.which {
        WhichEnd::First => copy_first(ctx, spec.span, counted, rest),
        WhichEnd::Last => copy_last(ctx, spec.span, counted, rest),
    }
}

fn copy_first(ctx: &mut StageCtx, span: Span, counted: StreamId, rest: StreamId) {
    if matches!(span, Span::Records(0)) {
        finish_span(ctx, counted, rest);
        return;
    }
    let mut seen = 0u64;
    while let Ok(Some(r)) = ctx.read() {
        let within = match span {
            Span::All => true,
            Span::Records(n) => seen < n,
        };
        if within {
            ctx.select_output(counted);
            if ctx.write(r).is_err() {
                break;
            }
            seen += 1;
            if let Span::Records(n) = span {
                if seen == n {
                    finish_span(ctx, counted, rest);
                    return;
                }
            }
        }
    }
}

/// Sever `counted`, then either copy the unread input to `rest` or stop the producer.
///
/// The input is severed before `sever_output` waits, so a producer blocked on the
/// one-record slot cannot enqueue another record during that wait.
fn finish_span(ctx: &mut StageCtx, counted: StreamId, rest: StreamId) {
    let rest_live = ctx.output_connected(rest);
    if !rest_live {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
    }
    ctx.select_output(counted);
    ctx.sever_output();
    if !rest_live {
        return;
    }
    while let Ok(Some(r)) = ctx.read() {
        ctx.select_output(rest);
        if ctx.write(r).is_err() {
            break;
        }
    }
}

fn copy_last(ctx: &mut StageCtx, span: Span, counted: StreamId, rest: StreamId) {
    match span {
        Span::Records(0) => {
            finish_span(ctx, counted, rest);
        }
        Span::All => {
            let recs = ctx.read_all().unwrap_or_default();
            ctx.select_output(rest);
            ctx.sever_output();
            ctx.select_output(counted);
            for r in recs {
                if ctx.write(r).is_err() {
                    break;
                }
            }
        }
        Span::Records(n) => {
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            let mut buf = std::collections::VecDeque::new();
            while let Ok(Some(r)) = ctx.read() {
                if buf.len() < n {
                    buf.push_back(r);
                } else {
                    let old = buf.pop_front().expect("window is non-empty");
                    buf.push_back(r);
                    ctx.select_output(rest);
                    let _ = ctx.write(old);
                }
            }
            ctx.select_output(rest);
            ctx.sever_output();
            ctx.select_output(counted);
            for r in buf {
                if ctx.write(r).is_err() {
                    break;
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum StripSide {
    Both,
    Leading,
    Trailing,
}

struct StripSpec {
    side: StripSide,
    anycase: bool,
    negate: bool,
    target: String,
    /// STRING keyword: match the whole string; otherwise a character set.
    as_string: bool,
    limit: Option<usize>,
}

fn parse_strip(args: &[String]) -> Result<StripSpec, String> {
    let mut side = None;
    let mut anycase = false;
    let mut negate = false;
    let mut target = None;
    let mut as_string = false;
    let mut limit = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let upper = a.to_ascii_uppercase();
        if upper == "BOTH" || upper == "LEADING" || upper == "TRAILING" {
            if side.is_some() {
                return Err("duplicate side".into());
            }
            side = Some(match upper.as_str() {
                "LEADING" => StripSide::Leading,
                "TRAILING" => StripSide::Trailing,
                _ => StripSide::Both,
            });
            i += 1;
        } else if is_anycase_min4(a) {
            if anycase {
                return Err("duplicate ANYCASE".into());
            }
            anycase = true;
            i += 1;
        } else if upper == "NOT" || upper == "TO" {
            if negate {
                return Err("duplicate NOT".into());
            }
            negate = true;
            i += 1;
        } else if upper == "BLANK" {
            if target.is_some() {
                return Err("duplicate target".into());
            }
            target = Some(" ".to_string());
            i += 1;
        } else if matches_abbrev(&upper, "ANYOF", 3) {
            if target.is_some() {
                return Err("duplicate target".into());
            }
            let next = args.get(i + 1).ok_or("ANYOF requires a string")?;
            target = Some(decode_dstring_text(next)?);
            i += 2;
        } else if matches_abbrev(&upper, "STRING", 3) {
            if target.is_some() {
                return Err("duplicate target".into());
            }
            let next = args
                .get(i + 1)
                .ok_or("STRING requires a delimited string")?;
            target = Some(decode_dstring_text(next)?);
            as_string = true;
            i += 2;
        } else if looks_like_xrange(a) {
            if target.is_some() {
                return Err("duplicate target".into());
            }
            target = Some(expand_xrange_chars(a)?);
            i += 1;
        } else if let Ok(n) = a.parse::<usize>() {
            if limit.is_some() {
                return Err("duplicate count".into());
            }
            limit = Some(n);
            i += 1;
        } else if is_delimited(a) || is_char_target(a) {
            if target.is_some() {
                return Err("duplicate target".into());
            }
            target = Some(if is_delimited(a) {
                decode_dstring_text(a)?
            } else {
                strip_delim(a).to_string()
            });
            i += 1;
        } else {
            return Err(format!("unrecognized operand {a}"));
        }
    }
    Ok(StripSpec {
        side: side.unwrap_or(StripSide::Both),
        anycase,
        negate,
        target: target.unwrap_or_else(|| " ".to_string()),
        as_string,
        limit,
    })
}

fn is_char_target(s: &str) -> bool {
    if is_delimited(s) {
        return true;
    }
    s.chars().count() == 1
}

fn looks_like_xrange(s: &str) -> bool {
    let Some((left, right)) = s.split_once('-') else {
        return false;
    };
    !left.is_empty() && !right.is_empty() && !right.contains('-')
}

fn expand_xrange_chars(s: &str) -> Result<String, String> {
    if let Some((left, right)) = s.split_once('-') {
        if !left.is_empty() && !right.is_empty() && !right.contains('-') {
            let from = xorc_byte(left)?;
            let to = xorc_byte(right)?;
            let mut out = String::new();
            if from <= to {
                for b in from..=to {
                    out.push(char::from(b));
                }
            } else {
                for b in from..=255 {
                    out.push(char::from(b));
                }
                for b in 0..=to {
                    out.push(char::from(b));
                }
            }
            return Ok(out);
        }
    }
    if let Some((left, right)) = s.split_once('.') {
        if !left.is_empty() && !right.is_empty() {
            let from = xorc_byte(left)?;
            let n: u32 = right
                .parse()
                .map_err(|_| format!("invalid xrange count {right}"))?;
            let mut out = String::new();
            let mut b = from;
            for _ in 0..n {
                out.push(char::from(b));
                b = b.wrapping_add(1);
            }
            return Ok(out);
        }
    }
    Err(format!("invalid xrange {s}"))
}

fn xorc_byte(s: &str) -> Result<u8, String> {
    let u = s.to_ascii_uppercase();
    if u == "BLANK" || u == "SPACE" {
        return Ok(b' ');
    }
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return u8::from_str_radix(s, 16).map_err(|_| format!("invalid xorc {s}"));
    }
    if s.len() == 1 {
        return Ok(s.as_bytes()[0]);
    }
    Err(format!("invalid xorc {s}"))
}

fn apply_strip(input: &str, spec: &StripSpec) -> String {
    if spec.as_string {
        return apply_strip_string(input, spec);
    }
    let set: Vec<char> = if spec.anycase {
        spec.target.to_ascii_uppercase().chars().collect()
    } else {
        spec.target.chars().collect()
    };
    let pred = |c: char| {
        let ch = if spec.anycase {
            c.to_ascii_uppercase()
        } else {
            c
        };
        let hit = set.contains(&ch);
        if spec.negate {
            !hit
        } else {
            hit
        }
    };
    match spec.side {
        StripSide::Leading => trim_end_chars(input, true, pred, spec.limit),
        StripSide::Trailing => trim_end_chars(input, false, pred, spec.limit),
        StripSide::Both => {
            let lead = trim_end_chars(input, true, &pred, spec.limit);
            trim_end_chars(&lead, false, &pred, spec.limit)
        }
    }
}

fn apply_strip_string(input: &str, spec: &StripSpec) -> String {
    let needle = spec.target.as_str();
    if needle.is_empty() {
        return input.to_string();
    }
    let mut s = input.to_string();
    let limit = spec.limit.unwrap_or(usize::MAX);
    if matches!(spec.side, StripSide::Both | StripSide::Leading) {
        let mut n = 0;
        while n < limit && strip_prefix_str(&mut s, needle, spec.anycase, spec.negate) {
            n += 1;
        }
    }
    if matches!(spec.side, StripSide::Both | StripSide::Trailing) {
        let mut n = 0;
        while n < limit && strip_suffix_str(&mut s, needle, spec.anycase, spec.negate) {
            n += 1;
        }
    }
    s
}

fn strip_prefix_str(s: &mut String, needle: &str, anycase: bool, negate: bool) -> bool {
    if s.len() < needle.len() {
        return false;
    }
    let hit = if anycase {
        s[..needle.len()].eq_ignore_ascii_case(needle)
    } else {
        s.starts_with(needle)
    };
    if hit == !negate {
        s.replace_range(..needle.len(), "");
        true
    } else {
        false
    }
}

fn strip_suffix_str(s: &mut String, needle: &str, anycase: bool, negate: bool) -> bool {
    if s.len() < needle.len() {
        return false;
    }
    let start = s.len() - needle.len();
    let hit = if anycase {
        s[start..].eq_ignore_ascii_case(needle)
    } else {
        s.ends_with(needle)
    };
    if hit == !negate {
        s.truncate(start);
        true
    } else {
        false
    }
}

fn trim_end_chars(
    input: &str,
    from_start: bool,
    pred: impl Fn(char) -> bool,
    limit: Option<usize>,
) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut removed = 0usize;
    if from_start {
        let mut i = 0;
        while i < chars.len() && pred(chars[i]) && limit.map(|n| removed < n).unwrap_or(true) {
            i += 1;
            removed += 1;
        }
        chars[i..].iter().collect()
    } else {
        let mut end = chars.len();
        while end > 0 && pred(chars[end - 1]) && limit.map(|n| removed < n).unwrap_or(true) {
            end -= 1;
            removed += 1;
        }
        chars[..end].iter().collect()
    }
}

/// SPEC — rearrange record contents (Appendix A keyword `SPECS` → `SPEC`).
/// Author's Edition 1.1.11 pp. 496–502 (tutorial Ch.14, reference Ch.20).
pub fn spec(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::spec::spec(args)
}

struct LocateSpec {
    anycase: bool,
    anyof: bool,
    pattern: Vec<u8>,
}

fn parse_locate(args: &[String]) -> Result<LocateSpec, String> {
    let mut i = 0;
    let mut anycase = false;
    let mut anyof = false;
    while i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if is_anycase_min4(&args[i]) {
            anycase = true;
            i += 1;
        } else if matches_abbrev(&u, "ANYOF", 3) {
            anyof = true;
            i += 1;
        } else if matches!(u.as_str(), "MIXED" | "ONES" | "ZEROS") {
            return Err(format!("{u} bit-mask matching is not implemented"));
        } else if u == "STRING" {
            return Err("STRING is not used with LOCATE; write a delimited string".into());
        } else {
            break;
        }
    }
    let pattern = if i < args.len() {
        let rest = &args[i];
        if looks_like_column_range(rest) {
            return Err("column ranges are not implemented".into());
        }
        if is_delimited(rest) {
            decode_dstring(rest)?
        } else {
            // Bare word is treated as the search string (XEDIT-style literal).
            rest.as_bytes().to_vec()
        }
    } else {
        Vec::new()
    };
    if i + 1 < args.len() {
        return Err(format!("unexpected operand {}", args[i + 1]));
    }
    Ok(LocateSpec {
        anycase,
        anyof,
        pattern,
    })
}

fn looks_like_column_range(s: &str) -> bool {
    if s.starts_with('(') {
        return true;
    }
    let u = s.to_ascii_uppercase();
    if u.starts_with('W') || u.starts_with('F') {
        return false;
    }
    s.contains('-') && s.chars().next().is_some_and(|c| c.is_ascii_digit())
        || s.contains('.') && s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

fn run_locate(ctx: &mut StageCtx, spec: LocateSpec, invert: bool) {
    if !ctx.output_connected(StreamId::PRIMARY) && !ctx.output_connected(StreamId::SECONDARY) {
        return;
    }
    while let Ok(Some(r)) = ctx.read() {
        let hit = locate_hit(r.as_bytes(), &spec);
        let select = if invert { !hit } else { hit };
        ctx.select_output(if select {
            StreamId::PRIMARY
        } else {
            StreamId::SECONDARY
        });
        if ctx.write(r).is_err() && select {
            break;
        }
    }
}

fn locate_hit(bytes: &[u8], spec: &LocateSpec) -> bool {
    // Null string: select records long enough to include column 1 (p.414).
    if spec.pattern.is_empty() {
        return !bytes.is_empty();
    }
    if spec.anyof {
        let set = spec.pattern.as_slice();
        return bytes.iter().any(|&b| {
            set.iter().any(|&p| {
                if spec.anycase {
                    b.to_ascii_uppercase() == p.to_ascii_uppercase()
                } else {
                    b == p
                }
            })
        });
    }
    let needle = spec.pattern.as_slice();
    if needle.len() > bytes.len() {
        return false;
    }
    let last = bytes.len() - needle.len();
    for i in 0..=last {
        let window = &bytes[i..i + needle.len()];
        let matched = if spec.anycase {
            window.eq_ignore_ascii_case(needle)
        } else {
            window == needle
        };
        if matched {
            return true;
        }
    }
    false
}

struct ChangeSpec {
    anycase: bool,
    old: Vec<u8>,
    new: Vec<u8>,
    max: Option<usize>,
}

fn parse_change(args: &[String]) -> Result<ChangeSpec, String> {
    let mut i = 0;
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }
    if i < args.len() && (args[i].starts_with('(') || looks_like_column_range(&args[i])) {
        return Err("column ranges are not implemented".into());
    }
    let (old, new, ni) = parse_change_pair(args, i)?;
    i = ni;
    let mut max = None;
    if i < args.len() {
        if args[i] == "*" {
            max = None;
            i += 1;
        } else if let Ok(n) = args[i].parse::<usize>() {
            if old.is_empty() && n != 1 {
                return Err("numorstar must be 1 when the first string is null".into());
            }
            max = Some(n);
            i += 1;
        } else {
            return Err(format!("unexpected operand {}", args[i]));
        }
    }
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }
    if old.is_empty() {
        max = Some(1);
    }
    Ok(ChangeSpec {
        anycase,
        old,
        new,
        max,
    })
}

fn parse_change_pair(args: &[String], i: usize) -> Result<(Vec<u8>, Vec<u8>, usize), String> {
    let a = args.get(i).ok_or("CHANGE requires a change string")?;
    if let Some((old, new)) = three_delim_pair(a) {
        return Ok((old.into_bytes(), new.into_bytes(), i + 1));
    }
    if is_delimited(a) {
        if let Some(b) = args.get(i + 1) {
            if is_delimited(b) {
                return Ok((decode_dstring(a)?, decode_dstring(b)?, i + 2));
            }
            // `/old/` + `new/` from a split `/old/new/`
            if let Some(d) = a.chars().next() {
                if b.ends_with(d) {
                    return Ok((
                        decode_dstring(a)?,
                        b[..b.len() - d.len_utf8()].as_bytes().to_vec(),
                        i + 2,
                    ));
                }
            }
        }
    }
    Err("CHANGE requires /old/new/ or two delimited strings".into())
}

fn three_delim_pair(s: &str) -> Option<(String, String)> {
    let d = s.chars().next()?;
    if d.is_alphanumeric() {
        return None;
    }
    let idxs: Vec<usize> = s.match_indices(d).map(|(i, _)| i).collect();
    if idxs.len() != 3 || idxs[0] != 0 {
        return None;
    }
    let dlen = d.len_utf8();
    if idxs[2] + dlen != s.len() {
        return None;
    }
    Some((
        s[dlen..idxs[1]].to_string(),
        s[idxs[1] + dlen..idxs[2]].to_string(),
    ))
}

fn run_change(ctx: &mut StageCtx, spec: ChangeSpec) {
    let secondary = ctx.output_connected(StreamId::SECONDARY);
    while let Ok(Some(r)) = ctx.read() {
        let (out, changed) = apply_change(r.as_bytes(), &spec);
        if secondary {
            ctx.select_output(if changed {
                StreamId::PRIMARY
            } else {
                StreamId::SECONDARY
            });
        } else {
            ctx.select_output(StreamId::PRIMARY);
        }
        if ctx.write(Record::from(out)).is_err() {
            break;
        }
    }
}

fn apply_change(bytes: &[u8], spec: &ChangeSpec) -> (Vec<u8>, bool) {
    if spec.old.is_empty() {
        let mut out = spec.new.clone();
        out.extend_from_slice(bytes);
        return (out, true);
    }
    let old = spec.old.as_slice();
    let limit = spec.max.unwrap_or(usize::MAX);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut n = 0usize;
    let mut changed = false;
    while i < bytes.len() {
        if n < limit && i + old.len() <= bytes.len() {
            let window = &bytes[i..i + old.len()];
            let hit = if spec.anycase {
                window.eq_ignore_ascii_case(old)
            } else {
                window == old
            };
            if hit {
                out.extend_from_slice(&change_replacement(window, spec));
                i += old.len();
                n += 1;
                changed = true;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    (out, changed)
}

fn change_replacement(matched: &[u8], spec: &ChangeSpec) -> Vec<u8> {
    if !spec.anycase {
        return spec.new.clone();
    }
    let old_chars: Vec<char> = String::from_utf8_lossy(&spec.old).chars().collect();
    let has_upper = old_chars.iter().any(|c| c.is_ascii_uppercase());
    let has_letter = old_chars.iter().any(|c| c.is_ascii_alphabetic());
    if has_upper || !has_letter {
        return spec.new.clone();
    }
    let first_is_letter = old_chars.first().is_some_and(|c| c.is_ascii_lowercase());
    if !first_is_letter {
        return spec.new.clone();
    }
    let m0 = matched.first().copied().unwrap_or(b'a');
    let m1 = matched.get(1).copied();
    let m0_up = m0.is_ascii_uppercase();
    let m1_low = m1.map(|c| c.is_ascii_lowercase() || !c.is_ascii_alphabetic());
    if !m0_up {
        return spec.new.clone();
    }
    if m1 == Some(b'?') {
        // unused, keep compiler calm in odd matches
    }
    let both_upper = m0_up && m1.is_some_and(|c| c.is_ascii_uppercase());
    if both_upper {
        return spec.new.to_ascii_uppercase();
    }
    // First upper, second lower / not a letter / one-char → first letter of new uppercased.
    let mut out = spec.new.clone();
    if let Some(i) = out.iter().position(|b| b.is_ascii_alphabetic()) {
        out[i] = out[i].to_ascii_uppercase();
    }
    let _ = m1_low;
    out
}

#[derive(Clone, Copy)]
struct CountSpec {
    chars: bool,
    words: bool,
    lines: bool,
    minline: bool,
    maxline: bool,
}

fn parse_count(args: &[String]) -> Result<CountSpec, String> {
    let mut spec = CountSpec {
        chars: false,
        words: false,
        lines: false,
        minline: false,
        maxline: false,
    };
    if args.is_empty() {
        return Err(
            "requires at least one keyword (CHARACTERS, WORDS, LINES, MINLINE, or MAXLINE)".into(),
        );
    }
    for a in args {
        let u = a.to_ascii_uppercase();
        if matches_abbrev(&u, "CHARACTERS", 8) || u == "CHARS" || u == "BYTES" {
            spec.chars = true;
        } else if u == "WORDS" {
            spec.words = true;
        } else if u == "LINES" || u == "RECORDS" {
            spec.lines = true;
        } else if matches_abbrev(&u, "MINLINE", 3) {
            spec.minline = true;
        } else if matches_abbrev(&u, "MAXLINE", 3) {
            spec.maxline = true;
        } else {
            return Err(format!("unrecognized operand {a}"));
        }
    }
    Ok(spec)
}

fn run_count(ctx: &mut StageCtx, spec: CountSpec) {
    let secondary = ctx.output_connected(StreamId::SECONDARY);
    let mut chars = 0u64;
    let mut words = 0u64;
    let mut lines = 0u64;
    let mut min_len: Option<u64> = None;
    let mut max_len: u64 = 0;
    while let Ok(Some(r)) = ctx.read() {
        lines += 1;
        chars += r.len() as u64;
        words += count_words(r.as_bytes());
        let n = r.len() as u64;
        min_len = Some(min_len.map(|m| m.min(n)).unwrap_or(n));
        max_len = max_len.max(n);
        if secondary {
            ctx.select_output(StreamId::PRIMARY);
            if ctx.write(r).is_err() {
                break;
            }
        }
    }
    let min_out = min_len.unwrap_or(i32::MAX as u64);
    let mut parts = Vec::new();
    if spec.chars {
        parts.push(chars.to_string());
    }
    if spec.words {
        parts.push(words.to_string());
    }
    if spec.lines {
        parts.push(lines.to_string());
    }
    if spec.minline {
        parts.push(min_out.to_string());
    }
    if spec.maxline {
        parts.push(max_len.to_string());
    }
    let rec = Record::from(parts.join(" "));
    if secondary {
        // Copies have gone to the primary. Present EOF there (not SHORT) so a
        // later BUFFER/SORT can emit, then write the count on the secondary
        // (p.306: secondary copies, then the count).
        ctx.select_output(StreamId::PRIMARY);
        ctx.close_output();
        ctx.select_output(StreamId::SECONDARY);
    } else {
        ctx.select_output(StreamId::PRIMARY);
    }
    let _ = ctx.write(rec);
}

fn count_words(bytes: &[u8]) -> u64 {
    let mut n = 0u64;
    let mut in_word = false;
    for &b in bytes {
        if b.is_ascii_whitespace() {
            in_word = false;
        } else if !in_word {
            n += 1;
            in_word = true;
        }
    }
    n
}

struct SortSpec {
    unique: bool,
    count: bool,
    descending: bool,
    anycase: bool,
}

fn parse_sort(args: &[String]) -> Result<SortSpec, String> {
    let mut i = 0;
    let mut unique = false;
    let mut count = false;
    let mut descending = false;
    let mut anycase = false;
    if i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if matches_abbrev(&u, "UNIQUE", 4) {
            unique = true;
            i += 1;
        } else if u == "COUNT" {
            count = true;
            i += 1;
        }
    }
    while i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if is_anycase(&args[i]) {
            anycase = true;
        } else if matches_abbrev(&u, "DESCENDING", 4) {
            descending = true;
        } else if matches_abbrev(&u, "ASCENDING", 3) {
            descending = false;
        } else if u == "PAD" || u == "NOPAD" {
            return Err(format!("{u} is not implemented"));
        } else if looks_like_column_range(&args[i]) {
            return Err("sort field ranges are not implemented".into());
        } else {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        i += 1;
    }
    Ok(SortSpec {
        unique,
        count,
        descending,
        anycase,
    })
}

fn run_sort(ctx: &mut StageCtx, spec: SortSpec) {
    let recs = ctx.read_all().unwrap_or_default();
    let mut indexed: Vec<(usize, Record)> = recs.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| {
        let mut ord = if spec.anycase {
            a.1.to_string_lossy()
                .to_ascii_uppercase()
                .cmp(&b.1.to_string_lossy().to_ascii_uppercase())
        } else {
            a.1.as_bytes().cmp(b.1.as_bytes())
        };
        if spec.descending {
            ord = ord.reverse();
        }
        ord.then(a.0.cmp(&b.0))
    });
    let secondary = ctx.output_connected(StreamId::SECONDARY);
    if spec.unique || spec.count {
        let mut i = 0;
        while i < indexed.len() {
            let key = indexed[i].1.as_bytes().to_vec();
            let mut n = 1u64;
            let mut j = i + 1;
            while j < indexed.len() && same_sort_key(&indexed[j].1, &key, spec.anycase) {
                n += 1;
                if secondary {
                    ctx.select_output(StreamId::SECONDARY);
                    let _ = ctx.write(indexed[j].1.clone());
                }
                j += 1;
            }
            ctx.select_output(StreamId::PRIMARY);
            let rec = if spec.count {
                let prefix = format!("{n:>10}");
                let mut data = prefix.into_bytes();
                data.extend_from_slice(indexed[i].1.as_bytes());
                Record::from(data)
            } else {
                indexed[i].1.clone()
            };
            if ctx.write(rec).is_err() {
                break;
            }
            i = j;
        }
    } else {
        for (_, r) in indexed {
            if ctx.write(r).is_err() {
                break;
            }
        }
    }
}

fn same_sort_key(rec: &Record, key: &[u8], anycase: bool) -> bool {
    if anycase {
        rec.to_string_lossy()
            .eq_ignore_ascii_case(&String::from_utf8_lossy(key))
    } else {
        rec.as_bytes() == key
    }
}

struct BufferSpec {
    copies: Option<u64>,
    delim: Option<Vec<u8>>,
}

fn parse_buffer(args: &[String]) -> Result<BufferSpec, String> {
    if args.is_empty() {
        return Ok(BufferSpec {
            copies: None,
            delim: None,
        });
    }
    let n: u64 = args[0]
        .parse()
        .map_err(|_| format!("BUFFER number required, got {}", args[0]))?;
    let delim = match args.get(1) {
        Some(s) => Some(if is_delimited(s) {
            decode_dstring(s)?
        } else {
            strip_delim(s).as_bytes().to_vec()
        }),
        None => None,
    };
    if args.len() > 2 {
        return Err(format!("unexpected operand {}", args[2]));
    }
    Ok(BufferSpec {
        copies: Some(n),
        delim,
    })
}

fn run_buffer(ctx: &mut StageCtx, spec: BufferSpec) {
    match spec.copies {
        None => {
            let recs = ctx.read_all().unwrap_or_default();
            for r in recs {
                if ctx.write(r).is_err() {
                    break;
                }
            }
        }
        Some(n) => {
            let mut buf = Vec::new();
            while let Ok(Some(r)) = ctx.read() {
                if r.is_empty() {
                    if !write_subfile(ctx, &buf, n, spec.delim.as_deref()) {
                        return;
                    }
                    if ctx.write(Record::empty()).is_err() {
                        return;
                    }
                    buf.clear();
                } else {
                    buf.push(r);
                }
            }
            let _ = write_subfile(ctx, &buf, n, spec.delim.as_deref());
        }
    }
}

fn write_subfile(ctx: &mut StageCtx, buf: &[Record], copies: u64, delim: Option<&[u8]>) -> bool {
    for c in 0..copies {
        if c > 0 {
            let sep = delim.map(Record::from).unwrap_or_else(Record::empty);
            if ctx.write(sep).is_err() {
                return false;
            }
        }
        for r in buf {
            if ctx.write(r.clone()).is_err() {
                return false;
            }
        }
    }
    true
}

enum FanoutStop {
    AllEof,
    AnyEof,
    Count(usize),
}

fn parse_fanout(args: &[String]) -> Result<FanoutStop, String> {
    if args.is_empty() {
        return Ok(FanoutStop::AllEof);
    }
    let mut i = 0;
    let u = args[i].to_ascii_uppercase();
    if u != "STOP" {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    i += 1;
    if i >= args.len() {
        return Ok(FanoutStop::AllEof);
    }
    let u = args[i].to_ascii_uppercase();
    if u == "ALLEOF" {
        Ok(FanoutStop::AllEof)
    } else if u == "ANYEOF" {
        Ok(FanoutStop::AnyEof)
    } else if let Ok(n) = args[i].parse::<usize>() {
        if n == 0 {
            Err("STOP count must be at least 1".into())
        } else {
            Ok(FanoutStop::Count(n))
        }
    } else {
        Err(format!("unrecognized STOP operand {}", args[i]))
    }
}

fn run_fanout(ctx: &mut StageCtx, stop: FanoutStop) {
    loop {
        let Ok(Some(r)) = ctx.read() else {
            break;
        };
        let outs = ctx.connected_outputs();
        if outs.is_empty() {
            break;
        }
        let mut dead = 0usize;
        for s in &outs {
            ctx.select_output(*s);
            if ctx.write(r.clone()).is_err() {
                dead += 1;
            }
        }
        let halt = match stop {
            FanoutStop::AllEof => ctx.connected_outputs().is_empty(),
            FanoutStop::AnyEof => dead > 0 || outs.len() < 2 && !ctx.output_connected(outs[0]),
            FanoutStop::Count(n) => {
                ctx.connected_outputs().len() + dead < outs.len()
                    && (outs.len() - ctx.connected_outputs().len() >= n || dead >= n)
            }
        };
        if halt {
            break;
        }
    }
}

/// BETWEEN — select records between two labels (inclusive).
/// Author's Edition 1.1.11 pp.272–273.
pub fn between(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::between::between(args)
}

/// INSIDE — select records between two labels (exclusive).
/// Author's Edition 1.1.11 p.393.
pub fn inside(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::between::inside(args)
}

/// NOTINSIDE — converse of INSIDE. Synonym NINSIDE.
/// Author's Edition 1.1.11 pp.442–443.
pub fn notinside(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::between::notinside(args)
}

/// OUTSIDE — converse of BETWEEN.
/// Author's Edition 1.1.11 pp.445–446.
pub fn outside(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::between::outside(args)
}

/// SPLIT — break each input record into multiple output records at a target.
/// Author's Edition 1.1.11, pp. 505–506.
pub fn split(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::split::split(args)
}

/// COLLATE — collate master and detail streams. pp.295–297.
pub fn collate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::collate::collate(args)
}

/// MERGE — merge already-ordered streams. pp.436–437.
pub fn merge(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::merge::merge(args)
}

/// DEAL — pass records to output streams. pp.313–316.
pub fn deal(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::deal::deal(args)
}

/// GATHER — copy records from input streams. pp.366–367.
pub fn gather(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::gather::gather(args)
}

pub fn combine(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::combine::combine(args)
}

/// JOIN — put input records together into one output record.
/// Author's Edition 1.1.11, pp. 399–401.
pub fn join(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::join::join(args)
}

/// JOINCONT — join records marked with a continuation string.
/// Author's Edition 1.1.11, pp. 401–403.
pub fn joincont(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::joincont::joincont(args)
}

/// CHOP — truncate records after a column or at a target.
/// Author's Edition 1.1.11, pp. 289–290. Synonym TRUNCATE.
pub fn chop(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::chop::chop(args)
}

/// UNIQUE — discard or retain adjacent duplicate records.
/// Author's Edition 1.1.11, pp. 572–574.
pub fn unique(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::unique::unique(args)
}

/// DUPLICATE — extra copies of each record. Author's Edition 1.1.11, p.334.
pub fn duplicate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::duplicate::duplicate(args)
}

/// PICK — select records that satisfy a relation. Author's Edition 1.1.11, pp.454–456.
pub fn pick(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::pick::pick(args)
}

/// ZONE — run a selection stage on a subset of each input record.
/// Author's Edition 1.1.11, pp.610–611.
pub fn zone(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::zone::zone(args)
}

/// CASEI — run a selection stage case-insensitively.
/// Author's Edition 1.1.11 pp.285–286.
pub fn casei(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::casei::casei(args)
}

/// NOT — run a stage with its output streams inverted.
/// Author's Edition 1.1.11 pp.441–442.
pub fn not_stage(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::not_stage::not_stage(args)
}

/// PREFACE — nested output, then copy the primary input.
/// Author's Edition 1.1.11, pp.461–462.
pub fn preface(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::preface::preface(args)
}

/// APPEND — copy the primary input, then nested output.
/// Author's Edition 1.1.11, pp.263–264.
pub fn append(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::preface::append(args)
}

/// LOOKUP — find records in a reference using a key field.
/// Author's Edition 1.1.11, pp.416–423.
pub fn lookup(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::lookup::lookup(args)
}

/// ELASTIC — stall-avoiding buffer. Author's Edition 1.1.11 p.335.
pub fn elastic(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::elastic::elastic(args)
}

/// GATE — pass records until stopped. Author's Edition 1.1.11 pp.365–366.
pub fn gate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::gate::gate(args)
}

/// IF — run a selection stage in an if-then / if-then-else topology.
/// Author's Edition 1.1.11 pp.389–390.
pub fn if_stage(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::if_stage::if_stage(args)
}

/// DAM — wait for a primary record, then short all streams.
/// Author's Edition 1.1.11 pp.310–311. Converse of GATE.
pub fn dam(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::dam::dam(args)
}

/// JUXTAPOSE — preface the last primary record with each secondary record.
/// Author's Edition 1.1.11 pp.403–405.
pub fn juxtapose(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::juxtapose::juxtapose(args)
}

/// SYNCHRONISE — wait for a record on every input, then copy one from each.
/// Author's Edition 1.1.11 pp.536–539. Synonym SYNCHRONIZE.
pub fn synchronise(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::synchronise::synchronise(args)
}

/// PREDSELECT — route the original record after a destructive test on copies.
/// Author's Edition 1.1.11 pp.459–460.
pub fn predselect(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::predselect::predselect(args)
}

/// NOEOFBACK — pass records and ignore end-of-file on output.
/// Author's Edition 1.1.11 pp.440–441. Synonym NOTEOFBACK.
pub fn noeofback(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::noeofback::noeofback(args)
}

/// EOFBACK — copy through, then run an output device driver so EOF can
/// propagate backwards. Author's Edition 1.1.11 p.337.
pub fn eofback(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::eofback::eofback(args)
}

/// AGGRC — aggregate numeric return codes (most negative, else largest).
/// Author's Edition 1.1.11 pp.258–259.
pub fn aggrc(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::aggrc::aggrc(args)
}

/// DELAY — suspend the stream for a time-of-day or interval.
/// Author's Edition 1.1.11 pp.321–322. Time is the first word of each record.
pub fn delay(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::delay::delay(args)
}

/// BEAT — copy records; write a marker on secondary if none arrive in the interval.
/// Author's Edition 1.1.11 pp.271–272.
pub fn beat(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::beat::beat(args)
}

/// UPDATE — apply a CMS-format UPDATE file from secondary onto the primary master.
/// Author's Edition 1.1.11 pp.576–577.
pub fn update(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::update::update(args)
}

/// PIPCMD — issue pipeline commands from input records.
/// Author's Edition 1.1.11 pp.457–458.
pub fn pipcmd(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::pipcmd::pipcmd(args)
}

/// PIPESTOP — stop stages waiting on an external/time event.
/// Author's Edition 1.1.11 pp.458–459.
pub fn pipestop(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::pipestop::pipestop(args)
}

/// RUNPIPE — issue nested PIPE specifications, intercepting messages.
/// Author's Edition 1.1.11 pp.481–483.
pub fn runpipe(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::runpipe::runpipe(args)
}

/// XLATE — transliterate contents of records.
/// Author's Edition 1.1.11 pp.604–607. Synonym TRANSLATE.
pub fn xlate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::xlate::xlate(args)
}

/// INSERT — insert a string in records.
/// Author's Edition 1.1.11 p.392.
pub fn insert(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::insert::insert(args)
}

/// OVERLAY — overlay data from input streams.
/// Author's Edition 1.1.11 p.447. Verb `OVERlay` min 4.
pub fn overlay(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::overlay::overlay(args)
}

/// PAD — expand short records.
/// Author's Edition 1.1.11 p.451.
pub fn pad(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::pad::pad(args)
}

/// REVERSE — reverse contents of records.
/// Author's Edition 1.1.11 p.474.
pub fn reverse(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::reverse::reverse(args)
}

/// RETAB — replace runs of blanks with tabulate characters.
/// Author's Edition 1.1.11 p.473. Converse of UNTAB.
pub fn retab(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::tab::retab(args)
}

/// UNTAB — replace tabulate characters with blanks.
/// Author's Edition 1.1.11 p.575. Converse of RETAB.
pub fn untab(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::tab::untab(args)
}

/// 64ENCODE — encode a byte stream to MIME Base-64.
/// Author's Edition 1.1.11 pp.613–614.
pub fn encode64(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::mime64::encode(args)
}

/// 64DECODE — decode MIME Base-64 records.
/// Author's Edition 1.1.11 p.613.
pub fn decode64(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::mime64::decode(args)
}

/// QPENCODE — encode records as MIME quoted-printable.
/// Author's Edition 1.1.11 p.467.
pub fn qpencode(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::quoted_printable::encode(args)
}

/// QPDECODE — decode MIME quoted-printable records.
/// Author's Edition 1.1.11 p.466.
pub fn qpdecode(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::quoted_printable::decode(args)
}

/// SNAKE — build a multicolumn page layout.
/// Author's Edition 1.1.11 pp.491–492.
pub fn snake(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::snake::snake(args)
}

/// SPACE — space words like the REXX SPACE built-in.
/// Author's Edition 1.1.11 p.495.
pub fn space(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::space::space(args)
}

/// TOKENISE — split a record into tokens.
/// Author's Edition 1.1.11 p.558. Synonym TOKENIZE.
pub fn tokenise(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::tokenise::tokenise(args)
}

/// FIND — select records that begin with an XEDIT FIND string.
/// Author's Edition 1.1.11 pp.349–350.
pub fn find(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::find::find(args)
}

/// NFIND — converse of FIND (XEDIT NFIND logic).
/// Author's Edition 1.1.11 pp.438–439. Synonym NOTFIND.
pub fn nfind(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::find::nfind(args)
}

/// STRFIND — FIND with a delimitedString argument. pp.349, 534.
pub fn strfind(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::find::strfind(args)
}

/// STRNFIND — NFIND with a delimitedString argument. pp.438, 534.
pub fn strnfind(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::find::strnfind(args)
}

/// FRLABEL — from the first record that begins with the string. p.353.
/// Synonym FROMLABEL.
pub fn frlabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::labels::frlabel(args)
}

/// TOLABEL — up to the first record that begins with the string. p.559.
pub fn tolabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::labels::tolabel(args)
}

/// WHILELABEL — while records begin with the string. p.596.
pub fn whilelabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::labels::whilelabel(args)
}

/// STRFRLABEL — FRLABEL with a delimitedString. pp.353, 534.
/// Synonym STRFROMLABEL.
pub fn strfrlabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::labels::strfrlabel(args)
}

/// STRTOLABEL — TOLABEL with a delimitedString. pp.559, 534.
pub fn strtolabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::labels::strtolabel(args)
}

/// STRWHILELABEL — WHILELABEL with a delimitedString. pp.596, 534.
pub fn strwhilelabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::labels::strwhilelabel(args)
}

/// STRLITERAL — LITERAL with a delimitedString and PREFACE/APPEND/IFEMPTY.
/// Author's Edition 1.1.11 pp.412–413, 534.
pub fn strliteral(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::strliteral::strliteral(args)
}

/// SPILL — spill long lines at word boundaries. pp.502–504.
pub fn spill(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::spill::spill(args)
}

/// FRTARGET — from the first record selected by an argument stage. p.354.
/// Synonym FROMTARGET.
pub fn frtarget(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::target::frtarget(args)
}

/// TOTARGET — through the first record selected by an argument stage. p.560.
pub fn totarget(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::target::totarget(args)
}

/// FANINTWO — two-stream fan-in with secondary priority. pp.340–341.
pub fn fanintwo(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::fanintwo::fanintwo(args)
}

/// FANOUTWO — copy to both outputs; EOF backwards from primary. pp.342–343.
pub fn fanoutwo(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::fanoutwo::fanoutwo(args)
}

/// FILLUP — pass records to output streams as they fill up. p.345.
pub fn fillup(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::fillup::fillup(args)
}

/// THREEWAY — split each record three ways onto three outputs. p.557.
/// Synonym `3WAY`.
pub fn threeway(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::threeway::threeway(args)
}

/// ABBREV — select records whose leading word is an abbreviation.
/// Author's Edition 1.1.11 p.254.
pub fn abbrev(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::abbrev::abbrev(args)
}

/// SUBSTRING — write a substring of each record.
/// Author's Edition 1.1.11 p.536. Verb `SUBSTRing` min 6.
pub fn substring(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::substring::substring(args)
}

/// VERIFY — select records that contain only specified characters.
/// Author's Edition 1.1.11 p.594.
pub fn verify(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::verify::verify(args)
}

/// ALL — select records matching a boolean string expression.
/// Author's Edition 1.1.11 pp.259–260.
pub fn all(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::all::all(args)
}

/// WILDCARD — select records matching a LISTFILE-style pattern.
/// Author's Edition 1.1.11 pp.597–598.
pub fn wildcard(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::wildcard::wildcard(args)
}

/// ESCAPE — insert escape characters before listed characters.
/// Author's Edition 1.1.11 pp.337–338.
pub fn escape(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::escape::escape(args)
}

/// XRANGE — write a range of characters (device driver).
/// Author's Edition 1.1.11 p.609.
pub fn xrange(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::xrange_driver::xrange(args)
}

/// RANDOM — generate pseudo-random numbers.
/// Author's Edition 1.1.11 pp.469–470.
pub fn random(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::random::random(args)
}

/// DIGEST — compute a message digest.
/// Author's Edition 1.1.11 pp.326–327.
pub fn digest(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::digest::digest(args)
}

/// DEBLOCK — deblock external data formats (LINEND / CRLF subset).
/// Author's Edition 1.1.11 pp.317–320.
pub fn deblock(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::deblock::deblock(args)
}

/// PARCEL — parcel a byte stream into records.
/// Author's Edition 1.1.11 p.452.
pub fn parcel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::parcel::parcel(args)
}

/// HTTPSPLIT — split an HTTP stream into headers and body.
/// Author's Edition 1.1.11 p.387.
pub fn httpsplit(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::httpsplit::httpsplit(args)
}

/// URLDEBLOCK — process a universal resource locator into fields.
/// Author's Edition 1.1.11 pp.577–578.
pub fn urldeblock(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::urldeblock::urldeblock(args)
}

/// STEM — REXX stem variables (arrays).
/// Author's Edition 1.1.11 p.528.
pub fn stem(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::stem::stem(args)
}

/// VAR — single variable in a REXX or CLIST pool.
/// Author's Edition 1.1.11 p.580.
pub fn var(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::var::var(args)
}

/// VARSET — set a variable in a REXX or CLIST pool.
/// Author's Edition 1.1.11 p.590.
pub fn varset(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::varset::varset(args)
}

/// VARFETCH — fetch a variable from a REXX or CLIST pool.
/// Author's Edition 1.1.11 p.585.
pub fn varfetch(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::varfetch::varfetch(args)
}

/// REXXVARS — retrieve variables from a REXX or CLIST pool.
/// Author's Edition 1.1.11 p.477.
pub fn rexxvars(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::rexxvars::rexxvars(args)
}

/// VARLOAD — set variables in a REXX or CLIST pool from records.
/// Author's Edition 1.1.11 p.587.
pub fn varload(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::varload::varload(args)
}

/// Test helper: pin DELAY's local time-since-midnight.
pub use crate::delay::{set_test_civil_now_since_midnight, with_civil_now_since_midnight};

/// Collect stage used in tests — not a CMS built-in; available as `>>COLLECT`.
pub fn collect_to(
    sink: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            sink.borrow_mut().push(r.to_string_lossy());
        }
    })
}

pub fn stage_error(msg: impl Into<String>) -> PipeError {
    PipeError::stage(msg)
}
