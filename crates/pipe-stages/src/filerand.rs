//! FILERAND — read selected records or ranges from a host file.
//!
//! z/VM CMS Pipelines Reference (FILERAND). The operand is one host path
//! (not CMS `fn ft fm`), then optional `RECNO` / `NUMBER` and `rrange`
//! tokens. When not first, input records also carry ranges (blank-separated).
//! Null and blank input records are ignored.
//!
//! Implemented `rrange` forms: `n`, `n-m`, `n-*`, `*-m`, `*-*`, `n.count`.
//! `BLOCKED` (F-format blocked read) is rejected. The file is indexed in
//! memory. Record numbers are 1-based. A missing file is an error.

use pipe_core::{Record, StageCtx, StreamId};

use crate::file_io::{file_path, load_host_records};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Range {
    /// Inclusive 1-based start and end. End is `None` until the file length
    /// is known (`*` on that side).
    Span { start: Bound, end: Bound },
    /// `n.count` records beginning at `start`.
    Count { start: u64, count: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    Rec(u64),
    Last,
}

#[derive(Clone, Debug)]
struct Spec {
    path: String,
    recno: bool,
    ranges: Vec<Range>,
}

pub fn filerand(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_filerand(ctx, "FILERAND", args))
}

pub fn diskrand(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_filerand(ctx, "DISKRAND", args))
}

fn run_filerand(ctx: &mut StageCtx, verb: &str, args: Vec<String>) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail(format!("{verb}: only the primary input may be connected"));
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
    let spec = match parse_filerand(&args, verb) {
        Ok(s) => s,
        Err(e) => {
            ctx.fail(format!("{verb}: {e}"));
            return;
        }
    };
    let recs = match load_host_records(&spec.path) {
        Ok(r) => r,
        Err(e) => {
            ctx.fail(format!("file read {}: {e}", spec.path));
            return;
        }
    };

    ctx.select_output(StreamId::PRIMARY);
    for range in &spec.ranges {
        if !write_range(ctx, &recs, *range, spec.recno, verb) {
            return;
        }
    }

    if !ctx.input_connected(StreamId::PRIMARY) {
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                let line = rec.to_string_lossy();
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let extra = match parse_rrange_list(line) {
                    Ok(r) => r,
                    Err(e) => {
                        ctx.fail(format!("{verb}: {e}"));
                        return;
                    }
                };
                for range in extra {
                    if !write_range(ctx, &recs, range, spec.recno, verb) {
                        return;
                    }
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

fn write_range(
    ctx: &mut StageCtx,
    recs: &[Vec<u8>],
    range: Range,
    recno: bool,
    verb: &str,
) -> bool {
    let n = recs.len() as u64;
    let nums = match resolve_range(range, n) {
        Ok(v) => v,
        Err(e) => {
            ctx.fail(format!("{verb}: {e}"));
            return false;
        }
    };
    for num in nums {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return false;
        }
        if num < 1 || num > n {
            ctx.fail(format!("{verb}: record {num} is not in the file"));
            return false;
        }
        let body = &recs[(num - 1) as usize];
        let out = if recno {
            recno_record(num, body)
        } else {
            body.clone()
        };
        if ctx.write(Record::from(out)).is_err() {
            return false;
        }
    }
    true
}

fn recno_record(num: u64, body: &[u8]) -> Vec<u8> {
    let label = format!("{num:>10}");
    let mut out = label.into_bytes();
    out.extend_from_slice(body);
    out
}

fn resolve_range(range: Range, n: u64) -> Result<Vec<u64>, String> {
    match range {
        Range::Count { start, count } => {
            if start < 1 {
                return Err("record numbers start at 1".into());
            }
            Ok((0..count).map(|i| start + i).collect())
        }
        Range::Span { start, end } => {
            let s = match start {
                Bound::Rec(v) => v,
                Bound::Last => 1,
            };
            let e = match end {
                Bound::Rec(v) => v,
                Bound::Last => n,
            };
            if s < 1 {
                return Err("record numbers start at 1".into());
            }
            if n == 0 {
                return Ok(Vec::new());
            }
            if e < s {
                return Err(format!("range {s}-{e} ends before it starts"));
            }
            Ok((s..=e).collect())
        }
    }
}

fn parse_filerand(args: &[String], verb: &str) -> Result<Spec, String> {
    let path = file_path(args, verb)?;
    let mut recno = false;
    let mut ranges = Vec::new();
    for tok in args.iter().skip(1) {
        let u = tok.to_ascii_uppercase();
        if u == "RECNO" || u == "NUMBER" {
            recno = true;
            continue;
        }
        if matches_abbrev(&u, "BLOCKED", 5) {
            return Err("BLOCKED is not implemented (CMS F-format only)".into());
        }
        ranges.push(parse_rrange(tok)?);
    }
    Ok(Spec {
        path,
        recno,
        ranges,
    })
}

fn parse_rrange_list(s: &str) -> Result<Vec<Range>, String> {
    let mut out = Vec::new();
    for tok in s.split_whitespace() {
        out.push(parse_rrange(tok)?);
    }
    Ok(out)
}

fn parse_rrange(tok: &str) -> Result<Range, String> {
    if let Some((left, right)) = tok.split_once('.') {
        if !right.is_empty()
            && right.bytes().all(|b| b.is_ascii_digit())
            && parse_bound(left).is_ok()
        {
            let start = match parse_bound(left)? {
                Bound::Rec(n) => n,
                Bound::Last => {
                    return Err(format!("unrecognized record range {tok}"));
                }
            };
            let count: u64 = right
                .parse()
                .map_err(|_| format!("unrecognized record range {tok}"))?;
            return Ok(Range::Count { start, count });
        }
    }
    if let Some((left, right)) = tok.split_once('-') {
        return Ok(Range::Span {
            start: parse_bound(left)?,
            end: parse_bound(right)?,
        });
    }
    Ok(Range::Span {
        start: parse_bound(tok)?,
        end: parse_bound(tok)?,
    })
}

fn parse_bound(s: &str) -> Result<Bound, String> {
    if s == "*" {
        return Ok(Bound::Last);
    }
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("unrecognized record range {s}"));
    }
    let n: u64 = s
        .parse()
        .map_err(|_| format!("unrecognized record range {s}"))?;
    if n == 0 {
        return Err("record numbers start at 1".into());
    }
    Ok(Bound::Rec(n))
}

