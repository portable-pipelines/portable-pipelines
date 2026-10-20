//! XRANGE — write a range of characters.
//!
//! Author's Edition 1.1.11 p.609 (device driver, not the xorc helper).
//!
//! Syntax (diagram all-caps): `XRANGE [xrange | xorc xorc]`
//!
//! Writes one record containing the inclusive character range. No argument
//! writes the 256-byte identity table X'00'–X'FF' (xlate note 4 p.607:
//! “the neutral table (from xrange)”). Must be a first stage.

use pipe_core::{Record, StageCtx, StreamId};

use crate::xrange::{expand_from_to, expand_xrange, parse_xorc};

pub fn xrange(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let bytes = match parse_xrange_args(&args) {
            Ok(bytes) => bytes,
            Err(e) => {
                ctx.fail(format!("XRANGE: {e}"));
                return;
            }
        };

        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.fail("XRANGE must be a first stage");
            return;
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }

        ctx.select_output(StreamId::PRIMARY);
        let _ = ctx.write(Record::from(bytes));
    })
}

fn parse_xrange_args(args: &[String]) -> Result<Vec<u8>, String> {
    match args.len() {
        0 => Ok(expand_from_to(0, 255)),
        1 => expand_xrange(&args[0])?.ok_or_else(|| format!("unrecognized operand {}", args[0])),
        2 => {
            let from = parse_xorc(&args[0])?;
            let to = parse_xorc(&args[1])?;
            Ok(expand_from_to(from, to))
        }
        _ => Err(format!("unexpected operand {}", args[2])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> Vec<u8> {
        parse_xrange_args(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn book_letter_forms() {
        // p.610: a-i / a.9 / a i → abcdefghi
        assert_eq!(spec(&["a-i"]), b"abcdefghi");
        assert_eq!(spec(&["a.9"]), b"abcdefghi");
        assert_eq!(spec(&["a", "i"]), b"abcdefghi");
    }

    #[test]
    fn wrap_and_hex() {
        assert_eq!(spec(&["fe", "01"]), vec![0xfe, 0xff, 0x00, 0x01]);
        assert_eq!(spec(&["fe-01"]), vec![0xfe, 0xff, 0x00, 0x01]);
        // Portable ASCII: xorc f0/f9 are the bytes, not EBCDIC digits.
        assert_eq!(spec(&["f0", "f9"]), (0xf0u8..=0xf9).collect::<Vec<_>>());
    }

    #[test]
    fn default_is_all_256() {
        let all = spec(&[]);
        assert_eq!(all.len(), 256);
        assert_eq!(all[0], 0);
        assert_eq!(all[255], 255);
    }

    #[test]
    fn wrap_nine_to_zero_is_248() {
        // p.610: xrange 9 0 | count bytes → 248 (same count on ASCII and EBCDIC).
        assert_eq!(spec(&["9", "0"]).len(), 248);
    }

    #[test]
    fn xorc_keywords() {
        assert_eq!(spec(&["blank"]), vec![b' ']);
        assert_eq!(spec(&["space"]), vec![b' ']);
        assert_eq!(spec(&["tab"]), vec![b'\t']);
    }

    #[test]
    fn extra_word_rejected() {
        assert!(parse_xrange_args(&["a".into(), "i".into(), "z".into()])
            .unwrap_err()
            .contains("unexpected"));
    }
}
