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

    let stage_args: &[String] = if verb.eq_ignore_ascii_case("REXX") {
        args.get(1..).unwrap_or(&[])
    } else {
        args
    };

    // Try Regina first (user-installed). I/O is buffered so RexxStart never
    // suspends a dispatcher coroutine (Regina is not safe to interleave).
    if regina_available() {
        let mut incoming = Vec::new();
        loop {
            match ctx.read()? {
                Some(r) => incoming.push(r.to_string_lossy()),
                None => break,
            }
        }
        let mut host = RexxPipeHost::buffered(incoming);
        match regina::run_with_regina(&mut host, &path, stage_args) {
            Ok(()) => {
                for rec in host.into_output() {
                    ctx.write(pipe_core::Record::from(rec))?;
                }
                return Ok(());
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("not found") || msg.contains("unavailable") {
                    // Fall through to subset. Input was already consumed — cannot
                    // recover; surface a clear error instead of running empty.
                    return Err(PipeError::stage(format!(
                        "Regina unavailable after consuming stage input: {msg}"
                    )));
                }
                return Err(e);
            }
        }
    }

    let source = std::fs::read_to_string(&path)
        .map_err(|e| PipeError::io(format!("reading {}: {e}", path.display())))?;
    let host = RefCell::new(RexxPipeHost::new(ctx));
    interp::run_rexx_source(&host, &source, stage_args)
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
    use std::cell::RefCell;

    use pipe_core::{PipelineBuilder, Record};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn prefix_script() -> PathBuf {
        repo_root().join("examples/prefix.rexx")
    }

    fn run_prefix(args: &[&str], input: &[&str]) -> Vec<String> {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let script = prefix_script();
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        let incoming: Vec<String> = input.iter().map(|s| (*s).to_string()).collect();

        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(move |ctx| {
                for rec in incoming {
                    ctx.write(Record::from(rec)).unwrap();
                }
            }),
        );
        let rexx = b.add_stage(
            "rexx",
            Box::new(move |ctx| {
                run_rexx_stage(ctx, script.to_str().unwrap(), &args).unwrap();
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(rec)) = ctx.read() {
                    out.borrow_mut().push(rec.to_string_lossy());
                }
            }),
        );
        b.connect_primary(src, rexx);
        b.connect_primary(rexx, sink);
        b.run().unwrap();
        let result = collected.borrow().clone();
        result
    }

    fn pipe_require_regina() -> bool {
        matches!(std::env::var("PIPE_REQUIRE_REGINA").as_deref(), Ok("1"))
    }

    #[test]
    fn preferred_engine_is_defined() {
        // Without Regina installed this is Subset; with Regina it is Regina.
        let e = preferred_engine();
        assert!(matches!(e, RexxEngine::Subset | RexxEngine::Regina));
        let _ = discovery_hint();
    }

    #[test]
    fn pipe_require_regina_loads_real_regina() {
        // Default: no-op so contributors without Regina still pass.
        // Cloud agents / CI: export PIPE_REQUIRE_REGINA=1 so a missing
        // install cannot silently fall back to the subset interpreter.
        if !pipe_require_regina() {
            return;
        }
        assert!(
            regina_available(),
            "PIPE_REQUIRE_REGINA=1 but Regina was not discovered/loaded ({})",
            discovery_hint()
        );
        assert_eq!(
            preferred_engine(),
            RexxEngine::Regina,
            "PIPE_REQUIRE_REGINA=1 requires the Regina engine"
        );
    }

    #[test]
    fn prefix_rexx_keeps_leading_and_trailing_blanks() {
        // All RexxStart cases live in one test function. Linux Regina 3.9.5
        // hangs on a later RexxStart once a previous `#[test]` that called
        // it has returned (same-function repeats are fine).
        assert_eq!(
            run_prefix(&["other="], &["  spaced  ", "  lead", "trail  ", ""]),
            vec![
                "other=  spaced  ".to_string(),
                "other=  lead".to_string(),
                "other=trail  ".to_string(),
                "other=".to_string(),
            ]
        );

        if regina_available() {
            let again = run_prefix(&["x="], &["aa"]);
            assert_eq!(again, vec!["x=aa".to_string()]);
        }

        disk_crlf_through_prefix_keeps_blanks();
    }

    fn disk_crlf_through_prefix_keeps_blanks() {
        // DISK strips CR-before-LF; the REXX filter must still keep trailing blanks.
        // Build the graph directly so a Windows temp path is not re-parsed as a spec.
        let path = std::env::temp_dir().join("cms-pipelines-rexx-blanks-crlf.txt");
        std::fs::write(&path, b"  spaced  \r\n  lead\r\ntrail  \r\n").unwrap();

        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let script = prefix_script();
        let args = vec!["other=".to_string()];
        let path_str = path.to_string_lossy().into_owned();
        let reg = pipe_stages::StageRegistry::default();

        let mut b = PipelineBuilder::new();
        let disk = b.add_stage("DISK", reg.create("DISK", vec![path_str]).unwrap());
        let rexx = b.add_stage(
            "rexx",
            Box::new(move |ctx| {
                run_rexx_stage(ctx, script.to_str().unwrap(), &args).unwrap();
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(rec)) = ctx.read() {
                    out.borrow_mut().push(rec.to_string_lossy());
                }
            }),
        );
        b.connect_primary(disk, rexx);
        b.connect_primary(rexx, sink);
        b.run().unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            collected.borrow().as_slice(),
            [
                "other=  spaced  ",
                "other=  lead",
                "other=trail  "
            ]
        );
    }
}
