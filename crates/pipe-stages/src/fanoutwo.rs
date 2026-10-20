//! FANOUTWO — Author's Edition 1.1.11 pp.342–343 (tutorial p.90 Figure 180).
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `FANOUTWO`
//!
//! Gateway. Specialised FANOUT: copy each primary-input record first to the
//! primary output and then to the secondary output. End-of-file on the
//! primary output stops the stage (EOF travels backwards). End-of-file on
//! the secondary output shorts remaining input onto the primary output so
//! the copy path cannot stall the main stream.

use pipe_core::{StageCtx, StreamId};

pub fn fanoutwo(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_fanoutwo(&args) {
            ctx.fail(format!("FANOUTWO: {e}"));
            return;
        }
        run_fanoutwo(ctx);
    })
}

fn parse_fanoutwo(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn stream_defined(ctx: &StageCtx, stream: StreamId) -> bool {
    ctx.input_connected(stream) || ctx.output_connected(stream)
}

fn run_fanoutwo(ctx: &mut StageCtx) {
    if ctx.connected_inputs().iter().any(|s| s.0 > 0) {
        ctx.fail("FANOUTWO: no input stream other than the primary may be connected");
        return;
    }
    // p.342: two streams must be defined. p.343 commit: secondary input is not
    // connected (covered by the extra-input check).
    if !stream_defined(ctx, StreamId::SECONDARY) {
        ctx.fail("FANOUTWO: two streams must be defined");
        return;
    }
    if ctx.connected_outputs().iter().any(|s| s.0 > 1)
        || ctx.connected_inputs().iter().any(|s| s.0 > 1)
    {
        ctx.fail("FANOUTWO: only the primary and secondary streams may be defined");
        return;
    }

    // Premature termination: EOF on the primary output (p.343).
    if !ctx.output_connected(StreamId::PRIMARY) {
        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
        }
        return;
    }

    ctx.select_input(StreamId::PRIMARY);

    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
            }
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(r) = ctx.peek() else {
                continue;
            };
            ctx.select_output(StreamId::PRIMARY);
            if ctx.write(r.clone()).is_err() {
                // Primary output at EOF: stop without consuming (p.342).
                // short_input drops the slot so a producer is not parked
                // (our SHORT severs; it does not splice).
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
                return;
            }
            if ctx.output_connected(StreamId::SECONDARY) {
                ctx.select_output(StreamId::SECONDARY);
                if ctx.write(r).is_err() {
                    // Secondary at EOF: consume this record (already on
                    // primary), then copy remaining input to primary only.
                    let _ = ctx.read();
                    copy_to_primary(ctx);
                    return;
                }
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

fn copy_to_primary(ctx: &mut StageCtx) {
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(r) = ctx.peek() else {
                continue;
            };
            if ctx.write(r).is_err() {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
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
