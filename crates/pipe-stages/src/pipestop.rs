//! PIPESTOP — Author's Edition 1.1.11 pp.458–459 (tutorial p.150; DELAY p.322).
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `PIPESTOP`
//!
//! Arcane control. When an input record arrives, posts the portable stand-in
//! for CMS ECBs that other stages wait on for external/time events, sets the
//! irreversible “no further waiting” indication, then copies the record to
//! the primary output if it is connected. Strictly does not delay: peek,
//! post, write, then consume.

use pipe_core::{StageCtx, StreamId};

pub fn pipestop(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_pipestop(&args) {
            ctx.fail(format!("PIPESTOP: {e}"));
            return;
        }
        run_pipestop(ctx);
    })
}

fn parse_pipestop(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_pipestop(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("PIPESTOP: only the primary input may be connected");
            return;
        }
    }

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    loop {
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                return;
            };
            // p.459: when a record arrives, post ECBs, then pass the record
            // if the output is connected. Unconnected write discards (last
            // stage is the book example).
            ctx.stop_external_waits();
            if ctx.write(rec).is_err() {
                if ctx.input_connected(StreamId::PRIMARY) {
                    ctx.short_input();
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_undocumented_operands() {
        assert!(parse_pipestop(&[]).is_ok());
        for w in ["FOREVER", "STRICT", "STOP", "ALLEOF", "1", "+60", "/x/"] {
            let e = parse_pipestop(&[w.into()]).unwrap_err();
            assert!(e.contains("unrecognized operand"), "{w}: {e}");
            assert!(e.contains(w), "{w}: {e}");
        }
    }
}
