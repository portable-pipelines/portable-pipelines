//! VERIFY — select records that contain only specified characters.
//!
//! Author's Edition 1.1.11 p.594.
//!
//! Syntax (diagram all-caps):
//! `VERIFY [ANYCASE] [inputRange] delimitedString`
//!
//! The delimited string enumerates allowed characters (a character list, not
//! an xrange token). A null input range, or a range whose every byte is in
//! that set, is selected. Similar to REXX `VERIFY()`.

use pipe_core::{StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase, is_delimited};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, InputRange,
};
use crate::xrange::ByteSet;

pub fn verify(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_verify(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("VERIFY: {e}"));
                return;
            }
        };
        run_selection(ctx, "VERIFY", |rec| is_verified(rec, &spec));
    })
}

struct VerifySpec {
    range: Option<InputRange>,
    allowed: ByteSet,
}

fn parse_verify(args: &[String]) -> Result<VerifySpec, String> {
    let mut i = 0;
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    let mut range = None;
    if i < args.len() && looks_like_input_range_start(&args[i]) {
        range = Some(parse_input_range(args, &mut i)?);
    }

    if i >= args.len() {
        return Err("a delimited string is required".into());
    }
    if !is_delimited(&args[i]) {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let chars = decode_dstring(&args[i])?;
    i += 1;
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(VerifySpec {
        range,
        allowed: ByteSet::from_bytes(chars, anycase),
    })
}

fn is_verified(record: &[u8], spec: &VerifySpec) -> bool {
    let bytes = match &spec.range {
        Some(range) => extract_input_range(record, range),
        None => record.to_vec(),
    };
    // Null range (missing or empty) is selected (p.594).
    bytes.iter().all(|&b| spec.allowed.contains(b))
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

    fn spec(args: &[&str]) -> VerifySpec {
        parse_verify(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn digits_in_a_column_range() {
        // p.594: `verify 5.5 /0123456789/`
        let s = spec(&["5.5", "/0123456789/"]);
        assert!(is_verified(b"xxxx12345", &s));
        assert!(!is_verified(b"xxxx12a45", &s));
        // Missing range is null → selected.
        assert!(is_verified(b"ab", &s));
    }

    #[test]
    fn entire_record_and_null() {
        let s = spec(&["/abc/"]);
        assert!(is_verified(b"abca", &s));
        assert!(is_verified(b"", &s));
        assert!(!is_verified(b"abd", &s));
    }

    #[test]
    fn anycase_expands_the_character_list() {
        let s = spec(&["ANYCASE", "/abc/"]);
        assert!(is_verified(b"AbC", &s));
        assert!(!is_verified(b"AbCd", &s));
        let sensitive = spec(&["/abc/"]);
        assert!(!is_verified(b"A", &sensitive));
    }

    #[test]
    fn hyphen_in_the_dstring_is_a_character_not_xrange() {
        let s = spec(&["/0-9/"]);
        assert!(is_verified(b"0-9", &s));
        assert!(is_verified(b"90", &s));
        assert!(!is_verified(b"5", &s));
    }
}
