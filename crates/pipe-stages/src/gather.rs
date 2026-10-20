//! GATHER — Author's Edition 1.1.11 pp.366–367.
//!
//! Syntax (diagram is all-caps except `STREAMid`; no verb prefix):
//! `GATHER [STOP [ALLEOF | ANYEOF | number] | STREAMid inputRange]`
//!
//! Gateway. Round-robin copy from every input onto the primary output, or
//! route from the stream named in a STREAMID range of the primary record.

use pipe_core::{StageCtx, StreamId};

use crate::common::{matches_abbrev, parse_numeric_stream_id, wait_peek};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, InputRange,
};

pub fn gather(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_gather(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("GATHER: {e}"));
                return;
            }
        };
        run_gather(ctx, &spec);
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GatherMode {
    Stop(Stop),
    StreamId(InputRange),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    AllEof,
    AnyEof,
    Count(usize),
}

fn parse_gather(args: &[String]) -> Result<GatherMode, String> {
    if args.is_empty() {
        return Ok(GatherMode::Stop(Stop::AllEof));
    }
    let mut i = 0;
    let upper = args[i].to_ascii_uppercase();
    if upper == "STOP" {
        i += 1;
        let stop = parse_stop(args, &mut i)?;
        if i < args.len() {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        return Ok(GatherMode::Stop(stop));
    }
    if is_streamid(&args[i]) {
        i += 1;
        if i >= args.len() || !looks_like_input_range_start(&args[i]) {
            return Err("STREAMID requires an inputRange".into());
        }
        let range = parse_input_range(args, &mut i)?;
        if i < args.len() {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        return Ok(GatherMode::StreamId(range));
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

fn is_streamid(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "STREAMID", 7)
}

fn run_gather(ctx: &mut StageCtx, spec: &GatherMode) {
    if ctx
        .connected_outputs()
        .iter()
        .any(|s| *s != StreamId::PRIMARY)
    {
        ctx.fail("GATHER: the primary output stream is the only connected output stream allowed");
        return;
    }
    if !ctx.output_connected(StreamId::PRIMARY) {
        short_inputs(ctx);
        return;
    }

    match spec {
        GatherMode::Stop(stop) => run_round_robin(ctx, *stop),
        GatherMode::StreamId(range) => run_streamid(ctx, range),
    }
}

fn run_round_robin(ctx: &mut StageCtx, stop: Stop) {
    let cycle = ctx.connected_inputs();
    if cycle.is_empty() {
        return;
    }
    let total = cycle.len();
    let mut idx = 0usize;
    let mut dead_flag = vec![false; cycle.len()];
    let mut dead = 0usize;

    ctx.select_output(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }
        if halt_stop(stop, dead, total) {
            short_inputs(ctx);
            return;
        }

        let dest = cycle[idx];
        if ctx.input_at_eof(dest) && !ctx.input_record_ready(dest) {
            if !dead_flag[idx] {
                dead_flag[idx] = true;
                dead += 1;
            }
            if halt_stop(stop, dead, total) {
                short_inputs(ctx);
                return;
            }
            idx = (idx + 1) % cycle.len();
            continue;
        }

        let Some(rec) = wait_peek(ctx, dest) else {
            if !dead_flag[idx] {
                dead_flag[idx] = true;
                dead += 1;
            }
            if halt_stop(stop, dead, total) {
                short_inputs(ctx);
                return;
            }
            idx = (idx + 1) % cycle.len();
            continue;
        };
        ctx.select_output(StreamId::PRIMARY);
        if ctx.write(rec).is_err() {
            short_inputs(ctx);
            return;
        }
        ctx.select_input(dest);
        if ctx.read().is_err() {
            return;
        }
        idx = (idx + 1) % cycle.len();
    }
}

fn halt_stop(stop: Stop, dead: usize, total: usize) -> bool {
    match stop {
        Stop::AllEof => dead >= total,
        Stop::AnyEof => dead >= 1,
        Stop::Count(n) => dead >= n,
    }
}

fn run_streamid(ctx: &mut StageCtx, range: &InputRange) {
    ctx.select_output(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }

        let Some(primary) = wait_peek(ctx, StreamId::PRIMARY) else {
            return;
        };
        let sid = extract_input_range(primary.as_bytes(), range);
        let dest = match resolve_gather_sid(&sid) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("GATHER: {e}"));
                return;
            }
        };

        if dest == StreamId::PRIMARY {
            ctx.select_output(StreamId::PRIMARY);
            if ctx.write(primary).is_err() {
                return;
            }
            ctx.select_input(StreamId::PRIMARY);
            if ctx.read().is_err() {
                return;
            }
            continue;
        }

        // p.367: STREAMID terminates at EOF on the stream it reads.
        let Some(rec) = wait_peek(ctx, dest) else {
            short_inputs(ctx);
            return;
        };
        ctx.select_output(StreamId::PRIMARY);
        if ctx.write(rec).is_err() {
            return;
        }
        ctx.select_input(dest);
        if ctx.read().is_err() {
            return;
        }
        ctx.select_input(StreamId::PRIMARY);
        if ctx.read().is_err() {
            return;
        }
    }
}

/// Null, blank, or decimal zero → stream 0 (pass the primary record, p.366).
fn resolve_gather_sid(bytes: &[u8]) -> Result<StreamId, String> {
    let s = String::from_utf8_lossy(bytes);
    let t = s.trim();
    if t.is_empty() {
        return Ok(StreamId::PRIMARY);
    }
    parse_numeric_stream_id(t)
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

    fn mode(args: &[&str]) -> GatherMode {
        parse_gather(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn default_is_stop_alleof() {
        assert_eq!(mode(&[]), GatherMode::Stop(Stop::AllEof));
        assert_eq!(mode(&["STOP"]), GatherMode::Stop(Stop::AllEof));
        assert_eq!(mode(&["STOP", "ANYEOF"]), GatherMode::Stop(Stop::AnyEof));
        assert_eq!(mode(&["STOP", "2"]), GatherMode::Stop(Stop::Count(2)));
    }

    #[test]
    fn streamid_min_7() {
        assert!(parse_gather(&["STREAM".into(), "1".into()]).is_err());
        assert!(matches!(mode(&["STREAMI", "1"]), GatherMode::StreamId(_)));
        assert!(matches!(
            mode(&["STREAMID", "1-2"]),
            GatherMode::StreamId(_)
        ));
    }

    #[test]
    fn exclusive_and_required_range() {
        assert!(parse_gather(&["STREAMID".into()]).is_err());
        assert!(parse_gather(&["STOP".into(), "STREAMID".into()]).is_err());
    }

    #[test]
    fn sid_null_blank_zero() {
        assert_eq!(resolve_gather_sid(b"").unwrap(), StreamId::PRIMARY);
        assert_eq!(resolve_gather_sid(b"  ").unwrap(), StreamId::PRIMARY);
        assert_eq!(resolve_gather_sid(b"0").unwrap(), StreamId::PRIMARY);
        assert_eq!(resolve_gather_sid(b"00").unwrap(), StreamId::PRIMARY);
        assert_eq!(resolve_gather_sid(b"2").unwrap(), StreamId(2));
        assert!(resolve_gather_sid(b"alpha").is_err());
    }
}
