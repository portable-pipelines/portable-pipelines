//! REXX stage bridge for CMS Pipelines.
//!
//! Provides pipeline subcommands (`READTO`, `WRITETO`, `CALLPIPE`, `ADDPIPE`,
//! `SHORT`, stream select) to REXX stage programs.
//!
//! **Runtime:** an embedded Regina-compatible subset interpreter ships by
//! default so the project builds and tests without an external Regina
//! installation. The command surface matches what a Regina FFI bridge would
//! expose; see `docs/architecture.md` for plugging in real Regina.

mod interp;
mod subcommands;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use pipe_core::{PipeError, PipeResult, StageCtx};
use pipe_parser::parse_pipeline;

pub use interp::run_rexx_source;
pub use subcommands::RexxPipeHost;

/// Install the REXX stage hook into `pipe-stages` so `.rex` verbs resolve.
pub fn install_hook() {
    let hook: pipe_stages::RexxHook = Rc::new(|ctx, verb, args| {
        run_rexx_stage(ctx, verb, args).map_err(|e| e.to_string())
    });
    pipe_stages::set_rexx_hook(hook);
}

/// Run a REXX stage: `verb` is a path (`.rex`) or `REXX` with path in args[0].
pub fn run_rexx_stage(ctx: &mut StageCtx, verb: &str, args: &[String]) -> PipeResult<()> {
    let path = if verb.eq_ignore_ascii_case("REXX") {
        args.first()
            .map(String::as_str)
            .ok_or_else(|| PipeError::stage("REXX requires a script path"))?
    } else {
        verb
    };
    let source = std::fs::read_to_string(path)
        .map_err(|e| PipeError::io(format!("reading {path}: {e}")))?;
    let host = RexxPipeHost::new(ctx);
    // Host borrows ctx for the duration of the interpreter run.
    run_with_host(host, &source, args)
}

fn run_with_host(host: RexxPipeHost<'_>, source: &str, args: &[String]) -> PipeResult<()> {
    let host = RefCell::new(host);
    interp::run_rexx_source(&host, source, args)
}

/// Evaluate inline REXX (for tests / CALLPIPE helpers).
pub fn eval_rexx(ctx: &mut StageCtx, source: &str) -> PipeResult<()> {
    let host = RefCell::new(RexxPipeHost::new(ctx));
    interp::run_rexx_source(&host, source, &[])
}

/// Resolve a script path relative to CWD or given search dirs.
pub fn find_script(name: &str) -> Option<std::path::PathBuf> {
    let p = Path::new(name);
    if p.exists() {
        return Some(p.to_path_buf());
    }
    None
}

/// Helper used by REXX `CALLPIPE` to run a nested specification.
pub fn callpipe_spec(ctx: &mut StageCtx, spec: &str) -> PipeResult<()> {
    let _ = parse_pipeline(spec).map_err(|e| PipeError::parse(e.to_string()))?;
    pipe_stages::callpipe_from_spec(ctx, spec, &pipe_stages::StageRegistry::default())
}
