use std::io::{BufRead, Write};
use std::process::Command;

use pipe_core::{PipeError, Record, StageCtx, StreamId};

/// LITERAL — produce operand words as a single record (joined by spaces).
pub fn literal(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let line = args.join(" ");
        let _ = ctx.write(Record::from(line));
    })
}

/// CONSOLE — write records to stdout.
pub fn console(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        while let Ok(Some(r)) = ctx.read() {
            let _ = writeln!(lock, "{}", r.to_string_lossy());
        }
    })
}

/// DISK — portable line-oriented read/write (stand-in for CMS `disk` / `diskfast`).
///
/// `DISK path` reads lines as records.
/// `DISK OUTPUT path` or `DISK APPEND path` writes records as lines.
pub fn disk(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if args.is_empty() {
            panic!("DISK requires a path");
        }
        let upper: Vec<String> = args.iter().map(|a| a.to_ascii_uppercase()).collect();
        if upper[0] == "OUTPUT" || upper[0] == "APPEND" {
            let path = args.get(1).cloned().unwrap_or_default();
            let append = upper[0] == "APPEND";
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&path)
                .unwrap_or_else(|e| panic!("DISK write {path}: {e}"));
            while let Ok(Some(r)) = ctx.read() {
                let _ = writeln!(f, "{}", r.to_string_lossy());
            }
        } else {
            let path = &args[0];
            let f = std::fs::File::open(path)
                .unwrap_or_else(|e| panic!("DISK read {path}: {e}"));
            let reader = std::io::BufReader::new(f);
            for line in reader.lines() {
                let line = line.unwrap_or_else(|e| panic!("DISK read: {e}"));
                if ctx.write(Record::from(line)).is_err() {
                    break;
                }
            }
        }
    })
}

/// COMMAND — run a shell command; stdout lines become records.
///
/// Portable subset: IBM COMMAND issues CMS/TSO commands into the pipeline.
pub fn command(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let cmd = args.join(" ");
        let output = if cfg!(windows) {
            Command::new("cmd").args(["/C", &cmd]).output()
        } else {
            Command::new("sh").args(["-c", &cmd]).output()
        };
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    if ctx.write(Record::from(line)).is_err() {
                        break;
                    }
                }
            }
            Err(e) => panic!("COMMAND: {e}"),
        }
    })
}

/// COPY — copy records (identity).
pub fn copy(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        while let Ok(Some(r)) = ctx.read() {
            if ctx.write(r).is_err() {
                break;
            }
        }
    })
}

/// LOCATE — pass matching records to primary out; non-matching to secondary.
pub fn locate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let pattern = args
            .first()
            .map(|s| strip_delim(s).to_string())
            .unwrap_or_default();
        while let Ok(Some(r)) = ctx.read() {
            let text = r.to_string_lossy();
            if text.contains(&pattern) {
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(r).is_err() {
                    break;
                }
            } else {
                ctx.select_output(StreamId::SECONDARY);
                // Secondary may be null-ended; write discards if so.
                let _ = ctx.write(r);
            }
        }
    })
}

/// CHANGE — replace first occurrence of old with new in each record.
pub fn change(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let old = args
            .first()
            .map(|s| strip_delim(s).to_string())
            .unwrap_or_default();
        let new = args
            .get(1)
            .map(|s| strip_delim(s).to_string())
            .unwrap_or_default();
        while let Ok(Some(r)) = ctx.read() {
            let text = r.to_string_lossy().replacen(&old, &new, 1);
            if ctx.write(Record::from(text)).is_err() {
                break;
            }
        }
    })
}

/// COUNT — emit a single record with the number of input records.
pub fn count(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        let mut n = 0u64;
        while let Ok(Some(_)) = ctx.read() {
            n += 1;
        }
        let _ = ctx.write(Record::from(n.to_string()));
    })
}

