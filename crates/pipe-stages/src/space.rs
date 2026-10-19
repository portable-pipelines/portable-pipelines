//! SPACE — space words like the REXX SPACE built-in.
//!
//! Author's Edition 1.1.11 p.495.
//!
//! Syntax (fixed railroad order; diagram all-caps verb):
//! `SPACE [number] [xorc | [STRing] delimitedString] [xorc | [ANYof] delimitedString]`
//!
//! Leading and trailing delimiter characters are removed. Internal runs of
//! delimiter characters are replaced by `number` copies of the replacement
//! string (default one blank). Default delimiter is a single blank.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_delimited, matches_abbrev};
use crate::xrange::{try_xorc, ByteSet};

pub fn space(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_space(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("SPACE: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("SPACE: no input stream other than the primary may be connected");
                return;
            }
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);
        run_space(ctx, &spec);
    })
}

#[derive(Clone, Debug)]
struct SpaceSpec {
    count: usize,
    replacement: Vec<u8>,
    delimiters: ByteSet,
}

fn parse_space(args: &[String]) -> Result<SpaceSpec, String> {
    let mut i = 0;

    // [number] — unsigned; a leading sign is not a count (railroad is `number`).
    let mut count = 1usize;
    let mut count_explicit = false;
    if i < args.len() && is_number_token(&args[i]) {
        count = parse_nonneg_number(&args[i], "number")?;
        count_explicit = true;
        i += 1;
    }

    let mut replacement = vec![b' '];
    let mut delimiters = blank_set();

    if i >= args.len() {
        return Ok(SpaceSpec {
            count,
            replacement,
            delimiters,
        });
    }

    // Note 2 p.495: one string argument is the replacement unless ANYOF is
    // specified or the count is zero (then that string is the delimiter).
    if is_anyof(&args[i]) {
        i += 1;
        delimiters = delimiter_from_dstring(args, &mut i)?;
    } else if count_explicit && count == 0 && is_single_bare_string(&args[i..]) {
        delimiters = take_bare_delimiter(&args[i])?;
        i += 1;
    } else {
        replacement = take_replacement(args, &mut i)?;
        if i < args.len() {
            if is_anyof(&args[i]) {
                i += 1;
                delimiters = delimiter_from_dstring(args, &mut i)?;
            } else if is_string(&args[i]) {
                return Err("STRING must appear as the replacement, before the delimiter".into());
            } else if is_number_token(&args[i]) && args[i].len() > 2 {
                return Err("number must appear first".into());
            } else {
                delimiters = take_bare_delimiter(&args[i])?;
                i += 1;
            }
        }
    }

    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    Ok(SpaceSpec {
        count,
        replacement,
        delimiters,
    })
}

fn is_number_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn parse_nonneg_number(s: &str, what: &str) -> Result<usize, String> {
    if !is_number_token(s) {
        return Err(format!("{what} requires a nonnegative number, got {s}"));
    }
    let n = s
        .parse::<i64>()
        .map_err(|_| format!("{what} {s} is out of range"))?;
    if n < 0 || n > i32::MAX as i64 {
        return Err(format!("{what} must be 0 through {}, got {s}", i32::MAX));
    }
    Ok(n as usize)
}

fn is_string(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "STRING", 3)
}

fn is_anyof(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "ANYOF", 3)
}

fn is_single_bare_string(rest: &[String]) -> bool {
    rest.len() == 1 && !is_string(&rest[0]) && !is_anyof(&rest[0]) && is_xorc_or_dstring(&rest[0])
}

fn is_xorc_or_dstring(s: &str) -> bool {
    is_delimited(s) || try_xorc(s).is_some()
}

fn take_replacement(args: &[String], i: &mut usize) -> Result<Vec<u8>, String> {
    if *i >= args.len() {
        return Ok(vec![b' ']);
    }
    if is_string(&args[*i]) {
        *i += 1;
        if *i >= args.len() {
            return Err("STRING requires a delimited string".into());
        }
        if !is_delimited(&args[*i]) {
            return Err(format!(
                "STRING requires a delimited string, got {}",
                args[*i]
            ));
        }
        let bytes = decode_dstring(&args[*i])?;
        *i += 1;
        return Ok(bytes);
    }
    if is_anyof(&args[*i]) {
        return Err("ANYOF is the delimiter keyword".into());
    }
    if is_delimited(&args[*i]) {
        let bytes = decode_dstring(&args[*i])?;
        *i += 1;
        return Ok(bytes);
    }
    if let Some(b) = try_xorc(&args[*i]) {
        *i += 1;
        return Ok(vec![b]);
    }
    Err(unexpected_operand(&args[*i]))
}

