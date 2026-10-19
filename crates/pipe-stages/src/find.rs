//! FIND / NFIND — select records by XEDIT FIND / NFIND logic.
//!
//! Author's Edition 1.1.11 pp.349–350 (FIND) and pp.438–439 (NFIND).
//! STRFIND / STRNFIND: family p.534; ANYCASE on the FIND/NFIND railroads.
//!
//! Syntax (diagrams all-caps; no verb prefix):
//! `FIND [string]`
//! `NFIND [string]`  (synonym `NOTFIND`)
//! `STRFIND [ANYcase] delimitedString`
//! `STRNFIND [ANYcase] delimitedString`
//!
//! FIND/NFIND: the string starts after exactly one blank; leading and
//! trailing blanks are significant. It is **not** a delimitedString.
//! STRFIND/STRNFIND take a required dstring (pair / hex / binary, p.194)
//! and optional ANYCASE (synonyms CASEANY / IGNORECASE / CASEIGNORE).
//!
//! XEDIT matching (tabs 1, image off, case mixed respect): a null string
//! matches any record; a blank in the string is a don't-care that must
//! still be present; an underscore requires a blank; other characters
//! must equal the corresponding position (ANYCASE ignores case). Matching
//! is a prefix of the record, not a substring (unlike LOCATE).

use pipe_core::{StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase, is_delimited};

pub fn find(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    selection("FIND", args, false)
}

pub fn nfind(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    selection("NFIND", args, true)
}

pub fn strfind(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    str_selection("STRFIND", args, false)
}

pub fn strnfind(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    str_selection("STRNFIND", args, true)
}

fn selection(
    verb: &'static str,
    args: Vec<String>,
    invert: bool,
) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let pattern = pattern_from_args(&args);
        run_selection(ctx, verb, &pattern, false, invert);
    })
}

fn str_selection(
    verb: &'static str,
    args: Vec<String>,
    invert: bool,
) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_str_find(&args) {
        Ok((pattern, anycase)) => run_selection(ctx, verb, &pattern, anycase, invert),
        Err(e) => ctx.fail(format!("{verb}: {e}")),
    })
}

fn parse_str_find(args: &[String]) -> Result<(Vec<u8>, bool), String> {
    let mut i = 0;
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }
    if i >= args.len() {
        return Err("a delimited string is required".into());
    }
    if !is_delimited(&args[i]) {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let pattern = decode_dstring(&args[i])?;
    i += 1;
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }
    Ok((pattern, anycase))
}

fn run_selection(ctx: &mut StageCtx, verb: &str, pattern: &[u8], anycase: bool, invert: bool) {
    if ctx.input_connected(StreamId::SECONDARY) {
        ctx.fail(format!(
            "{verb}: secondary input stream must not be connected"
        ));
        return;
    }
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::SECONDARY.0 {
            ctx.fail(format!(
                "{verb}: no input stream other than the primary may be connected"
            ));
            return;
        }
    }

    if !outputs_live(ctx) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    run_find(ctx, pattern, anycase, invert);
}

fn pattern_from_args(args: &[String]) -> Vec<u8> {
    match args {
        [] => Vec::new(),
        [one] => one.as_bytes().to_vec(),
        many => many.join(" ").into_bytes(),
    }
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

fn run_find(ctx: &mut StageCtx, pattern: &[u8], anycase: bool, invert: bool) {
    loop {
        if !outputs_live(ctx) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let hit = xedit_find_match(rec.as_bytes(), pattern, anycase);
            let select = if invert { !hit } else { hit };
            let stream = if select {
                StreamId::PRIMARY
            } else {
                StreamId::SECONDARY
            };
            if ctx.output_connected(stream) {
                ctx.select_output(stream);
                let write_failed = ctx.write(rec).is_err();
                let _ = ctx.read();
                if write_failed && !outputs_live(ctx) {
                    return;
                }
            } else {
                let _ = ctx.read();
            }
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

/// XEDIT FIND match: prefix of `record` against `pattern` (p.350).
///
/// Portable ASCII: blank is X'20'; underscore is X'5F'. The book describes
/// the same rules on EBCDIC (blank X'40', underscore X'6D'). `anycase`
/// ignores case for characters other than blank and underscore (STRFIND).
fn xedit_find_match(record: &[u8], pattern: &[u8], anycase: bool) -> bool {
    if pattern.is_empty() {
        return true;
    }
    if record.len() < pattern.len() {
        return false;
    }
    for (i, &p) in pattern.iter().enumerate() {
        if p == b' ' {
            continue;
        }
        if p == b'_' {
            if record[i] != b' ' {
                return false;
            }
            continue;
        }
        if anycase {
            if !eq_ignore_ascii_case_byte(record[i], p) {
                return false;
            }
        } else if record[i] != p {
            return false;
        }
    }
    true
}

fn eq_ignore_ascii_case_byte(a: u8, b: u8) -> bool {
    a.eq_ignore_ascii_case(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_string_matches_any_record() {
        assert!(xedit_find_match(b"", b"", false));
        assert!(xedit_find_match(b"abc", b"", false));
    }

    #[test]
    fn blank_is_dont_care_but_required() {
        assert!(xedit_find_match(b"abc", b"a c", false));
        assert!(xedit_find_match(b"axc", b"a c", false));
        assert!(!xedit_find_match(b"ab", b"a c", false));
        assert!(!xedit_find_match(b"", b" ", false));
        assert!(xedit_find_match(b"x", b" ", false));
    }

    #[test]
    fn underscore_requires_a_blank() {
        assert!(xedit_find_match(b"a b", b"a_b", false));
        assert!(!xedit_find_match(b"axb", b"a_b", false));
        assert!(!xedit_find_match(b"ab", b"a_b", false));
    }

    #[test]
    fn other_characters_are_case_sensitive_prefix() {
        assert!(xedit_find_match(b"abc", b"ab", false));
        assert!(xedit_find_match(b"abc", b"abc", false));
        assert!(!xedit_find_match(b"Abc", b"abc", false));
        assert!(!xedit_find_match(b"xabc", b"abc", false));
        assert!(!xedit_find_match(b"ab", b"abc", false));
    }

    #[test]
    fn anycase_ignores_letter_case_not_blank_or_underscore() {
        assert!(xedit_find_match(b"Abc", b"abc", true));
        assert!(xedit_find_match(b"aXc", b"a c", true));
        assert!(xedit_find_match(b"A b", b"a_b", true));
        assert!(!xedit_find_match(b"Axb", b"a_b", true));
    }

    #[test]
    fn pattern_from_one_or_many_args() {
        assert_eq!(pattern_from_args(&[]), b"");
        assert_eq!(pattern_from_args(&["a c".into()]), b"a c");
        assert_eq!(pattern_from_args(&["a".into(), "c".into()]), b"a c");
        assert_eq!(pattern_from_args(&["a c ".into()]), b"a c ");
    }
}
