//! MERGE — Author's Edition 1.1.11 pp.436–437.
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `MERGE [NOPAD | PAD xorc] [ANYcase]
//!        [Ascending | Descending |
//!         inputRange [Ascending | Descending] [NOPAD | PAD xorc] …]`
//!
//! Sorter. Merge up to 10 already-ordered input streams onto the primary
//! output. Equal keys keep the lowest-numbered stream first (stable).

use std::cmp::Ordering;

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{compare_keys, is_anycase, matches_abbrev, wait_peek};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, parse_xorc, InputRange,
};

const MAX_STREAMS: u32 = 9;
const MAX_FIELDS: usize = 10;

pub fn merge(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_merge(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("MERGE: {e}"));
                return;
            }
        };
        run_merge(ctx, &spec);
    })
}

#[derive(Clone, Debug)]
struct KeyField {
    range: Option<InputRange>,
    pad: Option<u8>,
    descending: bool,
}

#[derive(Debug)]
struct MergeSpec {
    anycase: bool,
    fields: Vec<KeyField>,
}

fn parse_merge(args: &[String]) -> Result<MergeSpec, String> {
    let mut i = 0;

    let mut default_pad = None;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "PAD" {
            i += 1;
            if i >= args.len() {
                return Err("PAD requires an xorc".into());
            }
            default_pad = Some(parse_xorc(&args[i])?);
            i += 1;
        } else if upper == "NOPAD" {
            i += 1;
        }
    }

    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    let mut descending = false;
    let mut fields = Vec::new();
    if i < args.len() {
        if let Some(desc) = parse_direction(&args[i]) {
            descending = desc;
            i += 1;
            if i < args.len() {
                return Err(unexpected_operand(&args[i]));
            }
        } else {
            while i < args.len() {
                if fields.len() >= MAX_FIELDS {
                    return Err("at most 10 sort ranges can be specified".into());
                }
                if !looks_like_input_range_start(&args[i]) {
                    return Err(unexpected_operand(&args[i]));
                }
                let range = parse_input_range(args, &mut i)?;
                let mut field_desc = false;
                if i < args.len() {
                    if let Some(desc) = parse_direction(&args[i]) {
                        field_desc = desc;
                        i += 1;
                    }
                }
                let mut pad = default_pad;
                if i < args.len() {
                    let upper = args[i].to_ascii_uppercase();
                    if upper == "PAD" {
                        i += 1;
                        if i >= args.len() {
                            return Err("PAD requires an xorc".into());
                        }
                        pad = Some(parse_xorc(&args[i])?);
                        i += 1;
                    } else if upper == "NOPAD" {
                        pad = None;
                        i += 1;
                    }
                }
                fields.push(KeyField {
                    range: Some(range),
                    pad,
                    descending: field_desc,
                });
            }
        }
    }

    if fields.is_empty() {
        fields.push(KeyField {
            range: None,
            pad: default_pad,
            descending,
        });
    }

    Ok(MergeSpec { anycase, fields })
}

fn parse_direction(s: &str) -> Option<bool> {
    let u = s.to_ascii_uppercase();
    if matches_abbrev(&u, "DESCENDING", 1) {
        Some(true)
    } else if matches_abbrev(&u, "ASCENDING", 1) {
        Some(false)
    } else {
        None
    }
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if upper == "PAD" || upper == "NOPAD" || is_anycase(arg) || parse_direction(arg).is_some() {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unrecognized operand {arg}")
    }
}

