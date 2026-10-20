//! COLLATE — Author's Edition 1.1.11 pp.295–297.
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `COLLATE [STOP ANYEOF] [NOPAD | PAD xorc] [ANYcase]
//!          [inputRange [inputRange]] [MASTER DETAIL | MASTER | DETAIL [MASTER]]`
//!
//! Sorter. Masters on primary input, details on secondary. Both streams are
//! assumed sorted ascending. Matched records → primary; unmatched masters →
//! secondary; unmatched details → tertiary.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{compare_keys, is_anycase, wait_peek};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range_with, parse_xorc,
    InputRange, FIELD_SEP, WORD_SEP,
};

const TERTIARY: StreamId = StreamId(2);

pub fn collate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_collate(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("COLLATE: {e}"));
                return;
            }
        };
        run_collate(ctx, &spec);
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteMode {
    MasterDetail,
    DetailMaster,
    Master,
    Detail,
}

struct CollateSpec {
    stop_anyeof: bool,
    pad: Option<u8>,
    anycase: bool,
    master_range: Option<InputRange>,
    detail_range: Option<InputRange>,
    mode: WriteMode,
}

fn parse_collate(args: &[String]) -> Result<CollateSpec, String> {
    let mut i = 0;

    let mut stop_anyeof = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("STOP") {
        i += 1;
        if i >= args.len() || !args[i].eq_ignore_ascii_case("ANYEOF") {
            return Err("STOP must be followed by ANYEOF".into());
        }
        stop_anyeof = true;
        i += 1;
    }

    let mut pad = None;
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
            i += 1;
        }
    }

    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    let mut master_range = None;
    let mut detail_range = None;
    if i < args.len() && looks_like_input_range_start(&args[i]) && !is_write_keyword(&args[i]) {
        let (first, word_sep, field_sep) =
            parse_input_range_with(args, &mut i, WORD_SEP, FIELD_SEP)?;
        master_range = Some(first.clone());
        detail_range = Some(first);
        if i < args.len() && looks_like_input_range_start(&args[i]) && !is_write_keyword(&args[i]) {
            let (second, _, _) = parse_input_range_with(args, &mut i, word_sep, field_sep)?;
            detail_range = Some(second);
        }
    }

    let mut mode = WriteMode::MasterDetail;
    if i < args.len() {
        if !is_write_keyword(&args[i]) {
            return Err(unexpected_operand(&args[i]));
        }
        mode = parse_write_mode(args, &mut i)?;
    }
    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    Ok(CollateSpec {
        stop_anyeof,
        pad,
        anycase,
        master_range,
        detail_range,
        mode,
    })
}

fn is_write_keyword(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    u == "MASTER" || u == "DETAIL"
}

fn parse_write_mode(args: &[String], i: &mut usize) -> Result<WriteMode, String> {
    let upper = args[*i].to_ascii_uppercase();
    if upper == "MASTER" {
        *i += 1;
        if *i < args.len() && args[*i].eq_ignore_ascii_case("DETAIL") {
            *i += 1;
            return Ok(WriteMode::MasterDetail);
        }
        return Ok(WriteMode::Master);
    }
    if upper == "DETAIL" {
        *i += 1;
        if *i < args.len() && args[*i].eq_ignore_ascii_case("MASTER") {
            *i += 1;
            return Ok(WriteMode::DetailMaster);
        }
        return Ok(WriteMode::Detail);
    }
    Err(unexpected_operand(&args[*i]))
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if upper == "STOP"
        || upper == "ANYEOF"
        || upper == "PAD"
        || upper == "NOPAD"
        || is_anycase(arg)
        || is_write_keyword(arg)
    {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unrecognized operand {arg}")
    }
}

