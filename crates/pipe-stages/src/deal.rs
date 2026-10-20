//! DEAL — Author's Edition 1.1.11 pp.313–316.
//!
//! Syntax (diagram is all-caps except `STREAMid`; no verb prefix):
//! `DEAL [STOP [ALLEOF | ANYEOF | number] | SECONDARY [RELEASE]
//!       | KEY inputRange [STRIP] | STREAMid inputRange [STRIP]]`
//!
//! Gateway. Pass each primary input record to one output stream: round-robin
//! (default), a secondary-input stream identifier, a key run, or a STREAMID
//! range in the record.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{first_word, matches_abbrev, parse_numeric_stream_id, wait_peek};
use crate::input_range::{
    extract_input_range, locate_input_range, looks_like_input_range_start, parse_input_range,
    InputRange,
};

pub fn deal(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_deal(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("DEAL: {e}"));
                return;
            }
        };
        run_deal(ctx, &spec);
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DealMode {
    Stop(Stop),
    Secondary { release: bool },
    Key { range: InputRange, strip: bool },
    StreamId { range: InputRange, strip: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    AllEof,
    AnyEof,
    Count(usize),
}

fn parse_deal(args: &[String]) -> Result<DealMode, String> {
    if args.is_empty() {
        return Ok(DealMode::Stop(Stop::AllEof));
    }
    let mut i = 0;
    let upper = args[i].to_ascii_uppercase();
    if upper == "STOP" {
        i += 1;
        let stop = parse_stop(args, &mut i)?;
        if i < args.len() {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        return Ok(DealMode::Stop(stop));
    }
    if upper == "SECONDARY" {
        i += 1;
        let mut release = false;
        if i < args.len() && args[i].eq_ignore_ascii_case("RELEASE") {
            release = true;
            i += 1;
        }
        if i < args.len() {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        return Ok(DealMode::Secondary { release });
    }
    if upper == "KEY" {
        i += 1;
        if i >= args.len() || !looks_like_input_range_start(&args[i]) {
            return Err("KEY requires an inputRange".into());
        }
        let range = parse_input_range(args, &mut i)?;
        let strip = take_strip(args, &mut i)?;
        if i < args.len() {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        return Ok(DealMode::Key { range, strip });
    }
    if is_streamid(&args[i]) {
        i += 1;
        if i >= args.len() || !looks_like_input_range_start(&args[i]) {
            return Err("STREAMID requires an inputRange".into());
        }
        let range = parse_input_range(args, &mut i)?;
        let strip = take_strip(args, &mut i)?;
        if i < args.len() {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        return Ok(DealMode::StreamId { range, strip });
    }
    Err(format!("unrecognized operand {}", args[i]))
}

fn parse_stop(args: &[String], i: &mut usize) -> Result<Stop, String> {
    if *i >= args.len() {
        return Ok(Stop::AllEof);
    }
    let upper = args[*i].to_ascii_uppercase();
    if upper == "ALLEOF" {
        *i += 1;
        Ok(Stop::AllEof)
    } else if upper == "ANYEOF" {
        *i += 1;
        Ok(Stop::AnyEof)
    } else if let Ok(n) = args[*i].parse::<usize>() {
        if n == 0 {
            Err("STOP count must be at least 1".into())
        } else {
            *i += 1;
            Ok(Stop::Count(n))
        }
    } else {
        Ok(Stop::AllEof)
    }
}

fn take_strip(args: &[String], i: &mut usize) -> Result<bool, String> {
    if *i < args.len() && args[*i].eq_ignore_ascii_case("STRIP") {
        *i += 1;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn is_streamid(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "STREAMID", 7)
}

fn run_deal(ctx: &mut StageCtx, spec: &DealMode) {
    let secondary_ok = matches!(spec, DealMode::Secondary { .. });
    if ctx.connected_inputs().iter().any(|s| s.0 > 1) {
        ctx.fail("DEAL: only the primary and secondary input streams may be connected");
        return;
    }
    if secondary_ok {
        if !ctx.input_connected(StreamId::SECONDARY) {
            ctx.fail("DEAL: SECONDARY requires the secondary input stream to be connected");
            return;
        }
    } else if ctx.input_connected(StreamId::SECONDARY) {
        ctx.fail("DEAL: no input stream other than the primary may be connected");
        return;
    }

    match spec {
        DealMode::Stop(stop) => run_round_robin(ctx, *stop),
        DealMode::Secondary { release } => run_secondary(ctx, *release),
        DealMode::Key { range, strip } => run_key(ctx, range, *strip),
        DealMode::StreamId { range, strip } => run_streamid(ctx, range, *strip),
    }
}

fn run_round_robin(ctx: &mut StageCtx, stop: Stop) {
    let cycle = output_cycle(ctx);
    if cycle.is_empty() {
        short_primary(ctx);
        return;
    }
    let mut idx = 0usize;
    let mut dead_flag = vec![false; cycle.len()];
    let mut dead = 0usize;
    let total = cycle.len();

    loop {
        if halt_stop(stop, dead, total, ctx) {
            short_primary(ctx);
            return;
        }
        let Some(rec) = wait_peek(ctx, StreamId::PRIMARY) else {
            return;
        };
        let mut wrote = false;
        let start = idx;
        loop {
            let dest = cycle[idx];
            if ctx.output_connected(dest) {
                ctx.select_output(dest);
                if ctx.write(rec.clone()).is_ok() {
                    wrote = true;
                    idx = (idx + 1) % cycle.len();
                    break;
                }
                if !dead_flag[idx] {
                    dead_flag[idx] = true;
                    dead += 1;
                }
            } else if !dead_flag[idx] {
                // Unconnected stream in the cycle counts as EOF (p.315).
                dead_flag[idx] = true;
                dead += 1;
            }
            idx = (idx + 1) % cycle.len();
            if idx == start {
                break;
            }
            if halt_stop(stop, dead, total, ctx) {
                short_primary(ctx);
                return;
            }
        }
        if !wrote {
            short_primary(ctx);
            return;
        }
        ctx.select_input(StreamId::PRIMARY);
        if ctx.read().is_err() {
            return;
        }
    }
}

fn halt_stop(stop: Stop, dead: usize, total: usize, ctx: &StageCtx) -> bool {
    match stop {
        Stop::AllEof => ctx.connected_outputs().is_empty() || dead >= total,
        Stop::AnyEof => dead >= 1 || ctx.connected_outputs().is_empty(),
        Stop::Count(n) => dead >= n || ctx.connected_outputs().is_empty(),
    }
}

fn run_secondary(ctx: &mut StageCtx, release: bool) {
    if !any_output(ctx) {
        short_inputs(ctx);
        return;
    }
    loop {
        if !any_output(ctx) {
            short_inputs(ctx);
            return;
        }
        let Some(sid_rec) = wait_peek(ctx, StreamId::SECONDARY) else {
            return;
        };
        let word = first_word(sid_rec.as_bytes());
        let dest = match parse_sid_word(word) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("DEAL: {e}"));
                return;
            }
        };
        if release {
            ctx.select_input(StreamId::SECONDARY);
            if ctx.read().is_err() {
                return;
            }
        }
        let Some(rec) = wait_peek(ctx, StreamId::PRIMARY) else {
            return;
        };
        if !write_strict(ctx, dest, rec) {
            return;
        }
        ctx.select_input(StreamId::PRIMARY);
        if ctx.read().is_err() {
            return;
        }
        if !release {
            ctx.select_input(StreamId::SECONDARY);
            if ctx.read().is_err() {
                return;
            }
        }
    }
}

fn run_key(ctx: &mut StageCtx, range: &InputRange, strip: bool) {
    if !any_output(ctx) {
        short_primary(ctx);
        return;
    }
    let cycle = output_cycle(ctx);
    if cycle.is_empty() {
        short_primary(ctx);
        return;
    }
    let mut idx = 0usize;
    let mut stored: Option<Vec<u8>> = None;

    loop {
        if !any_output(ctx) {
            short_primary(ctx);
            return;
        }
        let Some(rec) = wait_peek(ctx, StreamId::PRIMARY) else {
            return;
        };
        let key = extract_input_range(rec.as_bytes(), range);
        if stored.as_ref() != Some(&key) {
            if stored.is_some() {
                idx = (idx + 1) % cycle.len();
            }
            stored = Some(key);
        }
        let out = match maybe_strip(rec.as_bytes(), range, strip) {
            Ok(b) => Record::from(b),
            Err(e) => {
                ctx.fail(format!("DEAL: {e}"));
                return;
            }
        };
        if !write_strict(ctx, cycle[idx], out) {
            return;
        }
        ctx.select_input(StreamId::PRIMARY);
        if ctx.read().is_err() {
            return;
        }
    }
}

fn run_streamid(ctx: &mut StageCtx, range: &InputRange, strip: bool) {
    if !any_output(ctx) {
        short_primary(ctx);
        return;
    }
    loop {
        if !any_output(ctx) {
            short_primary(ctx);
            return;
        }
        let Some(rec) = wait_peek(ctx, StreamId::PRIMARY) else {
            return;
        };
        let sid = extract_input_range(rec.as_bytes(), range);
        let dest = match parse_sid_word(&sid) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("DEAL: {e}"));
                return;
            }
        };
        let out = match maybe_strip(rec.as_bytes(), range, strip) {
            Ok(b) => Record::from(b),
            Err(e) => {
                ctx.fail(format!("DEAL: {e}"));
                return;
            }
        };
        if !write_strict(ctx, dest, out) {
            return;
        }
        ctx.select_input(StreamId::PRIMARY);
        if ctx.read().is_err() {
            return;
        }
    }
}