fn run_merge(ctx: &mut StageCtx, spec: &MergeSpec) {
    if ctx
        .connected_outputs()
        .iter()
        .any(|s| *s != StreamId::PRIMARY)
    {
        ctx.fail("MERGE: the primary output stream is the only connected output stream allowed");
        return;
    }
    if ctx.connected_inputs().iter().any(|s| s.0 > MAX_STREAMS) {
        ctx.fail("MERGE: at most 10 input streams (0 through 9) are supported");
        return;
    }

    if !ctx.output_connected(StreamId::PRIMARY) {
        short_inputs(ctx);
        return;
    }

    ctx.select_output(StreamId::PRIMARY);

    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }

        let inputs = ctx.connected_inputs();
        if inputs.is_empty() || inputs.iter().all(|&s| ctx.input_at_eof(s)) {
            return;
        }

        let mut recs: Vec<(StreamId, Record)> = Vec::new();
        for &stream in &inputs {
            if ctx.input_at_eof(stream) {
                continue;
            }
            match wait_peek(ctx, stream) {
                Some(r) => recs.push((stream, r)),
                None => continue,
            }
        }
        if recs.is_empty() {
            return;
        }

        let (winner, rec) = pick_winner(&recs, spec);
        ctx.select_output(StreamId::PRIMARY);
        if ctx.write(rec).is_err() {
            short_inputs(ctx);
            return;
        }
        ctx.select_input(winner);
        if ctx.read().is_err() {
            return;
        }
    }
}

fn pick_winner(recs: &[(StreamId, Record)], spec: &MergeSpec) -> (StreamId, Record) {
    let mut best = 0usize;
    for i in 1..recs.len() {
        let ord = compare_records(recs[i].1.as_bytes(), recs[best].1.as_bytes(), spec);
        if ord == Ordering::Less || (ord == Ordering::Equal && recs[i].0 .0 < recs[best].0 .0) {
            best = i;
        }
    }
    recs[best].clone()
}

fn compare_records(a: &[u8], b: &[u8], spec: &MergeSpec) -> Ordering {
    for field in &spec.fields {
        let ka = extract_field(a, field);
        let kb = extract_field(b, field);
        let mut ord = compare_keys(&ka, &kb, field.pad, spec.anycase);
        if field.descending {
            ord = ord.reverse();
        }
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn extract_field(record: &[u8], field: &KeyField) -> Vec<u8> {
    match &field.range {
        None => record.to_vec(),
        Some(range) => extract_input_range(record, range),
    }
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

    fn spec(args: &[&str]) -> MergeSpec {
        parse_merge(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn default_is_ascending_whole_record() {
        let s = spec(&[]);
        assert_eq!(s.fields.len(), 1);
        assert!(!s.fields[0].descending);
        assert!(s.fields[0].range.is_none());
        assert!(!s.anycase);
    }

    #[test]
    fn descending_abbrev_is_min_one() {
        assert!(spec(&["d"]).fields[0].descending);
        assert!(spec(&["DESC"]).fields[0].descending);
        assert!(!spec(&["a"]).fields[0].descending);
    }

    #[test]
    fn field_list_with_per_field_order_and_pad() {
        let s = spec(&[
            "1.3",
            "DESCENDING",
            "PAD",
            "40",
            "5.2",
            "ASCENDING",
            "NOPAD",
        ]);
        assert_eq!(s.fields.len(), 2);
        assert!(s.fields[0].descending);
        assert_eq!(s.fields[0].pad, Some(b'@'));
        assert!(!s.fields[1].descending);
        assert_eq!(s.fields[1].pad, None);
    }

    #[test]
    fn descending_then_range_is_rejected() {
        assert!(parse_merge(&["DESCENDING".into(), "1.3".into()]).is_err());
    }

    #[test]
    fn more_than_ten_ranges_rejected() {
        let args: Vec<String> = (1..=11).map(|n| n.to_string()).collect();
        assert!(parse_merge(&args).unwrap_err().contains("10"));
    }

    #[test]
    fn compare_equal_keys() {
        let s = spec(&[]);
        assert_eq!(compare_records(b"aa", b"aa", &s), Ordering::Equal);
        assert_eq!(compare_records(b"aa", b"ab", &s), Ordering::Less);
        let d = spec(&["D"]);
        assert_eq!(compare_records(b"aa", b"ab", &d), Ordering::Greater);
    }
}