fn run_collate(ctx: &mut StageCtx, spec: &CollateSpec) {
    if ctx.input_connected(TERTIARY) {
        ctx.fail("COLLATE: the tertiary input stream must not be connected");
        return;
    }
    if ctx.connected_inputs().iter().any(|s| s.0 > 2)
        || ctx.connected_outputs().iter().any(|s| s.0 > 2)
    {
        ctx.fail("COLLATE: only the primary, secondary, and tertiary streams may be defined");
        return;
    }

    if !any_output(ctx) {
        short_inputs(ctx);
        return;
    }

    loop {
        if !any_output(ctx) {
            short_inputs(ctx);
            return;
        }

        let (master, detail) = match wait_pair(ctx, spec.stop_anyeof) {
            None => return,
            Some(pair) => pair,
        };

        match (master, detail) {
            (None, None) => return,
            (None, Some(_)) if spec.stop_anyeof => {
                short_inputs(ctx);
                return;
            }
            (Some(_), None) if spec.stop_anyeof => {
                short_inputs(ctx);
                return;
            }
            (None, Some(_)) => {
                // Primary EOF: remaining details are unmatched (SHORT splice
                // stand-in to tertiary, p.296).
                if !copy_unmatched(ctx, StreamId::SECONDARY, TERTIARY) {
                    return;
                }
            }
            (Some(_), None) => {
                if !copy_unmatched(ctx, StreamId::PRIMARY, StreamId::SECONDARY) {
                    return;
                }
            }
            (Some(m), Some(d)) => {
                if !step_pair(ctx, spec, m, d) {
                    return;
                }
            }
        }
    }
}

/// Wait until both sides can be decided. STOP ANYEOF returns as soon as either
/// stream is at end-of-file, leaving the other unconsumed (p.296).
fn wait_pair(ctx: &mut StageCtx, stop_anyeof: bool) -> Option<(Option<Record>, Option<Record>)> {
    loop {
        let p_ready = ctx.input_record_ready(StreamId::PRIMARY);
        let s_ready = ctx.input_record_ready(StreamId::SECONDARY);
        let p_eof = ctx.input_at_eof(StreamId::PRIMARY);
        let s_eof = ctx.input_at_eof(StreamId::SECONDARY);
        if stop_anyeof && (p_eof || s_eof) {
            return Some((
                if p_ready {
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.peek()
                } else {
                    None
                },
                if s_ready {
                    ctx.select_input(StreamId::SECONDARY);
                    ctx.peek()
                } else {
                    None
                },
            ));
        }
        if (p_ready || p_eof) && (s_ready || s_eof) {
            return Some((
                if p_ready {
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.peek()
                } else {
                    None
                },
                if s_ready {
                    ctx.select_input(StreamId::SECONDARY);
                    ctx.peek()
                } else {
                    None
                },
            ));
        }
        let mut wait = Vec::new();
        if !p_ready && !p_eof {
            wait.push(StreamId::PRIMARY);
        }
        if !s_ready && !s_eof {
            wait.push(StreamId::SECONDARY);
        }
        if wait.is_empty() {
            return Some((None, None));
        }
        if ctx.wait_any_input(&wait).is_err() {
            return None;
        }
    }
}

fn step_pair(ctx: &mut StageCtx, spec: &CollateSpec, master: Record, detail: Record) -> bool {
    let mk = key_of(master.as_bytes(), spec.master_range.as_ref());
    let dk = key_of(detail.as_bytes(), spec.detail_range.as_ref());
    match compare_keys(&mk, &dk, spec.pad, spec.anycase) {
        std::cmp::Ordering::Less => emit(ctx, StreamId::SECONDARY, master) && consume_primary(ctx),
        std::cmp::Ordering::Greater => emit(ctx, TERTIARY, detail) && consume_secondary(ctx),
        std::cmp::Ordering::Equal => write_match(ctx, spec, master, &mk),
    }
}

fn write_match(ctx: &mut StageCtx, spec: &CollateSpec, master: Record, master_key: &[u8]) -> bool {
    match spec.mode {
        WriteMode::MasterDetail => {
            if !emit(ctx, StreamId::PRIMARY, master) || !consume_primary(ctx) {
                return false;
            }
            drain_matching_details(ctx, spec, master_key, true)
        }
        WriteMode::DetailMaster => {
            if !drain_matching_details(ctx, spec, master_key, true) {
                return false;
            }
            emit(ctx, StreamId::PRIMARY, master) && consume_primary(ctx)
        }
        WriteMode::Master => {
            if !emit(ctx, StreamId::PRIMARY, master) || !consume_primary(ctx) {
                return false;
            }
            drain_matching_details(ctx, spec, master_key, false)
        }
        WriteMode::Detail => {
            if !drain_matching_details(ctx, spec, master_key, true) {
                return false;
            }
            consume_primary(ctx)
        }
    }
}

