//! REXX stage bridge for CMS Pipelines.
//!
//! Provides IBM pipeline subcommands (`READTO`, `OUTPUT`, `CALLPIPE`, `ADDPIPE`,
//! `SHORT`, stream select) to REXX stage programs.
//!
//! **Runtime:** if a user-installed **Regina** shared library is found, `.rexx`
//! / `.rex` stages run through it (ADDRESS `PIPELINE`). Otherwise the embedded
//! subset interpreter is used.

mod interp;
mod regina;
mod subcommands;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pipe_core::{PipeError, PipeResult, StageCtx};
use pipe_parser::parse_pipeline;

pub use interp::run_rexx_source;
pub use regina::{discovery_hint, regina_available};
pub use subcommands::RexxPipeHost;

/// Which engine ran (or will run) a REXX stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RexxEngine {
    Regina,
    Subset,
}

/// Install the REXX stage hook into `pipe-stages` so `.rexx` / `.rex` verbs resolve.
pub fn install_hook() {
    let hook: pipe_stages::RexxHook = Rc::new(|ctx, verb, args| {
        run_rexx_stage(ctx, verb, args).map_err(|e| e.to_string())
    });
    pipe_stages::set_rexx_hook(hook);
}

/// Prefer Regina when loadable; otherwise the subset interpreter.
pub fn preferred_engine() -> RexxEngine {
    if regina_available() {
        RexxEngine::Regina
    } else {
        RexxEngine::Subset
    }
}

/// Run a REXX stage: `verb` is a path (`.rexx` / `.rex`) or `REXX` with path in args[0].
pub fn run_rexx_stage(ctx: &mut StageCtx, verb: &str, args: &[String]) -> PipeResult<()> {
    let path = if verb.eq_ignore_ascii_case("REXX") {
        args.first()
            .map(String::as_str)
            .ok_or_else(|| PipeError::stage("REXX requires a script path"))?
    } else {
        verb
    };
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(PipeError::io(format!(
            "REXX script not found: {}",
            path.display()
        )));
    }

    let mut host = RexxPipeHost::new(ctx);

    // Try Regina first (user-installed). Fall back to subset on any load/run failure
    // that indicates Regina isn't usable — except for script logic errors from Regina.
    if regina_available() {
        match regina::run_with_regina(&mut host, &path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // If Regina loaded but the script failed, surface that error.
                // Only fall back when Regina could not be used at all.
                let msg = e.to_string();
                if msg.contains("not found") || msg.contains("unavailable") {
                    // continue to subset
                } else {
                    return Err(e);
                }
            }
        }
    }

    let source = std::fs::read_to_string(&path)
        .map_err(|e| PipeError::io(format!("reading {}: {e}", path.display())))?;
    let host = RefCell::new(host);
    interp::run_rexx_source(&host, &source, args)
}

/// Evaluate inline REXX via the subset interpreter (tests / helpers).
pub fn eval_rexx(ctx: &mut StageCtx, source: &str) -> PipeResult<()> {
    let host = RefCell::new(RexxPipeHost::new(ctx));
    interp::run_rexx_source(&host, source, &[])
}

/// Resolve a script path relative to CWD.
pub fn find_script(name: &str) -> Option<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_engine_is_defined() {
        // Without Regina installed this is Subset; with Regina it is Regina.
        let e = preferred_engine();
        assert!(matches!(e, RexxEngine::Subset | RexxEngine::Regina));
        let _ = discovery_hint();
    }
}
