//! DAM — Author's Edition 1.1.11 pp.310–311 (example p.311).
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `DAM`
//!
//! Gateway. Waits for the first record on the primary input, then shorts every
//! input to the corresponding output in numerical order (the dam bursts).
//! Converse of GATE: higher stream pairs are not copied while waiting.

use pipe_core::{StageCtx, StreamId};

pub fn dam(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_dam(&args) {
            ctx.fail(format!("DAM: {e}"));
            return;
        }
        run_dam(ctx);
    })
}

fn parse_dam(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_dam(ctx: &mut StageCtx) {
    if ctx.connected_outputs().is_empty() {
        short_inputs(ctx, &ctx.connected_inputs());
        return;
    }

    loop {
        if ctx.connected_outputs().is_empty() {
            short_inputs(ctx, &ctx.connected_inputs());
            return;
        }

        propagate_higher_eof(ctx);

        if ctx.input_record_ready(StreamId::PRIMARY) {
            burst(ctx);
            return;
        }

        // Empty primary: terminate without copying and without consuming
        // records sitting on other streams (p.310).
        if ctx.input_at_eof(StreamId::PRIMARY) {
            short_inputs(ctx, &ctx.connected_inputs());
            return;
        }

        // Wait for a primary record/EOF. Also wait on higher streams that are
        // not already holding a record so their EOF can be propagated without
        // spinning on a secondary slot that we must not consume yet.
        let mut wait_on = vec![StreamId::PRIMARY];
        for s in ctx.connected_inputs() {
            if s == StreamId::PRIMARY {
                continue;
            }
            if !ctx.input_record_ready(s) && !ctx.input_at_eof(s) {
                wait_on.push(s);
            }
        }
        if ctx.wait_any_input(&wait_on).is_err() {
            short_inputs(ctx, &ctx.connected_inputs());
            return;
        }
    }
}

/// Short every connected pair in numerical order (p.310). IBM SHORT splices;
/// we copy the remaining file, including the in-flight record, as a stand-in.
fn burst(ctx: &mut StageCtx) {
    let mut streams = ctx.connected_inputs();
    streams.extend(ctx.connected_outputs());
    streams.sort_by_key(|s| s.0);
    streams.dedup();
    for stream in streams {
        splice_pair(ctx, stream);
    }
}

fn splice_pair(ctx: &mut StageCtx, stream: StreamId) {
    ctx.select_input(stream);
    ctx.select_output(stream);
    if ctx.output_connected(stream) {
        loop {
            match ctx.read() {
                Ok(Some(r)) => {
                    if ctx.write(r).is_err() {
                        // Ignore end-of-file on the primary output (p.310).
                        if stream == StreamId::PRIMARY {
                            continue;
                        }
                        if ctx.input_connected(stream) {
                            ctx.select_input(stream);
                            ctx.short_input();
                        }
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    if ctx.output_connected(stream) {
                        ctx.select_output(stream);
                        ctx.sever_output();
                    }
                    if ctx.input_connected(stream) {
                        ctx.select_input(stream);
                        ctx.short_input();
                    }
                    break;
                }
            }
        }
    } else if ctx.input_connected(stream) {
        ctx.short_input();
    }
}

fn propagate_higher_eof(ctx: &mut StageCtx) {
    let mut streams = ctx.connected_inputs();
    streams.extend(ctx.connected_outputs());
    streams.sort_by_key(|s| s.0);
    streams.dedup();
    for stream in streams {
        if stream == StreamId::PRIMARY {
            continue;
        }
        if ctx.input_at_eof(stream) {
            if ctx.output_connected(stream) {
                ctx.select_output(stream);
                ctx.sever_output();
            }
            if ctx.input_connected(stream) {
                ctx.select_input(stream);
                ctx.short_input();
            }
        } else if !ctx.output_connected(stream) && ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}

fn short_inputs(ctx: &mut StageCtx, streams: &[StreamId]) {
    for &stream in streams {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}
