//! PREDSELECT — Author's Edition 1.1.11 pp.459–460 (tutorial p.85 Figure 175;
//! FANOUT example p.341).
//!
//! Syntax (diagram `PREDSELect`, min 7):
//! `PREDSELect`
//!
//! Gateway. Stores each primary input record in a one-record buffer and
//! consumes it (the “safe place” while a copy is tested destructively). A
//! record on the secondary input writes that buffer to the primary output; a
//! record on the tertiary input writes it to the secondary output. The trigger
//! records themselves are discarded.

use pipe_core::{Record, StageCtx, StreamId};

pub fn predselect(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_predselect(&args) {
            ctx.fail(format!("PREDSELECT: {e}"));
            return;
        }
        run_predselect(ctx);
    })
}

fn parse_predselect(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn stream_defined(ctx: &StageCtx, stream: StreamId) -> bool {
    ctx.input_connected(stream) || ctx.output_connected(stream)
}

fn run_predselect(ctx: &mut StageCtx) {
    if ctx.connected_inputs().iter().any(|s| s.0 > 2)
        || ctx.connected_outputs().iter().any(|s| s.0 > 2)
    {
        ctx.fail("PREDSELECT: at most three streams may be defined");
        return;
    }

    // Commit check (p.459): tertiary output must not be connected.
    if ctx.output_connected(StreamId::TERTIARY) {
        ctx.fail("PREDSELECT: tertiary output stream must not be connected");
        return;
    }

    // p.460: two streams must be defined; up to three may be defined.
    if !stream_defined(ctx, StreamId::SECONDARY) {
        ctx.fail("PREDSELECT: two streams must be defined");
        return;
    }

    if ctx.connected_outputs().is_empty() {
        short_inputs(ctx);
        return;
    }

    let mut buffer: Option<Record> = None;

    loop {
        if ctx.connected_outputs().is_empty() {
            short_inputs(ctx);
            return;
        }

        // p.460: terminate as soon as the primary input is severed, once
        // there is no buffered original still waiting for a trigger.
        if !ctx.input_connected(StreamId::PRIMARY) && buffer.is_none() {
            short_inputs(ctx);
            return;
        }

        propagate_eof_pairs(ctx);

        if ctx.connected_outputs().is_empty() {
            short_inputs(ctx);
            return;
        }

        let streams = wait_streams(ctx, buffer.is_some());
        if streams.is_empty() {
            short_inputs(ctx);
            return;
        }

        match ctx.read_any(&streams) {
            Ok(Some((stream, rec))) if stream == StreamId::PRIMARY => {
                // Replace the one-record buffer. A previous record with no
                // intervening trigger is discarded (p.459).
                buffer = Some(rec);
            }
            Ok(Some((stream, _rec))) if stream == StreamId::SECONDARY => {
                if let Some(orig) = buffer.take() {
                    if !emit(ctx, StreamId::PRIMARY, orig) {
                        short_inputs(ctx);
                        return;
                    }
                }
            }
            Ok(Some((stream, _rec))) if stream == StreamId::TERTIARY => {
                if let Some(orig) = buffer.take() {
                    if !emit(ctx, StreamId::SECONDARY, orig) {
                        short_inputs(ctx);
                        return;
                    }
                }
            }
            Ok(Some(_)) => unreachable!("predselect reads only streams 0–2"),
            Ok(None) => {
                short_inputs(ctx);
                return;
            }
            Err(_) => {
                short_inputs(ctx);
                return;
            }
        }
    }
}

/// While a primary record is buffered, wait only for a trigger. FANOUT can
/// already have placed the next original in the primary slot (p.85 topology);
/// consuming it before the selection stage writes would discard the current
/// original. A new primary is read only after the buffer is written, or after
/// the trigger streams reach end-of-file (the previous record is then
/// discarded, p.459).
fn wait_streams(ctx: &StageCtx, armed: bool) -> Vec<StreamId> {
    let mut v = Vec::new();
    if armed {
        for s in [StreamId::SECONDARY, StreamId::TERTIARY] {
            if ctx.input_connected(s) {
                v.push(s);
            }
        }
        if v.iter().all(|&s| ctx.input_at_eof(s))
            && ctx.input_connected(StreamId::PRIMARY)
            && !ctx.input_at_eof(StreamId::PRIMARY)
        {
            v.push(StreamId::PRIMARY);
        }
    } else {
        if ctx.input_connected(StreamId::PRIMARY) {
            v.push(StreamId::PRIMARY);
        }
        for s in [StreamId::SECONDARY, StreamId::TERTIARY] {
            if ctx.input_connected(s) {
                v.push(s);
            }
        }
    }
    v
}

/// End-of-file is paired across streams (p.460): secondary in ↔ primary out,
/// tertiary in ↔ secondary out. Unconnected sides are left alone.
fn propagate_eof_pairs(ctx: &mut StageCtx) {
    pair_eof(ctx, StreamId::SECONDARY, StreamId::PRIMARY);
    pair_eof(ctx, StreamId::TERTIARY, StreamId::SECONDARY);
}

fn pair_eof(ctx: &mut StageCtx, trigger_in: StreamId, routed_out: StreamId) {
    if ctx.input_connected(trigger_in) && ctx.input_at_eof(trigger_in) {
        if ctx.output_connected(routed_out) {
            ctx.select_output(routed_out);
            ctx.sever_output();
        }
        if ctx.input_connected(trigger_in) {
            ctx.select_input(trigger_in);
            ctx.short_input();
        }
    } else if ctx.input_connected(trigger_in) && !ctx.output_connected(routed_out) {
        ctx.select_input(trigger_in);
        ctx.short_input();
    }
}

fn emit(ctx: &mut StageCtx, stream: StreamId, rec: Record) -> bool {
    if ctx.output_connected(stream) {
        ctx.select_output(stream);
        if ctx.write(rec).is_err() {
            return any_output_live(ctx);
        }
    }
    any_output_live(ctx)
}

fn any_output_live(ctx: &StageCtx) -> bool {
    !ctx.connected_outputs().is_empty()
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}
