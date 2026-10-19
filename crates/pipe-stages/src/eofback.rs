//! EOFBACK — Author's Edition 1.1.11 p.337 (message 39I example p.665).
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `EOFBACK word [string]`
//!
//! Control. Passes each input record to the primary output. When that output
//! record has been consumed, the same record is passed to the specified output
//! device driver, whose output stream is not connected. Downstream end-of-file
//! therefore stops both the copy and the driver (EOF propagates backwards).

use pipe_core::{StageCtx, StreamId};
use pipe_parser::operands_for_stage;

use crate::registry::StageRegistry;

pub fn eofback(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_eofback(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("EOFBACK: {e}"));
                return;
            }
        };
        run_eofback(ctx, spec);
    })
}

struct EofbackSpec {
    verb: String,
    inner_args: Vec<String>,
}

fn parse_eofback(args: &[String]) -> Result<EofbackSpec, String> {
    if args.is_empty() {
        return Err("a word (the name of the program to run) is required".into());
    }
    let verb = args[0].clone();
    let registry = StageRegistry::default();
    if !registry.contains(&verb) {
        return Err(format!("unknown stage: {verb}"));
    }
    let inner_raw = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let inner_args = operands_for_stage(&verb, inner_raw);
    Ok(EofbackSpec { verb, inner_args })
}

fn extra_stream_connected(ctx: &StageCtx) -> bool {
    ctx.connected_inputs()
        .iter()
        .any(|&s| s != StreamId::PRIMARY)
        || ctx
            .connected_outputs()
            .iter()
            .any(|&s| s != StreamId::PRIMARY)
}

fn run_eofback(ctx: &mut StageCtx, spec: EofbackSpec) {
    if extra_stream_connected(ctx) {
        ctx.fail("EOFBACK: only the primary streams may be connected");
        return;
    }

    // Premature termination (p.337): output not connected — do not run the
    // driver; short the producer so EOF travels backwards.
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    let registry = StageRegistry::default();
    let inner = match registry.create(&spec.verb, spec.inner_args) {
        Some(s) => s,
        None => {
            ctx.fail(format!("EOFBACK: unknown stage: {}", spec.verb));
            return;
        }
    };
    let driver = ctx.spawn_stage(spec.verb.clone(), inner);
    // Nested driver's output is not connected (p.337). Feed it from this
    // stage's secondary output after each primary record is consumed.
    ctx.connect_streams(
        ctx.stage_id(),
        StreamId::SECONDARY,
        driver,
        StreamId::PRIMARY,
    );

    ctx.select_input(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(r) = ctx.peek() else {
                continue;
            };
            ctx.select_output(StreamId::PRIMARY);
            if ctx.write(r.clone()).is_err() {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
                return;
            }
            // Wait until the consumer has taken the record (p.337).
            ctx.select_output(StreamId::PRIMARY);
            if ctx.wait_output().is_err() {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
                return;
            }
            ctx.select_output(StreamId::SECONDARY);
            if ctx.write(r).is_err() {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
                return;
            }
            // Consume after the primary write so the record is not delayed.
            ctx.select_input(StreamId::PRIMARY);
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
