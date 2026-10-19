//! OVERLAY — overlay data from input streams.
//!
//! Author's Edition 1.1.11 p.447.
//!
//! Syntax (fixed railroad order):
//! `OVERlay [BLANK | xorc]`
//!
//! Verb `OVERlay` min 4. Optional pad character; default blank. xorc is the
//! shared helper (p.197): one character, two hex digits, `BLANK`/`SPACE`, or
//! `TABULATE`. Type: Gateway.

use pipe_core::{Record, StageCtx, StreamId};

use crate::xrange::parse_xorc;

pub fn overlay(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let pad = match parse_overlay(&args) {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(format!("OVERLAY: {e}"));
                return;
            }
        };

        // p.447 commit check: primary output is the only connected output.
        if ctx
            .connected_outputs()
            .iter()
            .any(|s| *s != StreamId::PRIMARY)
        {
            ctx.fail(
                "OVERLAY: the primary output stream is the only connected output stream allowed",
            );
            return;
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }

        ctx.select_output(StreamId::PRIMARY);
        run_overlay(ctx, pad);
    })
}

fn parse_overlay(args: &[String]) -> Result<u8, String> {
    match args {
        [] => Ok(b' '),
        [s] => parse_xorc(s),
        [_, rest, ..] => Err(format!("unexpected operand {rest}")),
    }
}

fn run_overlay(ctx: &mut StageCtx, pad: u8) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }

        // Live streams this cycle: connected and not at end-of-file.
        let mut remaining: Vec<StreamId> = ctx
            .connected_inputs()
            .into_iter()
            .filter(|&s| !ctx.input_at_eof(s))
            .collect();
        if remaining.is_empty() {
            return;
        }

        // Consume each stream's record as soon as it is loaded into the overlay
        // buffer (p.447 Record Delay). Do not hold input slots while writing.
        let mut recs: Vec<(StreamId, Vec<u8>)> = Vec::new();
        while !remaining.is_empty() {
            let mut still = Vec::new();
            for stream in remaining {
                if ctx.input_record_ready(stream) {
                    ctx.select_input(stream);
                    if let Some(rec) = ctx.peek() {
                        recs.push((stream, rec.as_bytes().to_vec()));
                        if ctx.read().is_err() {
                            short_inputs(ctx);
                            return;
                        }
                    }
                } else if !ctx.input_at_eof(stream) {
                    still.push(stream);
                }
            }
            remaining = still;
            if remaining.is_empty() {
                break;
            }
            if !ctx.output_connected(StreamId::PRIMARY) {
                short_inputs(ctx);
                return;
            }
            if ctx.wait_any_input(&remaining).is_err() {
                short_inputs(ctx);
                return;
            }
        }

        if recs.is_empty() {
            return;
        }

        ctx.select_output(StreamId::PRIMARY);
        if ctx
            .write(Record::from(overlay_records(&recs, pad)))
            .is_err()
        {
            short_inputs(ctx);
            return;
        }
    }
}

/// Overlay `recs` in stream-number order. Each position is the character from
/// the highest-numbered stream where that column is present and not the pad
/// (p.447). Missing columns do not contribute; the output length is the longest
/// contributing record, with pad filling columns that have no non-pad byte.
fn overlay_records(recs: &[(StreamId, Vec<u8>)], pad: u8) -> Vec<u8> {
    let mut ordered = recs.to_vec();
    ordered.sort_by_key(|(s, _)| s.0);
    let max = ordered.iter().map(|(_, r)| r.len()).max().unwrap_or(0);
    let mut out = vec![pad; max];
    for (_, rec) in ordered {
        for (i, &b) in rec.iter().enumerate() {
            if b != pad {
                out[i] = b;
            }
        }
    }
    out
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<u8, String> {
        parse_overlay(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn overlay_strs(pad: u8, recs: &[&str]) -> String {
        let pairs: Vec<(StreamId, Vec<u8>)> = recs
            .iter()
            .enumerate()
            .map(|(i, s)| (StreamId(i as u32), s.as_bytes().to_vec()))
            .collect();
        String::from_utf8(overlay_records(&pairs, pad)).unwrap()
    }

    #[test]
    fn default_pad_is_blank() {
        assert_eq!(parse(&[]).unwrap(), b' ');
        assert_eq!(parse(&["BLANK"]).unwrap(), b' ');
        assert_eq!(parse(&["blank"]).unwrap(), b' ');
        assert_eq!(parse(&["SPACE"]).unwrap(), b' ');
    }

    #[test]
    fn xorc_one_char_and_two_hex() {
        assert_eq!(parse(&["*"]).unwrap(), b'*');
        assert_eq!(parse(&["2a"]).unwrap(), b'*');
        // Two hex digits: ASCII X'40' is '@' (book X'40' is EBCDIC blank).
        assert_eq!(parse(&["40"]).unwrap(), b'@');
    }

    #[test]
    fn tabulate_xorc() {
        assert_eq!(parse(&["TAB"]).unwrap(), b'\t');
        assert_eq!(parse(&["TABULATE"]).unwrap(), b'\t');
    }

    #[test]
    fn extra_and_invented_operands_rejected() {
        let e = parse(&["BLANK", "*"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
        for w in ["PAD", "ANYCASE", "STOP", "FIRST", "COUNT", "XOR"] {
            let e = parse(&[w]).unwrap_err();
            assert!(
                e.contains("invalid xorc") || e.contains("unexpected"),
                "{w}: {e}"
            );
        }
        let e = parse(&["BLAN"]).unwrap_err();
        assert!(e.contains("invalid xorc"), "{e}");
    }

    #[test]
    fn higher_stream_non_pad_wins() {
        assert_eq!(overlay_strs(b' ', &["ABC", "  X"]), "ABX");
        assert_eq!(
            overlay_strs(b' ', &["the left", "   the right"]),
            "thethefright"
        );
    }

    #[test]
    fn pad_does_not_overlay_a_lower_stream() {
        assert_eq!(overlay_strs(b' ', &["XYZ", " Q"]), "XQZ");
    }

    #[test]
    fn shorter_record_does_not_contribute_missing_columns() {
        assert_eq!(overlay_strs(b' ', &["ABCD", "X"]), "XBCD");
        assert_eq!(overlay_strs(b' ', &["AB", "WXYZ"]), "WXYZ");
    }

    #[test]
    fn output_length_is_the_longest_record() {
        assert_eq!(overlay_strs(b' ', &["AB", "     "]), "AB   ");
        assert_eq!(overlay_strs(b' ', &["A  "]), "A  ");
    }

    #[test]
    fn custom_pad_star() {
        assert_eq!(overlay_strs(b'*', &["A*C", "*B*"]), "ABC");
    }

    #[test]
    fn null_records_contribute_nothing() {
        assert_eq!(overlay_strs(b' ', &["ABC", ""]), "ABC");
        assert_eq!(overlay_strs(b' ', &["", ""]), "");
    }

    #[test]
    fn three_streams_highest_non_pad_wins() {
        assert_eq!(overlay_strs(b' ', &["AAA", " B", "  C"]), "ABC");
    }

    #[test]
    fn arrival_order_does_not_change_who_wins() {
        // Higher stream number wins even if that record is listed first.
        let recs = vec![
            (StreamId::SECONDARY, b"  X".to_vec()),
            (StreamId::PRIMARY, b"ABC".to_vec()),
        ];
        assert_eq!(
            String::from_utf8(overlay_records(&recs, b' ')).unwrap(),
            "ABX"
        );
    }
}
