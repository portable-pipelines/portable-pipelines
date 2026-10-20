//! SPILL — Author's Edition 1.1.11 pp.502–504.
//!
//! Syntax (diagram all-caps; no verb prefix):
//! `SPILL number [ [STRing] dstring | [NOT] ANYof dstring ]
//!       [ANYcase] [KEEP] [OFFSET number|dstring]`
//!
//! Filter. Splits long records at word boundaries. Default separator is a
//! single blank. Optional secondary output receives the remainder when a word
//! cannot be split within the width.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase, is_delimited, matches_abbrev};
use crate::xrange::{parse_positive_usize, ByteSet};

pub fn spill(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_spill(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("SPILL: {e}"));
                return;
            }
        };
        run_spill(ctx, spec);
    })
}

#[derive(Clone, Debug)]
enum Separator {
    /// Multi-character STRING (or the default single blank).
    String(Vec<u8>),
    /// ANYOF / NOT ANYOF character set.
    Set { bytes: ByteSet, not: bool },
}

#[derive(Clone, Debug)]
struct SpillSpec {
    width: usize,
    sep: Separator,
    anycase: bool,
    keep: bool,
    offset: Vec<u8>,
}

fn parse_spill(args: &[String]) -> Result<SpillSpec, String> {
    if args.is_empty() {
        return Err("a positive number is required".into());
    }
    let mut i = 0;
    let width = parse_positive_usize(&args[i], "number")?;
    i += 1;

    let mut sep = Separator::String(vec![b' ']);
    if i < args.len() {
        if is_string_kw(&args[i]) {
            i += 1;
            sep = Separator::String(take_dstring(args, &mut i, "STRING")?);
        } else if is_not(&args[i]) {
            i += 1;
            if i < args.len() && is_anyof(&args[i]) {
                i += 1;
            } else if i > 0 && is_not_anyof(&args[i - 1]) {
                // `NOTANYOF` as one token already consumed as NOT; ANYOF missing.
                return Err("NOT requires ANYOF".into());
            } else {
                return Err("NOT requires ANYOF".into());
            }
            let bytes = ByteSet::from_bytes(take_dstring(args, &mut i, "ANYOF")?, false);
            sep = Separator::Set { bytes, not: true };
        } else if is_not_anyof(&args[i]) {
            i += 1;
            let bytes = ByteSet::from_bytes(take_dstring(args, &mut i, "ANYOF")?, false);
            sep = Separator::Set { bytes, not: true };
        } else if is_anyof(&args[i]) {
            i += 1;
            let bytes = ByteSet::from_bytes(take_dstring(args, &mut i, "ANYOF")?, false);
            sep = Separator::Set { bytes, not: false };
        } else if is_delimited(&args[i]) {
            sep = Separator::String(decode_dstring(&args[i])?);
            i += 1;
        }
    }

    let mut anycase = false;
    let mut keep = false;
    let mut offset = Vec::new();
    let mut offset_set = false;
    while i < args.len() {
        // Remaining ANYCASE is `ANYcase` (min 3). ANYOF lives only in the
        // separator position, so `any` here is ANYCASE, not ANYOF.
        if is_anycase(&args[i]) {
            if anycase {
                return Err("ANYCASE specified twice".into());
            }
            anycase = true;
            i += 1;
        } else if args[i].eq_ignore_ascii_case("KEEP") {
            if keep {
                return Err("KEEP specified twice".into());
            }
            keep = true;
            i += 1;
        } else if args[i].eq_ignore_ascii_case("OFFSET") {
            if offset_set {
                return Err("OFFSET specified twice".into());
            }
            i += 1;
            offset = take_offset(args, &mut i)?;
            offset_set = true;
        } else {
            return Err(format!("unexpected operand {}", args[i]));
        }
    }

    if anycase {
        sep = apply_anycase(sep);
    }

    Ok(SpillSpec {
        width,
        sep,
        anycase,
        keep,
        offset,
    })
}

fn is_string_kw(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "STRING", 3)
}

fn is_anyof(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "ANYOF", 3)
}

fn is_not(s: &str) -> bool {
    s.eq_ignore_ascii_case("NOT")
}

/// Blanks optional between NOT and ANYOF (p.502 Note 1).
fn is_not_anyof(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    u == "NOTANYOF" || u == "NOTANY"
}

fn take_dstring(args: &[String], i: &mut usize, what: &str) -> Result<Vec<u8>, String> {
    if *i >= args.len() {
        return Err(format!("{what} requires a delimited string"));
    }
    if !is_delimited(&args[*i]) {
        return Err(format!(
            "{what} requires a delimited string, got {}",
            args[*i]
        ));
    }
    let bytes = decode_dstring(&args[*i])?;
    *i += 1;
    Ok(bytes)
}