fn parse_sid_word(word: &[u8]) -> Result<StreamId, String> {
    let s = String::from_utf8_lossy(word);
    if s.trim().is_empty() {
        return Err("stream identifier is missing".into());
    }
    parse_numeric_stream_id(&s)
}

fn maybe_strip(record: &[u8], range: &InputRange, strip: bool) -> Result<Vec<u8>, String> {
    if !strip {
        return Ok(record.to_vec());
    }
    let loc = locate_input_range(record, range);
    if !loc.present {
        return Ok(record.to_vec());
    }
    if loc.start != 0 && loc.end != record.len() {
        return Err("STRIP inputRange must be at the beginning or the end of the record".into());
    }
    let mut out = Vec::with_capacity(record.len() - (loc.end - loc.start));
    out.extend_from_slice(&record[..loc.start]);
    out.extend_from_slice(&record[loc.end..]);
    Ok(out)
}

/// Write to `dest`. KEY / SECONDARY / STREAMID terminate on output EOF
/// without consuming the input (p.314).
fn write_strict(ctx: &mut StageCtx, dest: StreamId, rec: Record) -> bool {
    if !ctx.output_connected(dest) {
        return false;
    }
    ctx.select_output(dest);
    ctx.write(rec).is_ok()
}

fn output_cycle(ctx: &StageCtx) -> Vec<StreamId> {
    ctx.connected_outputs()
}

