//! SUBSTRING — write a substring of each record.
//!
//! Author's Edition 1.1.11 p.536.
//!
//! Syntax: `SUBSTRing inputRange` (min 6).
//!
//! Note 1: an optimisation for a special case of SPEC. A missing range writes
//! a null record (one output per input).

use pipe_core::{Record, StageCtx, StreamId};

use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, InputRange,
};

pub fn substring(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let range = match parse_substring(&args) {
            Ok(range) => range,
            Err(e) => {
                ctx.fail(format!("SUBSTRING: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("SUBSTRING: no input stream other than the primary may be connected");
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
        run_substring(ctx, &range);
    })
}

fn parse_substring(args: &[String]) -> Result<InputRange, String> {
    let mut i = 0;
    if i >= args.len() {
        return Err("inputRange is required".into());
    }
    if !looks_like_input_range_start(&args[i]) {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let range = parse_input_range(args, &mut i)?;
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }
    Ok(range)
}

fn run_substring(ctx: &mut StageCtx, range: &InputRange) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = extract_input_range(rec.as_bytes(), range);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn range(args: &[&str]) -> InputRange {
        parse_substring(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn columns_and_missing() {
        assert_eq!(extract_input_range(b"abcdef", &range(&["2-4"])), b"bcd");
        assert_eq!(extract_input_range(b"abcdef", &range(&["1-3"])), b"abc");
        assert_eq!(extract_input_range(b"ab", &range(&["10"])), b"");
        assert_eq!(extract_input_range(b"ab", &range(&["1-5"])), b"ab");
    }

    #[test]
    fn word_and_nested_substring() {
        let rec = b"What a rum fellow.";
        assert_eq!(extract_input_range(rec, &range(&["word", "3"])), b"rum");
        assert_eq!(
            extract_input_range(rec, &range(&["substring", "2", "of", "word", "3"])),
            b"u"
        );
    }
}