fn delimiter_from_dstring(args: &[String], i: &mut usize) -> Result<ByteSet, String> {
    if *i >= args.len() {
        return Err("ANYOF requires a delimited string".into());
    }
    if !is_delimited(&args[*i]) {
        return Err(format!(
            "ANYOF requires a delimited string, got {}",
            args[*i]
        ));
    }
    let bytes = decode_dstring(&args[*i])?;
    *i += 1;
    Ok(ByteSet::from_bytes(bytes, false))
}

fn take_bare_delimiter(arg: &str) -> Result<ByteSet, String> {
    if is_delimited(arg) {
        return Ok(ByteSet::from_bytes(decode_dstring(arg)?, false));
    }
    if let Some(b) = try_xorc(arg) {
        let mut set = ByteSet::new();
        set.insert(b);
        return Ok(set);
    }
    Err(unexpected_operand(arg))
}

fn unexpected_operand(arg: &str) -> String {
    let u = arg.to_ascii_uppercase();
    if is_number_token(arg) {
        "number must appear first".into()
    } else if is_string(arg) {
        "STRING must appear as the replacement, before the delimiter".into()
    } else if is_anyof(arg) {
        "ANYOF must appear with the delimiter".into()
    } else if u == "PAD"
        || u == "BOTH"
        || u == "LEADING"
        || u == "TRAILING"
        || u == "LEFT"
        || u == "RIGHT"
        || u == "ANYCASE"
        || u == "NOT"
        || u == "FIRST"
        || u == "LAST"
    {
        format!("unexpected operand {arg}")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn blank_set() -> ByteSet {
    let mut set = ByteSet::new();
    set.insert(b' ');
    set
}

fn run_space(ctx: &mut StageCtx, spec: &SpaceSpec) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = apply_space(rec.as_bytes(), spec);
            if ctx.write(Record::from(out)).is_err() {
                return;
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

fn apply_space(record: &[u8], spec: &SpaceSpec) -> Vec<u8> {
    let mut words: Vec<&[u8]> = Vec::new();
    let mut i = 0;
    while i < record.len() {
        while i < record.len() && spec.delimiters.contains(record[i]) {
            i += 1;
        }
        if i >= record.len() {
            break;
        }
        let start = i;
        i += 1;
        while i < record.len() && !spec.delimiters.contains(record[i]) {
            i += 1;
        }
        words.push(&record[start..i]);
    }
    if words.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (k, word) in words.iter().enumerate() {
        if k > 0 {
            for _ in 0..spec.count {
                out.extend_from_slice(&spec.replacement);
            }
        }
        out.extend_from_slice(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<SpaceSpec, String> {
        parse_space(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn apply(record: &str, args: &[&str]) -> String {
        let spec = parse(args).unwrap();
        String::from_utf8(apply_space(record.as_bytes(), &spec)).unwrap()
    }

    #[test]
    fn default_collapses_blank_runs() {
        assert_eq!(apply("        a   b    b    ", &[]), "a b b");
        assert_eq!(apply("a", &[]), "a");
        assert_eq!(apply("", &[]), "");
        assert_eq!(apply("     ", &[]), "");
    }

    #[test]
    fn book_example_zero_count_delimiters() {
        // p.495: `space 0 /,.=/` — count 0, one string is the delimiter (Note 2).
        assert_eq!(apply("3.6=7,3", &["0", "/,.=/"]), "3673");
    }

    #[test]
    fn blank_xorc_is_default_replacement() {
        assert_eq!(apply("  a   b  ", &["BLANK"]), "a b");
        assert_eq!(apply("  a   b  ", &["SPACE"]), "a b");
    }

    #[test]
    fn count_repeats_replacement() {
        assert_eq!(apply("a   b", &["2"]), "a  b");
        assert_eq!(apply("a   b", &["0"]), "ab");
        assert_eq!(apply("a   b", &["3", "*"]), "a***b");
    }

    #[test]
    fn note2_one_string_is_replacement_unless_anyof_or_count_zero() {
        assert_eq!(apply("a b", &["/-/"]), "a-b");
        assert_eq!(apply("a.b,c", &["ANYOF", "/.,/"]), "a b c");
        assert_eq!(apply("a*b**c", &["0", "*"]), "abc");
        // Two strings: first replacement, second delimiter, even when count is 0.
        assert_eq!(apply("a.b,c", &["0", "/-/", "/.,/"]), "abc");
        assert_eq!(apply("a.b,c", &["1", "/-/", "/.,/"]), "a-b-c");
    }

    #[test]
    fn note1_numeric_xorc_needs_a_count() {
        // `space 40` is count 40, not xorc X'40'.
        let spec = parse(&["40"]).unwrap();
        assert_eq!(spec.count, 40);
        assert_eq!(spec.replacement, b" ");
        // `space 1 40` is count 1, replacement xorc X'40' (ASCII '@'; book X'40' is EBCDIC blank).
        let spec = parse(&["1", "40"]).unwrap();
        assert_eq!(spec.count, 1);
        assert_eq!(spec.replacement, vec![0x40]);
        assert_eq!(apply("a b", &["1", "2a"]), "a*b");
    }

    #[test]
    fn string_and_anyof_keywords() {
        assert_eq!(apply("a b", &["STRING", "/--/"]), "a--b");
        assert_eq!(apply("a b", &["STR", "/--/"]), "a--b");
        assert_eq!(apply("a,b.c", &["STRING", "/-/", "ANYOF", "/,./"]), "a-b-c");
        assert_eq!(apply("a,b", &["2", "ANY", "/,/"]), "a  b");
        let e = parse(&["ST", "/--/"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["AN", "/,/"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        let e = parse(&["STRING"]).unwrap_err();
        assert!(e.contains("STRING requires a delimited string"), "{e}");
        let e = parse(&["ANYOF"]).unwrap_err();
        assert!(e.contains("ANYOF requires a delimited string"), "{e}");
        let e = parse(&["STRING", "*"]).unwrap_err();
        assert!(e.contains("STRING requires a delimited string"), "{e}");
        let e = parse(&["ANYOF", "*"]).unwrap_err();
        assert!(e.contains("ANYOF requires a delimited string"), "{e}");
    }

    #[test]
    fn xorc_delimiter_and_tabulate() {
        assert_eq!(apply("a,b,,c", &["1", "-", ","]), "a-b-c");
        assert_eq!(apply("a\tb\t\tc", &["1", "-", "TAB"]), "a-b-c");
    }

    #[test]
    fn empty_replacement_and_empty_delimiter() {
        assert_eq!(apply("a   b", &["//"]), "ab");
        assert_eq!(apply("a   b", &["1", "//", "BLANK"]), "ab");
        // Empty delimiter set: nothing is stripped or replaced.
        assert_eq!(apply(" a b ", &["1", "/-/", "//"]), " a b ");
    }

    #[test]
    fn invented_operands_rejected() {
        for w in [
            "PAD", "BOTH", "LEADING", "TRAILING", "LEFT", "RIGHT", "ANYCASE", "NOT", "FIRST",
        ] {
            let e = parse(&[w]).unwrap_err();
            assert!(e.contains("unexpected operand"), "{w}: {e}");
        }
        let e = parse(&["/-/", "2", "PAD"]).unwrap_err();
        assert!(e.contains("unexpected") || e.contains("must appear"), "{e}");
    }

    #[test]
    fn number_must_come_first() {
        let e = parse(&["ANYOF", "/,/", "2"]).unwrap_err();
        assert!(e.contains("number must appear first"), "{e}");
        let e = parse(&["STRING", "/-/", "STRING", "/x/"]).unwrap_err();
        assert!(e.contains("STRING"), "{e}");
    }

    #[test]
    fn parse_xorc_helper_is_reused() {
        assert_eq!(crate::xrange::parse_xorc("BLANK").unwrap(), b' ');
        assert_eq!(crate::xrange::parse_xorc("SPACE").unwrap(), b' ');
        assert_eq!(crate::xrange::parse_xorc("2a").unwrap(), b'*');
    }
}
