//! PAD — expand short records.
//!
//! Author's Edition 1.1.11 p.451.
//!
//! Syntax (fixed order):
//! `PAD [Right | Left] [[number] MODULO] number [BLANK | xorc]`
//!
//! Default is right-pad with blank. `Right` / `Left` are mixed-case (min 1).
//! `PAD`, `MODULO`, and `BLANK` are all-caps (exact). At least one number is
//! required. The first number, when present, is the MODULO offset and must be
//! smaller than the second.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::matches_abbrev;
use crate::xrange::parse_xorc;

pub fn pad(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_pad(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("PAD: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("PAD: no input stream other than the primary may be connected");
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
        run_pad(ctx, &spec);
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Length {
    /// Minimum output length when MODULO is omitted.
    Absolute(usize),
    /// Smallest `k * modulo + offset` that is ≥ the input length.
    Modulo { offset: usize, modulo: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PadSpec {
    left: bool,
    length: Length,
    pad: u8,
}

fn parse_pad(args: &[String]) -> Result<PadSpec, String> {
    let mut i = 0;

    // [Right | Left] — mixed-case diagram prefixes, min 1 (p.451). Default Right.
    let mut left = false;
    if i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if matches_abbrev(&u, "LEFT", 1) {
            left = true;
            i += 1;
        } else if matches_abbrev(&u, "RIGHT", 1) {
            left = false;
            i += 1;
        }
    }

    if i >= args.len() {
        return Err("number is required".into());
    }

    let length = if is_modulo(&args[i]) {
        // MODULO number — default offset 0.
        i += 1;
        let modulo = take_modulo_number(args, &mut i)?;
        Length::Modulo { offset: 0, modulo }
    } else if is_number_token(&args[i]) {
        let first = parse_nonneg_number(&args[i], "number")?;
        i += 1;
        if i < args.len() && is_modulo(&args[i]) {
            // number MODULO number — first is the offset.
            i += 1;
            let modulo = take_modulo_number(args, &mut i)?;
            if first >= modulo {
                return Err("first number must be smaller than the second".into());
            }
            Length::Modulo {
                offset: first,
                modulo,
            }
        } else {
            Length::Absolute(first)
        }
    } else {
        return Err(unexpected_before_number(&args[i]));
    };

    // [BLANK | xorc] — default blank (portable ASCII X'20'; book cites X'40').
    // After the number, a one-character word is xorc (so `R` is X'52'), not Right.
    let mut pad = b' ';
    if i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u == "LEFT" || u == "RIGHT" {
            return Err(format!("{u} must appear before the number"));
        }
        if is_modulo(&args[i]) {
            return Err("MODULO must appear before the length number".into());
        }
        if u == "BLANK" {
            pad = b' ';
        } else {
            pad = parse_xorc(&args[i])?;
        }
        i += 1;
    }

    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(PadSpec { left, length, pad })
}

fn is_modulo(s: &str) -> bool {
    s.eq_ignore_ascii_case("MODULO")
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

fn take_modulo_number(args: &[String], i: &mut usize) -> Result<usize, String> {
    if *i >= args.len() {
        return Err("MODULO requires a number".into());
    }
    let n = parse_nonneg_number(&args[*i], "number")?;
    if n == 0 {
        return Err("number must be positive when MODULO is specified".into());
    }
    *i += 1;
    Ok(n)
}

fn unexpected_before_number(arg: &str) -> String {
    let u = arg.to_ascii_uppercase();
    if matches_abbrev(&u, "LEFT", 1) || matches_abbrev(&u, "RIGHT", 1) {
        format!("{u} must appear before the number")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn run_pad(ctx: &mut StageCtx, spec: &PadSpec) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = apply_pad(rec.as_bytes(), spec);
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

fn target_len(input_len: usize, length: Length) -> usize {
    match length {
        Length::Absolute(n) => n,
        Length::Modulo { offset, modulo } => {
            if input_len <= offset {
                return offset;
            }
            let rem = (input_len - offset) % modulo;
            if rem == 0 {
                input_len
            } else {
                input_len + (modulo - rem)
            }
        }
    }
}

fn apply_pad(record: &[u8], spec: &PadSpec) -> Vec<u8> {
    let target = target_len(record.len(), spec.length);
    if record.len() >= target {
        return record.to_vec();
    }
    let n = target - record.len();
    let mut out = Vec::with_capacity(target);
    if spec.left {
        out.resize(n, spec.pad);
        out.extend_from_slice(record);
    } else {
        out.extend_from_slice(record);
        out.resize(target, spec.pad);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> PadSpec {
        parse_pad(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn default_right_blank() {
        assert_eq!(apply_pad(b"hi", &spec(&["5"])), b"hi   ");
        assert_eq!(
            spec(&["5"]),
            PadSpec {
                left: false,
                length: Length::Absolute(5),
                pad: b' ',
            }
        );
    }

    #[test]
    fn left_aligns_text_on_the_right() {
        assert_eq!(apply_pad(b"hi", &spec(&["left", "5"])), b"   hi");
    }

    #[test]
    fn longer_record_is_unmodified() {
        assert_eq!(apply_pad(b"hello", &spec(&["3"])), b"hello");
    }

    #[test]
    fn modulo_smallest_multiple_plus_offset() {
        assert_eq!(apply_pad(b"abcde", &spec(&["modulo", "4"])), b"abcde   ");
        assert_eq!(apply_pad(b"ab", &spec(&["1", "modulo", "4"])), b"ab   ");
        assert_eq!(apply_pad(b"abcd", &spec(&["modulo", "4"])), b"abcd");
    }

    #[test]
    fn xorc_star() {
        assert_eq!(apply_pad(b"hi", &spec(&["5", "*"])), b"hi***");
    }

    #[test]
    fn missing_number_is_an_error() {
        assert!(parse_pad(&[]).unwrap_err().contains("number is required"));
        assert!(parse_pad(&["left".into()])
            .unwrap_err()
            .contains("number is required"));
    }

    #[test]
    fn offset_not_smaller_than_modulo_is_an_error() {
        let err = parse_pad(&["4".into(), "modulo".into(), "4".into()]).unwrap_err();
        assert!(err.contains("smaller"), "{err}");
    }
}
