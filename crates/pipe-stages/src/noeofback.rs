//! NOEOFBACK — Author's Edition 1.1.11 pp.440–441.
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `NOEOFBACK`
//!
//! Filter. Passes records while the output stream is connected. When the
//! output is at end-of-file, consumes the remaining input. Propagates
//! end-of-file forwards, but not backwards. Synonym: `NOTEOFBACK` (p.441).

use pipe_core::{StageCtx, StreamId};

pub fn noeofback(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_noeofback(&args) {
            ctx.fail(format!("NOEOFBACK: {e}"));
            return;
        }
        run_noeofback(ctx);
    })
}

fn parse_noeofback(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_noeofback(ctx: &mut StageCtx) {
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    loop {
        // Output at end-of-file: consume remaining input; do not short
        // the producer (p.440: EOF travels forwards, not backwards).
        if !ctx.output_connected(StreamId::PRIMARY) {
            consume_remaining(ctx);
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(r) = ctx.peek() else {
                continue;
            };
            if ctx.write(r).is_err() {
                consume_remaining(ctx);
                return;
            }
            // Consume after the write so the record is not delayed (p.440).
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

fn consume_remaining(ctx: &mut StageCtx) {
    ctx.select_input(StreamId::PRIMARY);
    while let Ok(Some(_)) = ctx.read() {}
}
