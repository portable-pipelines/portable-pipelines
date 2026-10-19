//! GATE — Author's Edition 1.1.11 pp.365–366 (examples pp.366, 441, 458, 518).
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `GATE [STRICT]`
//!
//! Gateway. Copies records from secondary and higher-numbered input streams
//! to the corresponding output streams until a record arrives on the primary
//! input. That primary stream is then shorted to the primary output and GATE
//! terminates, so the other streams see end-of-file.

use pipe_core::{StageCtx, StreamId};

pub fn gate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let strict = match parse_gate(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("GATE: {e}"));
                return;
            }
        };
        run_gate(ctx, strict);
    })
}

fn parse_gate(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [s] if s.eq_ignore_ascii_case("STRICT") => Ok(true),
        [s] => Err(format!("unrecognized operand {s}")),
        [s, rest, ..] if s.eq_ignore_ascii_case("STRICT") => {
            Err(format!("unrecognized operand {rest}"))
        }
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_gate(ctx: &mut StageCtx, strict: bool) {
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

        let inputs = ctx.connected_inputs();
        if inputs.is_empty() || inputs.iter().all(|&s| ctx.input_at_eof(s)) {
            return;
        }

        let higher = ready_higher(ctx);
        let primary_ready = ctx.input_record_ready(StreamId::PRIMARY);

        // STRICT: a ready primary record stops the gate before any higher
        // record is consumed (p.365). Without STRICT the dispatcher may still
        // pass a higher record that is already waiting.
        if primary_ready && (strict || higher.is_none()) {
            stop_on_primary(ctx);
            return;
        }

        if let Some(stream) = higher {
            if strict && ctx.input_record_ready(StreamId::PRIMARY) {
                stop_on_primary(ctx);
                return;
            }
            pass_one(ctx, stream);
            continue;
        }

        if inputs.iter().any(|&s| ctx.input_at_eof(s)) {
            continue;
        }

        if ctx.wait_any_input(&inputs).is_err() {
            short_inputs(ctx, &ctx.connected_inputs());
            return;
        }
    }
}

/// A record on primary stops the gate: short primary in to primary out, then
/// the remaining higher stream pairs see end-of-file (p.365).
fn stop_on_primary(ctx: &mut StageCtx) {
    let higher_in: Vec<StreamId> = ctx
        .connected_inputs()
        .into_iter()
        .filter(|s| *s != StreamId::PRIMARY)
        .collect();
    short_inputs(ctx, &higher_in);

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    if ctx.output_connected(StreamId::PRIMARY) {
        // Approximate IBM SHORT (splice) by copying the rest of the primary
        // file, including the in-flight record. End-of-file on the primary
        // output is ignored (p.365).
        while let Ok(Some(r)) = ctx.read() {
            let _ = ctx.write(r);
        }
    } else if ctx.input_connected(StreamId::PRIMARY) {
        ctx.short_input();
    }
}

fn pass_one(ctx: &mut StageCtx, stream: StreamId) {
    ctx.select_input(stream);
    match ctx.read() {
        Ok(Some(r)) => {
            if ctx.output_connected(stream) {
                ctx.select_output(stream);
                if ctx.write(r).is_err() {
                    // Output severed: propagate end-of-file back (p.365).
                    if ctx.input_connected(stream) {
                        ctx.select_input(stream);
                        ctx.short_input();
                    }
                }
            } else if ctx.input_connected(stream) {
                ctx.select_input(stream);
                ctx.short_input();
            }
        }
        _ => {
            if ctx.output_connected(stream) {
                ctx.select_output(stream);
                ctx.sever_output();
            }
            if ctx.input_connected(stream) {
                ctx.select_input(stream);
                ctx.short_input();
            }
        }
    }
}

fn ready_higher(ctx: &StageCtx) -> Option<StreamId> {
    ctx.connected_inputs()
        .into_iter()
        .filter(|s| *s != StreamId::PRIMARY)
        .find(|&s| ctx.input_record_ready(s))
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
