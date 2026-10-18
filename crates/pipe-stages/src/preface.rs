//! PREFACE / APPEND — Author's Edition 1.1.11 pp.461–462 / pp.263–264.
//!
//! Syntax: `PREFACE string` / `APPEND string`. Diagrams are all-caps (no prefix).
//! The argument is a subroutine pipeline issued with CALLPIPE (separator `|`,
//! escape `"`, endchar `\`). The nested beginning is unconnected; the end
//! drains to this stage’s primary output. Do not write an explicit connector.

use pipe_core::{StageCtx, StreamId};

use crate::registry::StageRegistry;
use crate::run::callpipe_subroutine;

#[derive(Clone, Copy)]
enum Mode {
    Preface,
    Append,
}

/// PREFACE — nested output, then copy the primary input (p.461).
pub fn preface(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_control(ctx, &args, Mode::Preface))
}

/// APPEND — copy the primary input, then nested output (p.263).
pub fn append(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_control(ctx, &args, Mode::Append))
}

fn verb(mode: Mode) -> &'static str {
    match mode {
        Mode::Preface => "PREFACE",
        Mode::Append => "APPEND",
    }
}

fn run_control(ctx: &mut StageCtx, args: &[String], mode: Mode) {
    let v = verb(mode);
    let spec = match nested_spec(args) {
        Ok(s) => s,
        Err(e) => {
            ctx.fail(format!("{v}: {e}"));
            return;
        }
    };

    if ctx.input_connected(StreamId::SECONDARY) {
        ctx.fail(format!("{v}: secondary input stream must not be connected"));
        return;
    }

    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    match mode {
        Mode::Preface => {
            if let Err(e) = callpipe_subroutine(ctx, &spec, &StageRegistry::default()) {
                ctx.fail(format!("{v}: {e}"));
                return;
            }
            let _ = copy_through(ctx);
        }
        Mode::Append => {
            if !copy_through(ctx) {
                return;
            }
            if let Err(e) = callpipe_subroutine(ctx, &spec, &StageRegistry::default()) {
                ctx.fail(format!("{v}: {e}"));
            }
        }
    }
}

/// Copy primary input to primary output. `false` if the output severs first
/// (APPEND must not then run the nested specification, p.263).
fn copy_through(ctx: &mut StageCtx) -> bool {
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    while let Ok(Some(r)) = ctx.read() {
        if ctx.write(r).is_err() {
            return false;
        }
    }
    true
}

fn nested_spec(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        Err("a pipeline specification is required".into())
    } else {
        Ok(args.join(" "))
    }
}
