//! NOT — Author's Edition 1.1.11 pp.441–442.
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `NOT word [string]`
//!
//! Control. Runs the specified stage (most often a selection stage) with its
//! output streams inverted: the nested primary output is connected to NOT’s
//! secondary output if that stream is defined; the nested secondary output is
//! connected to NOT’s primary output.
//!
//! `word` is any registered stage (the book assumes a selection stage that
//! supports two output streams; it does not enforce a type). Further tokens
//! are that stage’s operands, parsed once with NOT (Note 2).

use pipe_core::{StageCtx, StreamId};

use crate::registry::StageRegistry;

pub fn not_stage(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_not(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("NOT: {e}"));
                return;
            }
        };
        run_not(ctx, spec);
    })
}

struct NotSpec {
    verb: String,
    inner_args: Vec<String>,
}

fn parse_not(args: &[String]) -> Result<NotSpec, String> {
    if args.is_empty() {
        return Err("a word (the name of the program to run) is required".into());
    }
    let verb = args[0].clone();
    let registry = StageRegistry::default();
    if !registry.contains(&verb) {
        return Err(format!("unknown stage: {verb}"));
    }
    Ok(NotSpec {
        verb,
        inner_args: args[1..].to_vec(),
    })
}

fn stream_defined(ctx: &StageCtx, stream: StreamId) -> bool {
    ctx.input_connected(stream) || ctx.output_connected(stream)
}

fn run_not(ctx: &mut StageCtx, spec: NotSpec) {
    // p.441: no input other than primary; commit check is secondary input.
    if ctx
        .connected_inputs()
        .iter()
        .any(|&s| s != StreamId::PRIMARY)
    {
        ctx.fail("NOT: secondary input stream must not be connected");
        return;
    }
    if ctx
        .connected_outputs()
        .iter()
        .any(|&s| s.0 > StreamId::SECONDARY.0)
        || stream_defined(ctx, StreamId::TERTIARY)
    {
        ctx.fail("NOT: at most two streams may be defined");
        return;
    }

    // Premature termination: the called stage terminates when it has no
    // output. Equivalent to not spawning when NOT itself has no output.
    if ctx.connected_outputs().is_empty() {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    let registry = StageRegistry::default();
    let inner = match registry.create(&spec.verb, spec.inner_args.clone()) {
        Some(s) => s,
        None => {
            ctx.fail(format!("NOT: unknown stage: {}", spec.verb));
            return;
        }
    };
    let sel = ctx.spawn_stage(spec.verb.clone(), inner);

    // Primary input of NOT is the nested primary input (p.441).
    ctx.retarget_input(StreamId::PRIMARY, sel, StreamId::PRIMARY);

    // Nested secondary output → NOT primary output.
    ctx.retarget_output(StreamId::PRIMARY, sel, StreamId::SECONDARY);

    // Nested primary output → NOT secondary output, only if that stream
    // is defined (p.441). Otherwise selected records are discarded.
    if ctx.output_connected(StreamId::SECONDARY) {
        ctx.retarget_output(StreamId::SECONDARY, sel, StreamId::PRIMARY);
    }
}