/// SORT — buffer all records, sort lexicographically, write out.
pub fn sort(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        let mut recs = ctx.read_all().unwrap_or_default();
        recs.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        for r in recs {
            if ctx.write(r).is_err() {
                break;
            }
        }
    })
}

/// BUFFER — read all, then write all (explicit delay point).
pub fn buffer(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        let recs = ctx.read_all().unwrap_or_default();
        for r in recs {
            if ctx.write(r).is_err() {
                break;
            }
        }
    })
}

/// FANOUT — copy each input record to all connected output streams (0..n).
pub fn fanout(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        while let Ok(Some(r)) = ctx.read() {
            // Write to primary and secondary; further streams if selected.
            for s in [StreamId::PRIMARY, StreamId::SECONDARY] {
                ctx.select_output(s);
                let _ = ctx.write(r.clone());
            }
        }
    })
}

/// FANIN — read primary to EOF, then secondary, etc.
pub fn fanin(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        for s in [StreamId::PRIMARY, StreamId::SECONDARY] {
            ctx.select_input(s);
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r).is_err() {
                    return;
                }
            }
        }
    })
}

/// FANINANY — round-robin style: keep reading whichever stream has data.
/// Simplified: drain primary then secondary like FANIN (good enough for v1 tests).
pub fn faninany(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    fanin(args)
}

/// HOLE — consume and discard all input.
pub fn hole(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        while let Ok(Some(_)) = ctx.read() {}
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

/// NLOCATE — pass records that do not contain the string.
///
/// Hits go to the secondary output. No operand (a null string) selects only
/// empty records. Column ranges, ANYOF, ANYCASE, and bit masks are not implemented.
pub fn nlocate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let pattern = args.first().map(|s| strip_delim(s).to_string());
        while let Ok(Some(r)) = ctx.read() {
            let text = r.to_string_lossy();
            let select = match pattern.as_deref() {
                None | Some("") => text.is_empty(),
                Some(p) => !text.contains(p),
            };
            if select {
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(r).is_err() {
                    break;
                }
            } else {
                ctx.select_output(StreamId::SECONDARY);
                let _ = ctx.write(r);
            }
        }
    })
}

/// STRIP — remove leading and/or trailing characters.
///
/// Default is both ends, blank. `STRING` and hex ranges are rejected.
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
    limit: Option<usize>,
}

fn parse_strip(args: &[String]) -> Result<StripSpec, String> {
    let mut side = None;
    let mut anycase = false;
    let mut negate = false;
    let mut target = None;
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
        } else if matches!(
            upper.as_str(),
            "ANYCASE" | "CASEANY" | "IGNORECASE" | "CASEIGNORE"
        ) {
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
        } else if upper == "ANYOF" {
            if target.is_some() {
                return Err("duplicate target".into());
            }
            let next = args.get(i + 1).ok_or("ANYOF requires a string")?;
            target = Some(strip_delim(next).to_string());
            i += 2;
        } else if upper == "STRING" || upper == "XRANGE" {
            return Err(format!("{upper} is not implemented"));
        } else if looks_like_xrange(a) {
            return Err("xrange is not implemented".into());
        } else if let Ok(n) = a.parse::<usize>() {
            if limit.is_some() {
                return Err("duplicate count".into());
            }
            limit = Some(n);
            i += 1;
        } else if is_char_target(a) {
            if target.is_some() {
                return Err("duplicate target".into());
            }
            target = Some(strip_delim(a).to_string());
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
        limit,
    })
}

fn is_char_target(s: &str) -> bool {
    let inner = strip_delim(s);
    if inner != s {
        // Delimited string, including an empty pair.
        return true;
    }
    inner.chars().count() == 1
}

fn looks_like_xrange(s: &str) -> bool {
    let Some((left, right)) = s.split_once('-') else {
        return false;
    };
    !left.is_empty() && !right.is_empty() && !right.contains('-')
}

