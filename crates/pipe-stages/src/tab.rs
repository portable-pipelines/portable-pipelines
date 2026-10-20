//! UNTAB / RETAB — expand and compress tabulate characters.
//!
//! Author's Edition 1.1.11 pp.575 (UNTAB) and 473 (RETAB).
//!
//! Syntax (fixed railroad; diagrams all-caps):
//! `UNTAB [ -number | number … ]`
//! `RETAB [ -number | number … ]`
//!
//! Default is `-3`, a tab stop in column 1 and every 3 columns (`1 4 7 …`).
//! A single negative number is that interval. A list of positive numbers is
//! an explicit stop list (any order); the smallest is the left margin.
//! Portable ASCII: tabulate is HT X'09' (book cites EBCDIC X'05'); blank is
//! X'20' (book cites X'40').

use pipe_core::{Record, StageCtx, StreamId};

const TAB: u8 = b'\t';
const BLANK: u8 = b' ';

pub fn untab(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_filter(ctx, "UNTAB", &args, apply_untab))
}

pub fn retab(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_filter(ctx, "RETAB", &args, apply_retab))
}

fn run_filter(
    ctx: &mut StageCtx,
    verb: &str,
    args: &[String],
    apply: fn(&[u8], &TabStops) -> Vec<u8>,
) {
    let stops = match parse_tab_stops(args) {
        Ok(s) => s,
        Err(e) => {
            ctx.fail(format!("{verb}: {e}"));
            return;
        }
    };

    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::PRIMARY.0 {
            ctx.fail(format!(
                "{verb}: no input stream other than the primary may be connected"
            ));
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
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = apply(rec.as_bytes(), &stops);
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum TabStops {
    /// Tab stop in column 1 and every `step` columns (`-number`).
    Interval { step: usize },
    /// Explicit stop list; `left` is the smallest stop (left margin).
    List { stops: Vec<usize>, left: usize },
}

impl TabStops {
    fn left_margin(&self) -> usize {
        match self {
            TabStops::Interval { .. } => 1,
            TabStops::List { left, .. } => *left,
        }
    }

    /// Smallest tab stop strictly greater than `col`, if any.
    fn next_stop(&self, col: usize) -> Option<usize> {
        match self {
            TabStops::Interval { step } => {
                let col = col.max(1);
                let k = (col - 1) / *step;
                step.checked_mul(k + 1).and_then(|p| p.checked_add(1))
            }
            TabStops::List { stops, .. } => stops.iter().copied().find(|&s| s > col),
        }
    }
}

fn parse_tab_stops(args: &[String]) -> Result<TabStops, String> {
    if args.is_empty() {
        return Ok(TabStops::Interval { step: 3 });
    }

    if is_negative_number(&args[0]) {
        if args.len() > 1 {
            return Err(unexpected_operand(&args[1]));
        }
        let step = parse_negative_interval(&args[0])?;
        return Ok(TabStops::Interval { step });
    }

    let mut stops = Vec::with_capacity(args.len());
    for arg in args {
        if is_negative_number(arg) {
            return Err("a negative number cannot be mixed with a list of positive numbers".into());
        }
        if !is_number_token(arg) {
            return Err(unexpected_operand(arg));
        }
        stops.push(parse_positive_number(arg, "tab stop")?);
    }
    stops.sort_unstable();
    stops.dedup();
    let left = stops[0];
    Ok(TabStops::List { stops, left })
}

fn is_number_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_negative_number(s: &str) -> bool {
    s.starts_with('-') && s.len() > 1 && s.as_bytes()[1..].iter().all(|b| b.is_ascii_digit())
}

fn parse_positive_number(s: &str, what: &str) -> Result<usize, String> {
    let n = s
        .parse::<i64>()
        .map_err(|_| format!("{what} {s} is out of range"))?;
    if n <= 0 || n > i32::MAX as i64 {
        return Err(format!("{what} must be 1 through {}, got {s}", i32::MAX));
    }
    Ok(n as usize)
}

fn parse_negative_interval(s: &str) -> Result<usize, String> {
    // `-number`: tab stop in column 1 and every n columns (p.473 / p.575).
    parse_positive_number(&s[1..], "-number")
}

fn unexpected_operand(arg: &str) -> String {
    format!("unexpected operand {arg}")
}

fn apply_untab(record: &[u8], stops: &TabStops) -> Vec<u8> {
    let mut out = Vec::with_capacity(record.len());
    let mut col = 1usize;
    for &b in record {
        if b == TAB {
            if let Some(stop) = stops.next_stop(col) {
                while col < stop {
                    out.push(BLANK);
                    col += 1;
                }
            } else {
                // Past the last stop: one blank (finite list; interval never ends).
                out.push(BLANK);
                col += 1;
            }
        } else {
            out.push(b);
            col += 1;
        }
    }
    out
}

fn apply_retab(record: &[u8], stops: &TabStops) -> Vec<u8> {
    let left = stops.left_margin();
    // p.473: when the smallest stop is not 1, discard columns before the
    // left margin. Column numbers of the remainder stay 1-based from the
    // start of the original record.
    let (data, mut col) = if left > 1 {
        let skip = left - 1;
        if record.len() <= skip {
            return Vec::new();
        }
        (&record[skip..], left)
    } else {
        (record, 1usize)
    };

    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] != BLANK {
            out.push(data[i]);
            col += 1;
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < data.len() && data[i] == BLANK {
            i += 1;
        }
        let mut remaining = i - start;
        while remaining > 0 {
            match stops.next_stop(col) {
                Some(stop) if stop - col <= remaining => {
                    let need = stop - col;
                    out.push(TAB);
                    col = stop;
                    remaining -= need;
                }
                Some(_) => {
                    for _ in 0..remaining {
                        out.push(BLANK);
                        col += 1;
                    }
                    remaining = 0;
                }
                None => {
                    for _ in 0..remaining {
                        out.push(BLANK);
                        col += 1;
                    }
                    remaining = 0;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops(args: &[&str]) -> TabStops {
        parse_tab_stops(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn untab_bytes(record: &[u8], args: &[&str]) -> Vec<u8> {
        apply_untab(record, &stops(args))
    }

    fn retab_bytes(record: &[u8], args: &[&str]) -> Vec<u8> {
        apply_retab(record, &stops(args))
    }

    #[test]
    fn default_is_minus_three() {
        assert_eq!(
            parse_tab_stops(&[]).unwrap(),
            TabStops::Interval { step: 3 }
        );
        assert_eq!(stops(&["-3"]), TabStops::Interval { step: 3 });
        assert_eq!(untab_bytes(b"a\tb", &[]), b"a  b");
        assert_eq!(untab_bytes(b"a\tb", &["-3"]), b"a  b");
        assert_eq!(retab_bytes(b"a  b", &[]), b"a\tb");
    }

    #[test]
    fn minus_three_equals_infinite_one_four_seven() {
        // `-3` is 1 4 7 … (infinite). An explicit `1 4 7` is a finite list.
        // Tab at column 8: `-3` next stop is 10 (two blanks); finite 1 4 7
        // has no stop after 7 (one blank).
        let tab_at_8 = b"xxxxxxx\ty";
        assert_eq!(untab_bytes(tab_at_8, &["-3"]), b"xxxxxxx  y");
        assert_eq!(untab_bytes(tab_at_8, &["1", "4", "7"]), b"xxxxxxx y");
        let long = {
            let mut v = vec![b'x'; 10];
            v.push(TAB);
            v.push(b'y');
            v
        };
        // Tab at column 11; `-3` next stop is 13 → two blanks.
        assert_eq!(untab_bytes(&long, &["-3"]), b"xxxxxxxxxx  y");
        // Finite 1 4 7: tab at 11 is past the last stop → one blank.
        assert_eq!(untab_bytes(&long, &["1", "4", "7"]), b"xxxxxxxxxx y");
    }

    #[test]
    fn minus_eight_unix_style() {
        // p.473 example uses `-8`: stops 1, 9, 17, …
        assert_eq!(untab_bytes(b"a\tb", &["-8"]), b"a       b");
        assert_eq!(retab_bytes(b"a       b", &["-8"]), b"a\tb");
        // Two blanks do not reach column 9.
        assert_eq!(retab_bytes(b"a  b", &["-8"]), b"a  b");
    }

    #[test]
    fn minus_one_is_every_column() {
        assert_eq!(untab_bytes(b"a\tb", &["-1"]), b"a b");
        assert_eq!(retab_bytes(b"a b", &["-1"]), b"a\tb");
    }

    #[test]
    fn positive_list_any_order() {
        let a = stops(&["1", "4", "7"]);
        let b = stops(&["7", "1", "4"]);
        assert_eq!(a, b);
        assert_eq!(untab_bytes(b"a\tb", &["7", "4", "1"]), b"a  b");
        assert_eq!(retab_bytes(b"a  b", &["7", "1", "4"]), b"a\tb");
    }

    #[test]
    fn left_margin_discards_on_retab_only() {
        // Stops 5 10: left margin 5. RETAB drops columns 1–4.
        assert_eq!(retab_bytes(b"ABCD     X", &["5", "10"]), b"\tX");
        assert_eq!(retab_bytes(b"ABCD    X", &["5", "10"]), b"    X");
        // UNTAB does not discard; a leading tab expands to column 5.
        assert_eq!(untab_bytes(b"\tX", &["5", "10"]), b"    X");
        assert_eq!(untab_bytes(b"AB\tX", &["5", "10"]), b"AB  X");
    }

    #[test]
    fn tab_past_last_stop_is_one_blank() {
        assert_eq!(untab_bytes(b"ab\tc", &["1"]), b"ab c");
        assert_eq!(retab_bytes(b"ab c", &["1"]), b"ab c");
        assert_eq!(untab_bytes(b"\t", &["1"]), b" ");
    }

    #[test]
    fn empty_and_trailing_blanks() {
        assert_eq!(untab_bytes(b"", &[]), b"");
        assert_eq!(retab_bytes(b"", &[]), b"");
        assert_eq!(untab_bytes(b"a\t", &[]), b"a  ");
        assert_eq!(retab_bytes(b"a  ", &[]), b"a\t");
        assert_eq!(retab_bytes(b"   ", &[]), b"\t");
        assert_eq!(untab_bytes(b"\t", &[]), b"   ");
    }

    #[test]
    fn short_record_before_left_margin_is_null() {
        assert_eq!(retab_bytes(b"ab", &["5", "10"]), b"");
        assert_eq!(retab_bytes(b"ABCD", &["5"]), b"");
        assert_eq!(retab_bytes(b"ABCDE", &["5"]), b"E");
    }

    #[test]
    fn existing_tab_in_retab_is_one_column() {
        // RETAB replaces blanks only; an input tab stays a one-column byte.
        assert_eq!(retab_bytes(b"a\tb", &[]), b"a\tb");
        assert_eq!(retab_bytes(b"a \tb", &[]), b"a \tb");
    }

    #[test]
    fn converse_default_tabs() {
        let samples: [&[u8]; 6] = [b"", b"a", b"a\tb", b"\tX", b"a\tb\tc", b"plain"];
        for s in samples {
            let expanded = apply_untab(s, &stops(&[]));
            let round = apply_untab(&apply_retab(&expanded, &stops(&[])), &stops(&[]));
            assert_eq!(round, expanded);
        }
    }

    #[test]
    fn parse_rejects_invented_and_mixed() {
        let err = parse_tab_stops(&["PAD".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
        let err = parse_tab_stops(&["-3".into(), "1".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
        let err = parse_tab_stops(&["1".into(), "-3".into()]).unwrap_err();
        assert!(err.contains("negative"), "{err}");
        let err = parse_tab_stops(&["0".into()]).unwrap_err();
        assert!(err.contains("must be 1"), "{err}");
        let err = parse_tab_stops(&["-0".into()]).unwrap_err();
        assert!(err.contains("must be 1"), "{err}");
        let err = parse_tab_stops(&["*".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
        let err = parse_tab_stops(&["+3".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
        let err = parse_tab_stops(&["BLANK".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
        let err = parse_tab_stops(&["TABULATE".into()]).unwrap_err();
        assert!(err.contains("unexpected operand"), "{err}");
    }

    #[test]
    fn duplicate_stops_are_unique() {
        assert_eq!(stops(&["1", "4", "4", "7"]), stops(&["1", "4", "7"]));
    }
}