fn drain_matching_details(
    ctx: &mut StageCtx,
    spec: &CollateSpec,
    master_key: &[u8],
    write: bool,
) -> bool {
    loop {
        let Some(detail) = wait_peek(ctx, StreamId::SECONDARY) else {
            return true;
        };
        let dk = key_of(detail.as_bytes(), spec.detail_range.as_ref());
        if compare_keys(master_key, &dk, spec.pad, spec.anycase) != std::cmp::Ordering::Equal {
            return true;
        }
        if write && !emit(ctx, StreamId::PRIMARY, detail) {
            return false;
        }
        if !consume_secondary(ctx) {
            return false;
        }
    }
}

fn copy_unmatched(ctx: &mut StageCtx, inn: StreamId, out: StreamId) -> bool {
    loop {
        if !any_output(ctx) {
            short_inputs(ctx);
            return false;
        }
        let Some(rec) = wait_peek(ctx, inn) else {
            return true;
        };
        if !emit(ctx, out, rec) {
            return false;
        }
        ctx.select_input(inn);
        if ctx.read().is_err() {
            return false;
        }
    }
}

fn key_of(record: &[u8], range: Option<&InputRange>) -> Vec<u8> {
    match range {
        None => record.to_vec(),
        Some(r) => extract_input_range(record, r),
    }
}

fn emit(ctx: &mut StageCtx, stream: StreamId, rec: Record) -> bool {
    if !any_output(ctx) {
        short_inputs(ctx);
        return false;
    }
    if !ctx.output_connected(stream) {
        return true;
    }
    ctx.select_output(stream);
    if ctx.write(rec).is_err() {
        return any_output(ctx);
    }
    true
}

fn consume_primary(ctx: &mut StageCtx) -> bool {
    ctx.select_input(StreamId::PRIMARY);
    ctx.read().is_ok()
}

fn consume_secondary(ctx: &mut StageCtx) -> bool {
    ctx.select_input(StreamId::SECONDARY);
    ctx.read().is_ok()
}

fn any_output(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY)
        || ctx.output_connected(StreamId::SECONDARY)
        || ctx.output_connected(TERTIARY)
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in [StreamId::PRIMARY, StreamId::SECONDARY] {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> CollateSpec {
        parse_collate(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn defaults_are_nopad_master_detail() {
        let s = spec(&[]);
        assert!(!s.stop_anyeof);
        assert_eq!(s.pad, None);
        assert!(!s.anycase);
        assert!(s.master_range.is_none());
        assert_eq!(s.mode, WriteMode::MasterDetail);
    }

    #[test]
    fn stop_anyeof_pad_anycase_ranges_detail() {
        let s = spec(&[
            "STOP", "ANYEOF", "PAD", "40", "ANY", "1.10", "11.5", "DETAIL",
        ]);
        assert!(s.stop_anyeof);
        assert_eq!(s.pad, Some(b'@'));
        assert!(s.anycase);
        assert!(s.master_range.is_some());
        assert!(s.detail_range.is_some());
        assert_eq!(s.mode, WriteMode::Detail);
    }

    #[test]
    fn detail_master_order() {
        assert_eq!(spec(&["DETAIL", "MASTER"]).mode, WriteMode::DetailMaster);
        assert_eq!(spec(&["MASTER"]).mode, WriteMode::Master);
    }

    #[test]
    fn stop_requires_anyeof() {
        assert!(parse_collate(&["STOP".into()]).is_err());
        assert!(parse_collate(&["STOP".into(), "ALLEOF".into()]).is_err());
    }

    #[test]
    fn one_range_applies_to_both_streams() {
        let s = spec(&["1.3"]);
        assert_eq!(s.master_range, s.detail_range);
        assert!(s.master_range.is_some());
    }

    #[test]
    fn anycase_synonyms() {
        assert!(spec(&["CASEANY"]).anycase);
        assert!(spec(&["IGNORECASE"]).anycase);
        assert!(spec(&["CASEIGNORE"]).anycase);
        assert!(spec(&["ANY"]).anycase);
        assert!(parse_collate(&["AN".into()]).is_err());
    }
}
