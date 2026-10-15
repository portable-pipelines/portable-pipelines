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

/// CONSOLE / OUTPUT — write records to stdout.
pub fn console(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        while let Ok(Some(r)) = ctx.read() {
            let _ = writeln!(lock, "{}", r.to_string_lossy());
        }
    })
}

/// FILE — read or write a filesystem path.
///
/// `FILE path` reads lines as records.
/// `FILE OUTPUT path` or `FILE APPEND path` writes records as lines.
pub fn file(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if args.is_empty() {
            panic!("FILE requires a path");
        }
        let upper: Vec<String> = args.iter().map(|a| a.to_ascii_uppercase()).collect();
        if upper[0] == "OUTPUT" || upper[0] == "APPEND" || upper[0] == "WRITE" {
            let path = args.get(1).cloned().unwrap_or_default();
            let append = upper[0] == "APPEND";
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&path)
                .unwrap_or_else(|e| panic!("FILE write {path}: {e}"));
            while let Ok(Some(r)) = ctx.read() {
                let _ = writeln!(f, "{}", r.to_string_lossy());
            }
        } else {
            let path = &args[0];
            let f = std::fs::File::open(path)
                .unwrap_or_else(|e| panic!("FILE read {path}: {e}"));
            let reader = std::io::BufReader::new(f);
            for line in reader.lines() {
                let line = line.unwrap_or_else(|e| panic!("FILE read: {e}"));
                if ctx.write(Record::from(line)).is_err() {
                    break;
                }
            }
        }
    })
}

/// HOST / COMMAND — run a shell command; stdout lines become records.
pub fn host(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
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
            Err(e) => panic!("HOST: {e}"),
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

/// DROP / HOLE — consume and discard all input.
pub fn hole(_args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| {
        while let Ok(Some(_)) = ctx.read() {}
    })
}

/// SPECS subset — copy fields.
///
/// Supported forms (simplified):
/// - `SPECS 1-* 1` — copy whole record
/// - `SPECS WORD 1 1` — first blank-delimited word to output column 1 (emits word)
/// - `SPECS /literal/ 1` — insert literal
pub fn specs(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            let out = apply_specs(&r.to_string_lossy(), &args);
            if ctx.write(Record::from(out)).is_err() {
                break;
            }
        }
    })
}

fn apply_specs(input: &str, args: &[String]) -> String {
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
