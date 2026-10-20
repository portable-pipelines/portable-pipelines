//! FRTARGET / TOTARGET — Author's Edition 1.1.11 pp.354–355, 560–561.
//!
//! Syntax (diagrams all-caps; no verb prefix):
//! `FRTARGET word [string]`  (synonym `FROMTARGET`)
//! `TOTARGET word [string]`
//!
//! Control. The argument is a selection stage run as a subroutine (same
//! `word [string]` hosting as NOT / CASEI / IF / ZONE). Records the nested
//! stage writes to its secondary are the rejects; the first nested primary
//! write is the trigger. FRTARGET then shorts the trigger and remaining
//! input to its primary; TOTARGET shorts them to its secondary (if defined).
//!
//! Topology reuses CASEI’s feed / nested / restore `spawn_stage` wiring so
//! the dispatcher is unchanged.

use pipe_core::{Record, StageCtx, StreamId};

use crate::registry::StageRegistry;

#[derive(Clone, Copy)]
enum Kind {
    /// Rejects → secondary; trigger + rest → primary (p.354).
    Fr,
    /// Rejects → primary; trigger + rest → secondary (p.560).
    To,
}

struct Spec {
    kind: Kind,
    verb: String,
    inner_args: Vec<String>,
}

pub fn frtarget(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    host("FRTARGET", Kind::Fr, args)
}

pub fn totarget(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    host("TOTARGET", Kind::To, args)
}

fn host(verb: &'static str, kind: Kind, args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_target(verb, kind, &args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("{verb}: {e}"));
                return;
            }
        };
        run_target(ctx, spec);
    })
}

fn parse_target(_host: &str, kind: Kind, args: &[String]) -> Result<Spec, String> {
    if args.is_empty() {
        return Err("a word (the name of the program to run) is required".into());
    }
    let verb = args[0].clone();
    let registry = StageRegistry::default();
    if !registry.contains(&verb) {
        return Err(format!("unknown stage: {verb}"));
    }
    Ok(Spec {
        kind,
        verb,
        inner_args: args[1..].to_vec(),
    })
}

fn extra_output_connected(ctx: &StageCtx) -> bool {
    ctx.connected_outputs()
        .iter()
        .any(|&s| s.0 > StreamId::SECONDARY.0)
}

fn extra_input_above_secondary(ctx: &StageCtx) -> bool {
    ctx.connected_inputs()
        .iter()
        .any(|&s| s.0 > StreamId::SECONDARY.0)
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

fn run_target(ctx: &mut StageCtx, spec: Spec) {
    if extra_input_above_secondary(ctx) {
        ctx.fail(format!(
            "{}: at most two streams may be defined",
            match spec.kind {
                Kind::Fr => "FRTARGET",
                Kind::To => "TOTARGET",
            }
        ));
        return;
    }
    if extra_output_connected(ctx) {
        ctx.fail(format!(
            "{}: at most two streams may be defined",
            match spec.kind {
                Kind::Fr => "FRTARGET",
                Kind::To => "TOTARGET",
            }
        ));
        return;
    }

    if !outputs_live(ctx) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    let registry = StageRegistry::default();
    let inner = match registry.create(&spec.verb, spec.inner_args.clone()) {
        Some(s) => s,
        None => {
            ctx.fail(format!(
                "{}: unknown stage: {}",
                match spec.kind {
                    Kind::Fr => "FRTARGET",
                    Kind::To => "TOTARGET",
                },
                spec.verb
            ));
            return;
        }
    };

    let feed = ctx.spawn_stage("target-feed", target_feed());
    let sel = ctx.spawn_stage(spec.verb.clone(), inner);
    let restore = ctx.spawn_stage("target-restore", target_restore(spec.kind));

    ctx.retarget_input(StreamId::PRIMARY, feed, StreamId::PRIMARY);
    // p.355 / p.560: if the host secondary input is connected, the argument
    // stage must support a connected secondary input.
    if ctx.input_connected(StreamId::SECONDARY) {
        ctx.retarget_input(StreamId::SECONDARY, sel, StreamId::SECONDARY);
    }

    // Originals first so restore arms before the nested stage can write.
    ctx.connect_streams(feed, StreamId::SECONDARY, restore, StreamId::PRIMARY);
    ctx.connect_streams(feed, StreamId::PRIMARY, sel, StreamId::PRIMARY);
    ctx.connect_streams(sel, StreamId::PRIMARY, restore, StreamId::SECONDARY);
    ctx.connect_streams(sel, StreamId::SECONDARY, restore, StreamId::TERTIARY);
    ctx.retarget_output(StreamId::PRIMARY, restore, StreamId::PRIMARY);
    if ctx.output_connected(StreamId::SECONDARY) {
        ctx.retarget_output(StreamId::SECONDARY, restore, StreamId::SECONDARY);
    }
}

/// Copy each original to restore, then a working copy to the nested stage.
fn target_feed() -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        ctx.select_input(StreamId::PRIMARY);
        loop {
            if !ctx.output_connected(StreamId::SECONDARY) {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
                return;
            }
            if ctx.input_record_ready(StreamId::PRIMARY) {
                let Some(orig) = ctx.peek() else {
                    continue;
                };
                ctx.select_output(StreamId::SECONDARY);
                if ctx.write(orig.clone()).is_err() {
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.short_input();
                    return;
                }
                if ctx.output_connected(StreamId::PRIMARY) {
                    ctx.select_output(StreamId::PRIMARY);
                    if ctx.write(orig).is_err() {
                        // Nested no longer wants copies (trigger fired).
                        ctx.select_output(StreamId::PRIMARY);
                        ctx.short_output();
                    }
                }
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
    })
}

