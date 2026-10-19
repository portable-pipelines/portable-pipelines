//! IF — Author's Edition 1.1.11 pp.389–390.
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `IF word [string]`
//!
//! Gateway. Adds a selection stage and a FANINANY stage to the running
//! pipeline set (p.390) and splices them onto IF’s streams:
//!
//! - if-then (two streams): selected records leave on primary; rejected
//!   records are merged with the secondary input and written to the
//!   secondary output.
//! - if-then-else (three streams): selected records leave on primary;
//!   rejected records leave on secondary; secondary and tertiary inputs
//!   are merged onto the tertiary output.
//!
//! `word` is any registered stage (the book assumes a selection stage and
//! does not enforce one — Note 1). Further tokens are that stage’s operands.

use pipe_core::{StageCtx, StreamId};

use crate::registry::StageRegistry;

pub fn if_stage(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_if(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("IF: {e}"));
                return;
            }
        };
        run_if(ctx, spec);
    })
}

struct IfSpec {
    verb: String,
    inner_args: Vec<String>,
}

fn parse_if(args: &[String]) -> Result<IfSpec, String> {
    if args.is_empty() {
        return Err("a word (the name of the program to run) is required".into());
    }
    let verb = args[0].clone();
    let registry = StageRegistry::default();
    if !registry.contains(&verb) {
        return Err(format!("unknown stage: {verb}"));
    }
    Ok(IfSpec {
        verb,
        inner_args: args[1..].to_vec(),
    })
}

fn run_if(ctx: &mut StageCtx, spec: IfSpec) {
    let then_else = stream_defined(ctx, StreamId::TERTIARY);
    if stream_defined(ctx, StreamId(3)) {
        ctx.fail("IF: at most three streams may be defined");
        return;
    }
    if !stream_defined(ctx, StreamId::PRIMARY) || !stream_defined(ctx, StreamId::SECONDARY) {
        ctx.fail("IF: two streams must be defined");
        return;
    }

    let registry = StageRegistry::default();
    let inner = match registry.create(&spec.verb, spec.inner_args.clone()) {
        Some(s) => s,
        None => {
            ctx.fail(format!("IF: unknown stage: {}", spec.verb));
            return;
        }
    };
    let faninany = match registry.create("FANINANY", Vec::new()) {
        Some(s) => s,
        None => {
            ctx.fail("IF: FANINANY is not registered");
            return;
        }
    };

    let sel = ctx.spawn_stage(spec.verb.clone(), inner);
    let fan = ctx.spawn_stage("FANINANY", faninany);

    // Primary pair: caller ↔ selection (p.390 topology, *.input.0 / *.output.0).
    ctx.retarget_input(StreamId::PRIMARY, sel, StreamId::PRIMARY);
    ctx.retarget_output(StreamId::PRIMARY, sel, StreamId::PRIMARY);

    if then_else {
        // Rejected records (selection secondary) go to the else-path
        // (IF secondary output). Then-path return is IF secondary input;
        // else-path return is IF tertiary input; FANINANY writes the
        // tertiary output (p.390).
        ctx.retarget_output(StreamId::SECONDARY, sel, StreamId::SECONDARY);
        ctx.retarget_input(StreamId::SECONDARY, fan, StreamId::PRIMARY);
        ctx.retarget_input(StreamId::TERTIARY, fan, StreamId::SECONDARY);
        ctx.retarget_output(StreamId::TERTIARY, fan, StreamId::PRIMARY);
    } else {
        // if-then: selection secondary is wired directly to FANINANY and
        // merged with the secondary input onto the secondary output (p.390).
        ctx.connect_streams(sel, StreamId::SECONDARY, fan, StreamId::PRIMARY);
        ctx.retarget_input(StreamId::SECONDARY, fan, StreamId::SECONDARY);
        ctx.retarget_output(StreamId::SECONDARY, fan, StreamId::PRIMARY);
    }
}

fn stream_defined(ctx: &StageCtx, stream: StreamId) -> bool {
    ctx.input_connected(stream) || ctx.output_connected(stream)
}
