//! FANINTWO — Author's Edition 1.1.11 pp.340–341.
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `FANINTWO`
//!
//! Arcane gateway. Passes records from whichever of two input streams has one
//! available to the primary output, giving priority to the secondary input.
//! When a primary record is waiting, FANINTWO yields so a secondary producer
//! can catch up, then drains secondary records before consuming the primary.

use pipe_core::{StageCtx, StreamId};

pub fn fanintwo(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_fanintwo(&args) {
            ctx.fail(format!("FANINTWO: {e}"));
            return;
        }
        run_fanintwo(ctx);
    })
}

fn parse_fanintwo(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_fanintwo(ctx: &mut StageCtx) {
    // p.340 commit: primary output is the only connected output.
    if ctx
        .connected_outputs()
        .iter()
        .any(|s| *s != StreamId::PRIMARY)
    {
        ctx.fail("FANINTWO: the primary output stream is the only connected output stream allowed");
        return;
    }
    if ctx.connected_inputs().iter().any(|s| s.0 > 1) {
        ctx.fail("FANINTWO: only the primary and secondary input streams may be connected");
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

        if ctx.input_record_ready(StreamId::SECONDARY) {
            if !pass_secondary(ctx) {
                short_inputs(ctx);
                return;
            }
            continue;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            // p.340: when a primary record is available, suspend, then test
            // whether a secondary record arrived as well.
            if ctx.yield_others().is_err() {
                short_inputs(ctx);
                return;
            }
            while ctx.input_record_ready(StreamId::SECONDARY) {
                if !pass_secondary(ctx) {
                    short_inputs(ctx);
                    return;
                }
            }
            if !ctx.output_connected(StreamId::PRIMARY) {
                short_inputs(ctx);
                return;
            }
            if ctx.input_record_ready(StreamId::SECONDARY) {
                continue;
            }
            if !pass_primary(ctx) {
                short_inputs(ctx);
                return;
            }
            continue;
        }

        let primary_eof = ctx.input_at_eof(StreamId::PRIMARY);
        let secondary_eof = ctx.input_at_eof(StreamId::SECONDARY);
        if primary_eof && secondary_eof {
            return;
        }

        let mut wait = Vec::new();
        if !primary_eof {
            wait.push(StreamId::PRIMARY);
        }
        if ctx.input_connected(StreamId::SECONDARY) && !secondary_eof {
            wait.push(StreamId::SECONDARY);
        }
        if wait.is_empty() {
            return;
        }
        if ctx.wait_any_input(&wait).is_err() {
            short_inputs(ctx);
            return;
        }
    }
}

/// Secondary strictly does not delay: peek, write, consume (p.340).
fn pass_secondary(ctx: &mut StageCtx) -> bool {
    if !ctx.output_connected(StreamId::PRIMARY) {
        return false;
    }
    ctx.select_input(StreamId::SECONDARY);
    let Some(r) = ctx.peek() else {
        return true;
    };
    ctx.select_output(StreamId::PRIMARY);
    if ctx.write(r).is_err() {
        return false;
    }
    let _ = ctx.read();
    true
}

/// Write the peeked primary record, drain any secondary arrivals, then consume
/// the primary (p.340: delay primary by those secondary records).
fn pass_primary(ctx: &mut StageCtx) -> bool {
    ctx.select_input(StreamId::PRIMARY);
    let Some(r) = ctx.peek() else {
        return true;
    };
    ctx.select_output(StreamId::PRIMARY);
    if ctx.write(r).is_err() {
        return false;
    }
    while ctx.input_record_ready(StreamId::SECONDARY) {
        if !pass_secondary(ctx) {
            let _ = ctx.read();
            return false;
        }
    }
    ctx.select_input(StreamId::PRIMARY);
    let _ = ctx.read();
    true
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}
