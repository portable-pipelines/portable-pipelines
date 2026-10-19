//! AGGRC — Author's Edition 1.1.11 pp.258–259 (host-command note p.39).
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `AGGRC`
//!
//! Filter. Reads records that each contain one signed 32-bit number, delays
//! every record until end-of-file, then writes a single record with the
//! aggregate return code: the most negative number if any input is negative,
//! otherwise the largest. A nonnegative output is unsigned (no plus sign).
//! When the output record cannot be written (the stage is last), the
//! aggregate becomes this stage's return code. No output if there is no input.

use pipe_core::{aggregate_return_codes, Record, StageCtx, StreamId};

pub fn aggrc(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_aggrc(&args) {
            ctx.fail(format!("AGGRC: {e}"));
            return;
        }
        run_aggrc(ctx);
    })
}

fn parse_aggrc(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_aggrc(ctx: &mut StageCtx) {
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    let mut codes: Vec<i32> = Vec::new();
    loop {
        match ctx.read() {
            Ok(Some(r)) => match parse_return_code_record(r.as_bytes()) {
                Ok(n) => codes.push(n),
                Err(e) => {
                    ctx.fail(format!("AGGRC: {e}"));
                    return;
                }
            },
            Ok(None) => break,
            Err(_) => return,
        }
    }

    // Note 2 p.259: no output if there is no input.
    if codes.is_empty() {
        return;
    }

    let agg = aggregate_return_codes(codes);
    let rec = Record::from(format_return_code(agg));
    if ctx.output_connected(StreamId::PRIMARY) {
        if ctx.write(rec).is_err() {
            // Output was connected, then could not be written (p.259).
            ctx.set_return_code(agg);
        }
    } else {
        // Last stage: the output record cannot be written (p.258–259).
        ctx.set_return_code(agg);
    }
}

/// One number in −2147483648…2147483647. A leading plus sign is ignored (p.258).
fn parse_return_code_record(bytes: &[u8]) -> Result<i32, String> {
    let t = trim_ascii(bytes);
    if t.is_empty() {
        return Err("input record is not a number".into());
    }
    let t = match t.strip_prefix(b"+") {
        Some(rest) => {
            if rest.is_empty() || rest[0] == b'+' || rest[0] == b'-' {
                return Err("input record is not a number".into());
            }
            rest
        }
        None => t,
    };
    let s = std::str::from_utf8(t).map_err(|_| "input record is not a number".to_string())?;
    if !is_rc_number_body(s) {
        return Err("input record is not a number".into());
    }
    s.parse::<i32>()
        .map_err(|_| format!("{s} is not in the interval -2147483648 to 2147483647"))
}

fn is_rc_number_body(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let mut s = bytes;
    while s.first().is_some_and(u8::is_ascii_whitespace) {
        s = &s[1..];
    }
    while s.last().is_some_and(u8::is_ascii_whitespace) {
        s = &s[..s.len() - 1];
    }
    s
}

/// Nonnegative numbers are unsigned (p.258): no plus sign, no pad.
fn format_return_code(n: i32) -> String {
    n.to_string()
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn leading_plus_is_ignored() {
        assert_eq!(parse_return_code_record(b"+6").unwrap(), 6);
        assert_eq!(parse_return_code_record(b"+0").unwrap(), 0);
        assert_eq!(parse_return_code_record(b"+2147483647").unwrap(), i32::MAX);
    }

    #[test]
    fn surrounding_blanks_are_trimmed() {
        assert_eq!(parse_return_code_record(b"  99\t").unwrap(), 99);
        assert_eq!(parse_return_code_record(b" -3 \n").unwrap(), -3);
    }

    #[test]
    fn interval_bounds() {
        assert_eq!(parse_return_code_record(b"-2147483648").unwrap(), i32::MIN);
        assert_eq!(parse_return_code_record(b"2147483647").unwrap(), i32::MAX);
        assert!(parse_return_code_record(b"2147483648").is_err());
        assert!(parse_return_code_record(b"-2147483649").is_err());
    }

    #[test]
    fn plus_then_sign_is_not_a_number() {
        assert!(parse_return_code_record(b"+-3").is_err());
        assert!(parse_return_code_record(b"++6").is_err());
        assert!(parse_return_code_record(b"+").is_err());
    }

    #[test]
    fn not_a_single_number() {
        assert!(parse_return_code_record(b"").is_err());
        assert!(parse_return_code_record(b"   ").is_err());
        assert!(parse_return_code_record(b"0 99").is_err());
        assert!(parse_return_code_record(b"abc").is_err());
        assert!(parse_return_code_record(b"3.0").is_err());
        assert!(parse_return_code_record(b"6x").is_err());
    }

    #[test]
    fn nonnegative_format_is_unsigned() {
        assert_eq!(format_return_code(0), "0");
        assert_eq!(format_return_code(99), "99");
        assert_eq!(format_return_code(-3), "-3");
        assert_eq!(format_return_code(i32::MIN), "-2147483648");
    }
}