fn take_offset(args: &[String], i: &mut usize) -> Result<Vec<u8>, String> {
    if *i >= args.len() {
        return Err("OFFSET requires a number or a delimited string".into());
    }
    if args[*i].bytes().all(|b| b.is_ascii_digit()) && !args[*i].is_empty() {
        let n = parse_nonneg(&args[*i])?;
        *i += 1;
        return Ok(vec![b' '; n]);
    }
    if is_delimited(&args[*i]) {
        let bytes = decode_dstring(&args[*i])?;
        *i += 1;
        return Ok(bytes);
    }
    Err(format!(
        "OFFSET requires a number or a delimited string, got {}",
        args[*i]
    ))
}

fn parse_nonneg(s: &str) -> Result<usize, String> {
    let n = s
        .parse::<i64>()
        .map_err(|_| format!("OFFSET {s} is out of range"))?;
    if n < 0 || n > i32::MAX as i64 {
        return Err(format!("OFFSET must be 0 through {}, got {s}", i32::MAX));
    }
    Ok(n as usize)
}

fn apply_anycase(sep: Separator) -> Separator {
    match sep {
        Separator::String(s) => Separator::String(s),
        Separator::Set { bytes, not } => Separator::Set {
            bytes: bytes.with_anycase(),
            not,
        },
    }
}

fn extra_input_connected(ctx: &StageCtx) -> bool {
    ctx.connected_inputs()
        .iter()
        .any(|&s| s != StreamId::PRIMARY)
}

