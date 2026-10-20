//! ESCAPE — insert escape characters in front of listed characters.
//!
//! Author's Edition 1.1.11 pp.337–338.
//!
//! Syntax (diagram all-caps): `ESCAPE [delimitedString]`
//!
//! The first character of the string is the escape character; every character
//! in the string (including the escape itself) is prefixed with it. Default
//! `/"\|/` — double quote, backslant, vertical bar.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_delimited};
use crate::xrange::ByteSet;

/// Default argument: quote, backslash (backslant), vertical bar (p.338).
const DEFAULT: &[u8] = b"\"\\|";

pub fn escape(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let chars = match parse_escape(&args) {
            Ok(chars) => chars,
            Err(e) => {
                ctx.fail(format!("ESCAPE: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("ESCAPE: no input stream other than the primary may be connected");
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
        run_escape(ctx, &chars);
    })
}

fn parse_escape(args: &[String]) -> Result<Vec<u8>, String> {
    if args.is_empty() {
        return Ok(DEFAULT.to_vec());
    }
    if !is_delimited(&args[0]) {
        return Err(format!("unrecognized operand {}", args[0]));
    }
    let chars = decode_dstring(&args[0])?;
    if chars.is_empty() {
        return Err("the delimited string must contain an escape character".into());
    }
    if args.len() > 1 {
        return Err(format!("unexpected operand {}", args[1]));
    }
    Ok(chars)
}

fn escape_record(record: &[u8], chars: &[u8]) -> Vec<u8> {
    let esc = chars[0];
    let set = ByteSet::from_bytes(chars.iter().copied(), false);
    let mut out = Vec::with_capacity(record.len() * 2);
    for &b in record {
        if set.contains(b) {
            out.push(esc);
        }
        out.push(b);
    }
    out
}

fn run_escape(ctx: &mut StageCtx, chars: &[u8]) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = escape_record(rec.as_bytes(), chars);
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

    fn spec(args: &[&str]) -> Vec<u8> {
        parse_escape(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn book_default_doubles_quotes() {
        // p.338: `This: "a beautiful day"` → `This: ""a beautiful day""`.
        let chars = spec(&[]);
        assert_eq!(
            escape_record(br#"This: "a beautiful day""#, &chars),
            br#"This: ""a beautiful day"""#.to_vec()
        );
    }

    #[test]
    fn default_also_escapes_backslash_and_bar() {
        let chars = spec(&[]);
        assert_eq!(escape_record(br"a\b|c", &chars), br#"a"\b"|c"#.to_vec());
    }

    #[test]
    fn listed_characters_are_prefixed_with_the_first() {
        let chars = spec(&["/l/"]);
        assert_eq!(escape_record(b"hello", &chars), b"hellllo".to_vec());
    }

    #[test]
    fn hex_dstring() {
        let chars = spec(&["x6122"]); // 'a' then quote; escape is 'a'
        assert_eq!(escape_record(b"a\"", &chars), b"aaa\"".to_vec());
    }

    #[test]
    fn null_dstring_is_rejected() {
        assert!(parse_escape(&["//".into()])
            .unwrap_err()
            .contains("escape character"));
    }
}