fn any_output(ctx: &StageCtx) -> bool {
    !ctx.connected_outputs().is_empty()
}

fn short_primary(ctx: &mut StageCtx) {
    if ctx.input_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
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

    fn mode(args: &[&str]) -> DealMode {
        parse_deal(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn default_is_stop_alleof() {
        assert_eq!(mode(&[]), DealMode::Stop(Stop::AllEof));
        assert_eq!(mode(&["STOP"]), DealMode::Stop(Stop::AllEof));
        assert_eq!(mode(&["STOP", "ANYEOF"]), DealMode::Stop(Stop::AnyEof));
        assert_eq!(mode(&["STOP", "2"]), DealMode::Stop(Stop::Count(2)));
    }

    #[test]
    fn exclusive_options() {
        assert_eq!(mode(&["SECONDARY"]), DealMode::Secondary { release: false });
        assert_eq!(
            mode(&["SECONDARY", "RELEASE"]),
            DealMode::Secondary { release: true }
        );
        assert!(matches!(
            mode(&["KEY", "1.2"]),
            DealMode::Key { strip: false, .. }
        ));
        assert!(matches!(
            mode(&["STREAMID", "1", "STRIP"]),
            DealMode::StreamId { strip: true, .. }
        ));
        assert!(matches!(
            mode(&["STREAMI", "1"]),
            DealMode::StreamId { strip: false, .. }
        ));
    }

    #[test]
    fn streamid_min_7() {
        assert!(parse_deal(&["STREAM".into(), "1".into()]).is_err());
        assert!(parse_deal(&["STREAMI".into(), "1".into()]).is_ok());
    }

    #[test]
    fn stop_zero_rejected() {
        assert!(parse_deal(&["STOP".into(), "0".into()]).is_err());
    }

    #[test]
    fn options_are_exclusive() {
        assert!(parse_deal(&["STOP".into(), "KEY".into()]).is_err());
        assert!(parse_deal(&["KEY".into(), "1".into(), "SECONDARY".into()]).is_err());
    }
}