fn apply_strip(input: &str, spec: &StripSpec) -> String {
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

/// SPEC subset — rearrange record contents (Appendix A keyword `SPECS` → `SPEC`).
///
/// Supported forms (simplified):
/// - `SPEC 1-* 1` — copy whole record
/// - `SPEC WORD 1 1` — first blank-delimited word to output column 1 (emits word)
/// - `SPEC /literal/ 1` — insert literal
pub fn spec(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            let out = apply_spec(&r.to_string_lossy(), &args);
            if ctx.write(Record::from(out)).is_err() {
                break;
            }
        }
    })
}

fn apply_spec(input: &str, args: &[String]) -> String {
    if args.is_empty() {
        return input.to_string();
    }
    // Subset: WORD n, FIELD n (same as WORD for blank-delimited),
    // delimited literals, 1-* / * (whole record), FS= / ranges like 1.3
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let au = a.to_ascii_uppercase();
        if au == "WORD" || au == "FIELD" {
            let n: usize = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(1);
            let words: Vec<&str> = input.split_whitespace().collect();
            parts.push(
                words
                    .get(n.saturating_sub(1))
                    .copied()
                    .unwrap_or("")
                    .to_string(),
            );
            i += 2;
            if i < args.len() && args[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
            }
        } else if au == "WORDS" || au == "FIELDS" {
            // WORDS n-m → join words n through m
            let range = args.get(i + 1).cloned().unwrap_or_else(|| "1-*".into());
            let words: Vec<&str> = input.split_whitespace().collect();
            parts.push(select_range_words(&words, &range));
            i += 2;
            if i < args.len() && args[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
            }
        } else if a.starts_with('/') || a.starts_with('%') {
            parts.push(strip_delim(a).to_string());
            i += 1;
            if i < args.len() && args[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
            }
        } else if au == "1-*" || au == "*" || au == "1-*" || a == "1" {
            parts.push(input.to_string());
            i += 1;
            if i < args.len() && args[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
            }
        } else if let Some((start, len)) = parse_byte_range(a) {
            let bytes = input.as_bytes();
            let start = start.saturating_sub(1);
            let slice = if start < bytes.len() {
                let end = (start + len).min(bytes.len());
                String::from_utf8_lossy(&bytes[start..end]).into_owned()
            } else {
                String::new()
            };
            parts.push(slice);
            i += 1;
            if i < args.len() && args[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
            }
        } else {
            parts.push(a.clone());
            i += 1;
        }
    }
    if parts.is_empty() {
        input.to_string()
    } else {
        parts.join("")
    }
}

fn select_range_words(words: &[&str], range: &str) -> String {
    if range == "*" || range == "1-*" {
        return words.join(" ");
    }
    if let Some((a, b)) = range.split_once('-') {
        let start: usize = a.parse::<usize>().unwrap_or(1).saturating_sub(1);
        let end = if b == "*" {
            words.len()
        } else {
            b.parse::<usize>().unwrap_or(words.len())
        };
        return words
            .get(start..end.min(words.len()))
            .unwrap_or(&[])
            .join(" ");
    }
    let n: usize = range.parse::<usize>().unwrap_or(1).saturating_sub(1);
    words.get(n).copied().unwrap_or("").to_string()
}

/// Parse CMS-like `start.length` byte range (1-based start).
fn parse_byte_range(s: &str) -> Option<(usize, usize)> {
    let (a, b) = s.split_once('.')?;
    let start = a.parse().ok()?;
    let len = b.parse().ok()?;
    Some((start, len))
}