fn matches_abbrev(key: &str, full: &str, min_len: usize) -> bool {
    key.len() >= min_len && key.len() <= full.len() && full.starts_with(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrange_forms() {
        assert_eq!(
            parse_rrange("3").unwrap(),
            Range::Span {
                start: Bound::Rec(3),
                end: Bound::Rec(3)
            }
        );
        assert_eq!(
            parse_rrange("20-30").unwrap(),
            Range::Span {
                start: Bound::Rec(20),
                end: Bound::Rec(30)
            }
        );
        assert_eq!(
            parse_rrange("15.2").unwrap(),
            Range::Count {
                start: 15,
                count: 2
            }
        );
        assert_eq!(
            parse_rrange("4-*").unwrap(),
            Range::Span {
                start: Bound::Rec(4),
                end: Bound::Last
            }
        );
        assert_eq!(
            parse_rrange("*-2").unwrap(),
            Range::Span {
                start: Bound::Last,
                end: Bound::Rec(2)
            }
        );
        assert_eq!(
            parse_rrange("*-*").unwrap(),
            Range::Span {
                start: Bound::Last,
                end: Bound::Last
            }
        );
    }

    #[test]
    fn resolve_star_and_count() {
        assert_eq!(
            resolve_range(
                Range::Span {
                    start: Bound::Rec(2),
                    end: Bound::Last
                },
                4
            )
            .unwrap(),
            vec![2, 3, 4]
        );
        assert_eq!(
            resolve_range(
                Range::Count {
                    start: 15,
                    count: 2
                },
                20
            )
            .unwrap(),
            vec![15, 16]
        );
        assert_eq!(
            resolve_range(
                Range::Span {
                    start: Bound::Last,
                    end: Bound::Last
                },
                0
            )
            .unwrap(),
            Vec::<u64>::new()
        );
    }

    #[test]
    fn recno_is_ten_columns() {
        let out = recno_record(3, b"hello");
        assert_eq!(&out[..10], b"         3");
        assert_eq!(&out[10..], b"hello");
    }
}
