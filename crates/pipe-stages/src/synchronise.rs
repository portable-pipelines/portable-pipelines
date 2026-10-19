//! SYNCHRONISE — Author's Edition 1.1.11 pp.536–539 (examples pp.537–538;
//! tutorial p.84; dispatcher delay p.219).
//!
//! Syntax (diagram mixed-case prefix SYNC):
//! `SYNChronise` / `SYNChronize`
//!
//! Arcane gateway. Peeks every connected input in numerical order until each
//! has a record, writes one record to each corresponding output in that same
//! order, then consumes the inputs. Terminates on end-of-file of any input or
//! output without consuming the peeked records of that cycle.

use pipe_core::{Record, StageCtx, StreamId};

pub fn synchronise(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_synchronise(&args) {
            ctx.fail(format!("SYNCHRONISE: {e}"));
            return;
        }
        run_synchronise(ctx);
    })
}

fn parse_synchronise(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_synchronise(ctx: &mut StageCtx) {
    loop {
        let inputs = ctx.connected_inputs();
        if inputs.is_empty() {
            return;
        }

        // Peek every input in numerical order (p.537). A higher stream that is
        // already at EOF is not observed until lower streams have been peeked.
        let mut recs: Vec<(StreamId, Record)> = Vec::new();
        for &stream in &inputs {
            match wait_peek(ctx, stream) {
                Some(r) => recs.push((stream, r)),
                None => {
                    // EOF / abort on this input: lower-numbered streams still
                    // hold a record (p.537). Do not consume; short so producers
                    // are not parked on a full slot after this stage ends.
                    short_inputs(ctx, &ctx.connected_inputs());
                    return;
                }
            }
        }

        // Write corresponding outputs in numerical order only after every
        // input has been peeked (p.537).
        for (stream, rec) in &recs {
            ctx.select_output(*stream);
            if !ctx.output_connected(*stream) {
                short_inputs(ctx, &ctx.connected_inputs());
                return;
            }
            if ctx.write(rec.clone()).is_err() {
                short_inputs(ctx, &ctx.connected_inputs());
                return;
            }
        }

        // Consume only after every write succeeded (p.537).
        for (stream, _) in &recs {
            ctx.select_input(*stream);
            let _ = ctx.read();
        }
    }
}

/// Wait until `stream` has a record or is at end-of-file, then clone the slot.
fn wait_peek(ctx: &mut StageCtx, stream: StreamId) -> Option<Record> {
    ctx.select_input(stream);
    loop {
        if ctx.input_record_ready(stream) {
            return ctx.peek();
        }
        if ctx.input_at_eof(stream) {
            return None;
        }
        if ctx.wait_any_input(&[stream]).is_err() {
            return None;
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