fn target_restore(kind: Kind) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| loop {
        if !outputs_live(ctx) {
            short_inputs(ctx);
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            let Some(orig) = ctx.read().ok().flatten() else {
                short_inputs(ctx);
                return;
            };
            match classify(ctx) {
                Classify::Rejected => {
                    let dest = match kind {
                        Kind::Fr => StreamId::SECONDARY,
                        Kind::To => StreamId::PRIMARY,
                    };
                    if !emit(ctx, dest, orig) {
                        short_inputs(ctx);
                        return;
                    }
                }
                Classify::Trigger => {
                    on_trigger(ctx, kind, orig);
                    return;
                }
                Classify::Dropped => {
                    if !outputs_live(ctx) {
                        short_inputs(ctx);
                        return;
                    }
                }
                Classify::Dead => {
                    short_inputs(ctx);
                    return;
                }
            }
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            short_inputs(ctx);
            return;
        }
    })
}

enum Classify {
    /// Nested wrote secondary (not selected).
    Rejected,
    /// Nested wrote primary (selected / trigger).
    Trigger,
    /// Nested wrote nothing for this original.
    Dropped,
    /// Nested streams failed.
    Dead,
}

fn classify(ctx: &mut StageCtx) -> Classify {
    loop {
        if ctx.input_record_ready(StreamId::SECONDARY) {
            ctx.select_input(StreamId::SECONDARY);
            let _ = ctx.read();
            return Classify::Trigger;
        }
        if ctx.input_record_ready(StreamId::TERTIARY) {
            ctx.select_input(StreamId::TERTIARY);
            let _ = ctx.read();
            return Classify::Rejected;
        }
        if ctx.input_at_eof(StreamId::SECONDARY) && ctx.input_at_eof(StreamId::TERTIARY) {
            return Classify::Dropped;
        }
        if ctx
            .wait_any_input(&[StreamId::SECONDARY, StreamId::TERTIARY])
            .is_err()
        {
            return Classify::Dead;
        }
    }
}

fn on_trigger(ctx: &mut StageCtx, kind: Kind, orig: Record) {
    // Stop the nested stage: further copies are not classified.
    short_nested_outputs(ctx);
    match kind {
        Kind::Fr => {
            if ctx.output_connected(StreamId::SECONDARY) {
                ctx.select_output(StreamId::SECONDARY);
                ctx.sever_output();
            }
            if !emit(ctx, StreamId::PRIMARY, orig) {
                short_inputs(ctx);
                return;
            }
            copy_remaining(ctx, StreamId::PRIMARY);
        }
        Kind::To => {
            if ctx.output_connected(StreamId::PRIMARY) {
                ctx.select_output(StreamId::PRIMARY);
                ctx.sever_output();
            }
            if ctx.output_connected(StreamId::SECONDARY) {
                if !emit(ctx, StreamId::SECONDARY, orig) {
                    short_inputs(ctx);
                    return;
                }
                copy_remaining(ctx, StreamId::SECONDARY);
            } else {
                // Secondary not defined: drop the trigger and remaining
                // originals (subset of p.559 leftover for CALLPIPE).
                short_inputs(ctx);
            }
        }
    }
}

fn short_nested_outputs(ctx: &mut StageCtx) {
    for stream in [StreamId::SECONDARY, StreamId::TERTIARY] {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}

fn copy_remaining(ctx: &mut StageCtx, dest: StreamId) {
    ctx.select_input(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(dest) {
            short_inputs(ctx);
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            let Some(rec) = ctx.read().ok().flatten() else {
                short_inputs(ctx);
                return;
            };
            if !emit(ctx, dest, rec) {
                short_inputs(ctx);
                return;
            }
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            short_inputs(ctx);
            return;
        }
    }
}

fn emit(ctx: &mut StageCtx, stream: StreamId, rec: Record) -> bool {
    if ctx.output_connected(stream) {
        ctx.select_output(stream);
        if ctx.write(rec).is_err() {
            return outputs_live(ctx);
        }
    }
    outputs_live(ctx)
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}
