//! FILLUP — Author's Edition 1.1.11 p.345.
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `FILLUP`
//!
//! Gateway. Pass records from the primary input to output streams as they
//! fill up: primary first, then secondary, and so on. When a consumer severs
//! (EOF backwards), switch to the next connected output. When only one
//! output remains, copy remaining input there (IBM SHORT splice stand-in).

use pipe_core::{StageCtx, StreamId};

pub fn fillup(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_fillup(&args) {
            ctx.fail(format!("FILLUP: {e}"));
            return;
        }
        run_fillup(ctx);
    })
}

fn parse_fillup(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_fillup(ctx: &mut StageCtx) {
    if ctx.connected_inputs().iter().any(|s| s.0 > 0) {
        ctx.fail("FILLUP: no input stream other than the primary may be connected");
        return;
    }

    // Premature termination: primary output is not connected (p.345).
    if !ctx.output_connected(StreamId::PRIMARY) {
        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
        }
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    let mut current = StreamId::PRIMARY;

    loop {
        if !ctx.output_connected(current) {
            match next_output(ctx) {
                None => {
                    stop_unconsumed(ctx);
                    return;
                }
                Some(s) => current = s,
            }
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(r) = ctx.peek() else {
                continue;
            };
            ctx.select_output(current);
            if ctx.write(r).is_err() {
                match next_output(ctx) {
                    None => {
                        stop_unconsumed(ctx);
                        return;
                    }
                    Some(s) => {
                        current = s;
                        continue;
                    }
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

fn next_output(ctx: &StageCtx) -> Option<StreamId> {
    ctx.connected_outputs().into_iter().next()
}

fn stop_unconsumed(ctx: &mut StageCtx) {
    if ctx.input_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
    }
}
