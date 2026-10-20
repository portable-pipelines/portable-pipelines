//! ABBREV — select records whose leading word is an abbreviation.
//!
//! Author's Edition 1.1.11 p.254.
//!
//! Syntax (diagram all-caps; nested optionals):
//! `ABBREV [word [number [ANYCASE]]]`
//!
//! Leading columns up to the first blank (or end of record) must be a prefix
//! of `word` and at least `number` characters (REXX `ABBREV`). Default word
//! is null; default number is 0.

use pipe_core::{StageCtx, StreamId};

use crate::common::is_anycase;
use crate::xrange::parse_column_number;

pub fn abbrev(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_abbrev(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("ABBREV: {e}"));
                return;
            }
        };
        run_selection(ctx, "ABBREV", |rec| is_abbrev(rec, &spec));
    })
}

#[derive(Debug)]
struct AbbrevSpec {
    word: Vec<u8>,
    min: usize,
    anycase: bool,
}

fn parse_abbrev(args: &[String]) -> Result<AbbrevSpec, String> {
    if args.is_empty() {
        return Ok(AbbrevSpec {
            word: Vec::new(),
            min: 0,
            anycase: false,
        });
    }

    let mut i = 0;
    // First token is the word even when it looks like a number or ANYCASE.
    let word = args[i].as_bytes().to_vec();
    i += 1;

    let mut min = 0usize;
    let mut anycase = false;
    if i < args.len() {
        if is_anycase(&args[i]) {
            return Err("ANYCASE must follow a number".into());
        }
        min = parse_min_count(&args[i])?;
        i += 1;
    }
    if i < args.len() {
        if is_anycase(&args[i]) {
            anycase = true;
            i += 1;
        } else {
            return Err(format!("unexpected operand {}", args[i]));
        }
    }
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }
    Ok(AbbrevSpec { word, min, anycase })
}

fn parse_min_count(s: &str) -> Result<usize, String> {
    match parse_column_number(s)? {
        Some(n) => Ok(n as usize),
        None => Err(format!("minimum count requires a number, got {s}")),
    }
}

/// Bytes from column 1 through the character before the first blank.
///
/// A leading blank yields a null abbreviation (Note 2). Portable ASCII blank
/// is X'20' (book cites EBCDIC X'40').
fn leading_word(record: &[u8]) -> &[u8] {
    match record.iter().position(|&b| b == b' ') {
        Some(i) => &record[..i],
        None => record,
    }
}

fn is_abbrev(record: &[u8], spec: &AbbrevSpec) -> bool {
    let info = leading_word(record);
    if info.len() < spec.min || info.len() > spec.word.len() {
        return false;
    }
    let prefix = &spec.word[..info.len()];
    if spec.anycase {
        info.eq_ignore_ascii_case(prefix)
    } else {
        info == prefix
    }
}

fn run_selection(ctx: &mut StageCtx, verb: &str, selected: impl Fn(&[u8]) -> bool) {
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
    loop {
        if !outputs_live(ctx) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let stream = if selected(rec.as_bytes()) {
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

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> AbbrevSpec {
        parse_abbrev(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn book_min_three() {
        // p.254: `abbrev string 3` keeps `str` and `string`, not `st` / `strings`.
        assert!(!is_abbrev(b"st", &spec(&["string", "3"])));
        assert!(is_abbrev(b"str", &spec(&["string", "3"])));
        assert!(is_abbrev(b"string", &spec(&["string", "3"])));
        assert!(!is_abbrev(b"strings", &spec(&["string", "3"])));
    }

    #[test]
    fn default_min_zero_keeps_empty_and_leading_blank() {
        // Note 2 / default number 0.
        assert!(is_abbrev(b"", &spec(&["string"])));
        assert!(is_abbrev(b" extra", &spec(&["string"])));
        assert!(is_abbrev(b"s", &spec(&["string"])));
        assert!(!is_abbrev(b"x", &spec(&["string"])));
    }

    #[test]
    fn null_word_selects_null_or_leading_blank() {
        assert!(is_abbrev(b"", &spec(&[])));
        assert!(is_abbrev(b" leading", &spec(&[])));
        assert!(!is_abbrev(b"a", &spec(&[])));
    }

    #[test]
    fn stops_at_the_first_blank() {
        assert!(is_abbrev(b"str extra", &spec(&["string", "3"])));
        assert!(!is_abbrev(b"st extra", &spec(&["string", "3"])));
    }

    #[test]
    fn anycase_folds_ascii_letters() {
        assert!(!is_abbrev(b"STR", &spec(&["string", "3"])));
        assert!(is_abbrev(b"STR", &spec(&["string", "3", "ANYCASE"])));
        assert!(is_abbrev(b"String", &spec(&["STRING", "3", "CASEANY"])));
    }

    #[test]
    fn anycase_without_number_is_rejected() {
        let err = parse_abbrev(&["string".into(), "ANYCASE".into()]).unwrap_err();
        assert!(err.contains("ANYCASE"), "{err}");
    }
}