fn strip_delim(s: &str) -> &str {
    if s.len() >= 2 {
        let b = s.as_bytes();
        let first = b[0];
        let last = b[b.len() - 1];
        if first == last && matches!(first, b'/' | b'%' | b'$' | b'#' | b'!') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// BETWEEN — select records between two targets or from a target for a count.
///
/// Records from the first match up to and including the second match (or count)
/// go to primary output. Records outside the range go to secondary output.
pub fn between(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_between(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("BETWEEN: {e}"));
                return;
            }
        };
        
        let mut selecting = false;
        let mut count_remaining = 0u64;
        
        while let Ok(Some(r)) = ctx.read() {
            let text = r.to_string_lossy();
            let text_cmp = if spec.anycase {
                text.to_ascii_uppercase()
            } else {
                text.to_string()
            };
            
            if !selecting {
                if matches_pattern(&text_cmp, &spec.start_pattern, spec.anycase) {
                    selecting = true;
                    match spec.end_spec {
                        BetweenEnd::Pattern(_) => {},
                        BetweenEnd::Count(n) => count_remaining = n,
                    }
                    ctx.select_output(StreamId::PRIMARY);
                    if ctx.write(r).is_err() {
                        break;
                    }
                    
                    if let BetweenEnd::Count(1) = spec.end_spec {
                        selecting = false;
                    } else if let BetweenEnd::Count(_) = spec.end_spec {
                        count_remaining -= 1;
                        if count_remaining == 0 {
                            selecting = false;
                        }
                    }
                } else {
                    ctx.select_output(StreamId::SECONDARY);
                    let _ = ctx.write(r);
                }
            } else {
                let should_end = match &spec.end_spec {
                    BetweenEnd::Pattern(p) => matches_pattern(&text_cmp, p, spec.anycase),
                    BetweenEnd::Count(_) => {
                        count_remaining -= 1;
                        count_remaining == 0
                    }
                };
                
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(r).is_err() {
                    break;
                }
                
                if should_end {
                    selecting = false;
                }
            }
        }
    })
}

enum BetweenEnd {
    Pattern(String),
    Count(u64),
}

struct BetweenSpec {
    start_pattern: String,
    end_spec: BetweenEnd,
    anycase: bool,
}

fn parse_between(args: &[String]) -> Result<BetweenSpec, String> {
    if args.is_empty() {
        return Err("requires two operands".into());
    }
    
    let mut i = 0;
    let mut anycase = false;
    
    // Check for ANYCASE before the patterns
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "ANYCASE" {
            anycase = true;
            i += 1;
        }
    }
    
    if i >= args.len() {
        return Err("requires two operands".into());
    }
    
    let start_pattern = strip_delim(&args[i]).to_string();
    i += 1;
    
    if i >= args.len() {
        return Err("requires two operands".into());
    }
    
    let end_spec = if let Ok(n) = args[i].parse::<u64>() {
        if n == 0 {
            return Err("count must be at least 1".into());
        }
        BetweenEnd::Count(n)
    } else {
        BetweenEnd::Pattern(strip_delim(&args[i]).to_string())
    };
    i += 1;
    
    if i < args.len() {
        return Err("too many operands".into());
    }
    
    Ok(BetweenSpec {
        start_pattern: if anycase {
            start_pattern.to_ascii_uppercase()
        } else {
            start_pattern
        },
        end_spec: match end_spec {
            BetweenEnd::Pattern(p) => BetweenEnd::Pattern(if anycase {
                p.to_ascii_uppercase()
            } else {
                p
            }),
            BetweenEnd::Count(n) => BetweenEnd::Count(n),
        },
        anycase,
    })
}

fn matches_pattern(text: &str, pattern: &str, _anycase: bool) -> bool {
    text.contains(pattern)
}

/// SPLIT — break each input record into multiple output records at a target.
/// Author's Edition 1.1.11, pp. 505–506.
pub fn split(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    crate::split::split(args)
}

/// Collect stage used in tests — not a CMS built-in; available as `>>COLLECT`.
pub fn collect_to(sink: std::rc::Rc<std::cell::RefCell<Vec<String>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            sink.borrow_mut().push(r.to_string_lossy());
        }
    })
}

pub fn stage_error(msg: impl Into<String>) -> PipeError {
    PipeError::stage(msg)
}