fn run_spill(ctx: &mut StageCtx, spec: SpillSpec) {
    if extra_input_connected(ctx) {
        ctx.fail("SPILL: no input stream other than the primary may be connected");
        return;
    }

    let secondary = ctx.output_connected(StreamId::SECONDARY);
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.short_input();
            return;
        }
        if secondary && !ctx.output_connected(StreamId::SECONDARY) {
            ctx.short_input();
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let (primary, rest) = spill_record(rec.as_bytes(), &spec, secondary);
            for piece in primary {
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(Record::from(piece)).is_err() {
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.short_input();
                    return;
                }
            }
            if let Some(sec) = rest {
                if ctx.output_connected(StreamId::SECONDARY) {
                    ctx.select_output(StreamId::SECONDARY);
                    if ctx.write(Record::from(sec)).is_err() {
                        ctx.select_input(StreamId::PRIMARY);
                        ctx.short_input();
                        return;
                    }
                }
            }
            let _ = ctx.read();
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

/// Split one record. Secondary remainder is `Some` only when the secondary
/// stream is in use and a word cannot be split within the width.
fn spill_record(
    bytes: &[u8],
    spec: &SpillSpec,
    secondary: bool,
) -> (Vec<Vec<u8>>, Option<Vec<u8>>) {
    if bytes.len() <= spec.width {
        return (vec![bytes.to_vec()], None);
    }

    let mut rest = bytes.to_vec();
    let mut primary = Vec::new();
    let mut first = true;
    let later_limit = spec.width.saturating_sub(spec.offset.len());

    while rest.len() > current_limit(first, spec.width, later_limit) {
        let limit = current_limit(first, spec.width, later_limit);
        if limit == 0 {
            if secondary {
                let wrote = !primary.is_empty();
                return (primary, Some(prefix_offset(wrote, spec, rest)));
            }
            // No room for continuation content: split within the word at `width`.
            let n = spec.width.min(rest.len());
            let piece = rest[..n].to_vec();
            rest = rest[n..].to_vec();
            if !piece.is_empty() {
                primary.push(with_offset(first, spec, piece));
            }
            first = false;
            continue;
        }
        if let Some((pos, seplen)) = rightmost_sep(&rest, limit, spec) {
            let (piece, new_rest) = split_at(&rest, pos, seplen, spec.keep);
            rest = new_rest;
            if !piece.is_empty() {
                primary.push(with_offset(first, spec, piece));
            }
            first = false;
        } else if secondary {
            let wrote = !primary.is_empty();
            return (primary, Some(prefix_offset(wrote, spec, rest)));
        } else {
            let n = limit.min(rest.len());
            let piece = rest[..n].to_vec();
            rest = rest[n..].to_vec();
            if !piece.is_empty() {
                primary.push(with_offset(first, spec, piece));
            }
            first = false;
        }
    }

    if !rest.is_empty() || primary.is_empty() {
        primary.push(with_offset(first, spec, rest));
    }
    (primary, None)
}

fn current_limit(first: bool, width: usize, later: usize) -> usize {
    if first {
        width
    } else {
        later
    }
}

fn with_offset(first: bool, spec: &SpillSpec, piece: Vec<u8>) -> Vec<u8> {
    if first || spec.offset.is_empty() {
        piece
    } else {
        let mut out = spec.offset.clone();
        out.extend_from_slice(&piece);
        out
    }
}

fn prefix_offset(wrote_primary: bool, spec: &SpillSpec, rest: Vec<u8>) -> Vec<u8> {
    if wrote_primary && !spec.offset.is_empty() {
        let mut out = spec.offset.clone();
        out.extend_from_slice(&rest);
        out
    } else {
        rest
    }
}

fn split_at(data: &[u8], pos: usize, seplen: usize, keep: bool) -> (Vec<u8>, Vec<u8>) {
    let end = if keep { pos + seplen } else { pos };
    let piece = data[..end.min(data.len())].to_vec();
    let rest = data[(pos + seplen).min(data.len())..].to_vec();
    (piece, rest)
}

/// Rightmost separator whose start is within the width or abutting it on the
/// right (start == `limit`).
fn rightmost_sep(data: &[u8], limit: usize, spec: &SpillSpec) -> Option<(usize, usize)> {
    let mut best = None;
    let max_start = limit.min(data.len().saturating_sub(1));
    for start in 0..=max_start {
        if let Some(len) = match_sep(data, start, spec) {
            best = Some((start, len));
        }
    }
    // Abutting: separator begins at `limit` (first byte past the window).
    if limit < data.len() {
        if let Some(len) = match_sep(data, limit, spec) {
            best = Some((limit, len));
        }
    }
    best
}

fn match_sep(data: &[u8], start: usize, spec: &SpillSpec) -> Option<usize> {
    match &spec.sep {
        Separator::String(needle) => {
            if needle.is_empty() || start + needle.len() > data.len() {
                return None;
            }
            let window = &data[start..start + needle.len()];
            let hit = if spec.anycase {
                window.eq_ignore_ascii_case(needle)
            } else {
                window == needle.as_slice()
            };
            if hit {
                Some(needle.len())
            } else {
                None
            }
        }
        Separator::Set { bytes, not } => {
            let b = *data.get(start)?;
            if bytes.contains(b) != *not {
                Some(1)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_blank(width: usize) -> SpillSpec {
        SpillSpec {
            width,
            sep: Separator::String(vec![b' ']),
            anycase: false,
            keep: false,
            offset: Vec::new(),
        }
    }

    fn texts(parts: &[Vec<u8>]) -> Vec<String> {
        parts
            .iter()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect()
    }

    #[test]
    fn paragraph_example() {
        // p.504: `spill 12` on the flowed paragraph.
        let (p, s) = spill_record(b"This is a paragraph to be flowed.", &spec_blank(12), false);
        assert_eq!(texts(&p), vec!["This is a", "paragraph to", "be flowed."]);
        assert!(s.is_none());
    }

    #[test]
    fn long_word_without_secondary() {
        // p.504: split inside the word when secondary is not defined.
        let (p, s) = spill_record(b"abcdefghi klmnopq", &spec_blank(4), false);
        assert_eq!(texts(&p), vec!["abcd", "efgh", "i", "klmn", "opq"]);
        assert!(s.is_none());
    }

    #[test]
    fn long_word_with_secondary() {
        let (p, s) = spill_record(b"abc defghi klmn opq", &spec_blank(4), true);
        assert_eq!(texts(&p), vec!["abc"]);
        assert_eq!(String::from_utf8_lossy(&s.unwrap()), "defghi klmn opq");
    }

    #[test]
    fn offset_prefixes_secondary_after_a_primary_write() {
        let mut spec = spec_blank(4);
        spec.offset = vec![b' '; 2];
        let (p, s) = spill_record(b"abc defghi klmn", &spec, true);
        assert_eq!(texts(&p), vec!["abc"]);
        assert_eq!(String::from_utf8_lossy(&s.unwrap()), "  defghi klmn");
    }

    #[test]
    fn offset_on_numbered_list() {
        let mut spec = spec_blank(14);
        spec.offset = vec![b' '; 4];
        let (p, s) = spill_record(b"2. A most important item.", &spec, false);
        assert_eq!(texts(&p), vec!["2. A most", "    important", "    item."]);
        assert!(s.is_none());
    }

    #[test]
    fn abutting_separator_on_the_right() {
        // Split point may abut the width (start == limit).
        let (p, s) = spill_record(b"hello world", &spec_blank(5), false);
        assert_eq!(texts(&p), vec!["hello", "world"]);
        assert!(s.is_none());
    }
}
