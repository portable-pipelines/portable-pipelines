//! REXX stage bridge for Portable Pipelines.
//!
//! Provides IBM pipeline subcommands (`READTO`, `OUTPUT`, `CALLPIPE`, `ADDPIPE`,
//! `SHORT`, stream select) to REXX stage programs.
//!
//! **Runtime:** if a user-installed **Regina** shared library is found, `.rexx`
//! / `.rex` stages run through it (ADDRESS `PIPELINE`). Otherwise the embedded
//! subset interpreter is used.

mod cms_ops;
mod interp;
mod regina;
mod subcommands;
mod varpool;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pipe_core::{PipeError, PipeResult, Record, StageCtx};
use pipe_stages::StageRegistry;

pub use cms_ops::normalize_cms_rexx_operators;
pub use interp::run_rexx_source;
pub use regina::{discovery_hint, regina_api_for_stages, regina_available};
pub use subcommands::RexxPipeHost;

/// Which engine ran (or will run) a REXX stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RexxEngine {
    Regina,
    Subset,
}

/// Install the REXX stage hook into `pipe-stages` so `.rexx` / `.rex` verbs resolve.
pub fn install_hook() {
    let hook: pipe_stages::RexxHook =
        Rc::new(|ctx, verb, args| run_rexx_stage(ctx, verb, args).map_err(|e| e.to_string()));
    pipe_stages::set_rexx_hook(hook);

    let varpool_hook: pipe_stages::VarPoolHook = Rc::new(|op| match op {
        pipe_stages::VarPoolOp::Get(name) => {
            varpool::varpool_get(&name).map(pipe_stages::VarPoolResult::Value)
        }
        pipe_stages::VarPoolOp::Set(name, value) => {
            varpool::varpool_set(&name, &value).map(|_| pipe_stages::VarPoolResult::Success)
        }
        pipe_stages::VarPoolOp::List => {
            varpool::varpool_list().map(pipe_stages::VarPoolResult::List)
        }
    });
    pipe_stages::set_varpool_hook(varpool_hook);
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
    // A nested `.rexx` under an active RexxStart cannot call RexxStart again
    // (Regina 3.9.x hangs on ADDRESS PIPELINE re-entry); use the subset
    // interpreter with this stage's live StageCtx instead.
    if regina_available() && !regina::in_regina_run() {
        let mut incoming = Vec::new();
        loop {
            match ctx.read()? {
                Some(r) => incoming.push(r.to_string_lossy()),
                None => break,
            }
        }
        let mut host = RexxPipeHost::buffered(incoming);
        match regina::run_with_regina(&mut host, &path, stage_args) {
            Ok(exit_rc) => {
                for rec in host.into_output() {
                    ctx.write(Record::from(rec))?;
                }
                ctx.set_return_code(exit_rc);
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
    let result = interp::run_rexx_source(&host, &source, stage_args);
    host.borrow_mut().apply_stage_exit();
    result
}

/// Evaluate inline REXX via the subset interpreter (tests / helpers).
pub fn eval_rexx(ctx: &mut StageCtx, source: &str) -> PipeResult<()> {
    let host = RefCell::new(RexxPipeHost::new(ctx));
    let result = interp::run_rexx_source(&host, source, &[]);
    host.borrow_mut().apply_stage_exit();
    result
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
    pipe_stages::callpipe_from_spec(ctx, spec, &StageRegistry::default())
}

/// Buffered CALLPIPE for the Regina host (no live parent `StageCtx`).
///
/// Leading `*:` consumes `buf_in`; trailing `*:` appends nested records to
/// `buf_out`. Specs without `*:` (including labeled multi-pipeline LOOKUP)
/// run the same graph as the CLI. Nested `.rexx` stages go through
/// [`run_rexx_stage`] (subset while a Regina `RexxStart` is already active).
pub(crate) fn callpipe_buffered(
    buf_in: &mut VecDeque<String>,
    buf_out: &mut Vec<String>,
    spec: &str,
) -> PipeResult<()> {
    pipe_stages::run_callpipe_standalone(spec, buf_in, buf_out)
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

    fn run_rexx_file(script: &Path, args: &[&str], input: &[&str]) -> Vec<String> {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let script = script.to_path_buf();
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

    fn run_prefix(args: &[&str], input: &[&str]) -> Vec<String> {
        run_rexx_file(&prefix_script(), args, input)
    }

    fn pipe_require_regina() -> bool {
        matches!(std::env::var("PIPE_REQUIRE_REGINA").as_deref(), Ok("1"))
    }

    #[test]
    fn buffered_callpipe_feeds_and_drains() {
        let mut input: VecDeque<String> = vec!["keep".into(), "two".into()].into();
        let mut output = Vec::new();
        callpipe_buffered(&mut input, &mut output, "strliteral /z/ | *:").unwrap();
        assert_eq!(
            input.iter().cloned().collect::<Vec<_>>(),
            vec!["keep", "two"]
        );
        assert_eq!(output, vec!["z".to_string()]);

        output.clear();
        callpipe_buffered(&mut input, &mut output, "*: | change /two/ /TWO/ | *:").unwrap();
        assert!(input.is_empty());
        assert_eq!(output, vec!["keep".to_string(), "TWO".to_string()]);
    }

    #[test]
    fn buffered_callpipe_unknown_stage_reports_nested_origin() {
        let mut input = VecDeque::new();
        let mut output = Vec::new();
        let err = callpipe_buffered(&mut input, &mut output, "literal x | nosuch").unwrap_err();
        let s = err.to_string();
        assert!(s.contains("unknown stage: NOSUCH"), "{s}");
        assert!(
            s.contains("... Processing \"callpipe literal x | nosuch\""),
            "{s}"
        );
        assert!(s.contains("... Issued from stage 2 of pipeline 1"), "{s}");
        assert!(s.contains("... Running \"nosuch\""), "{s}");
    }

    #[test]
    fn buffered_callpipe_labeled_lookup_connects_secondary() {
        // Details on pipe 1, masters on pipe 2 via L:. The previous Regina
        // CALLPIPE runner kept only the first pipeline, so LOOKUP saw one
        // stream and failed. Unmatched details are dropped here because
        // there is no drain; success means the two streams were wired.
        let mut input = VecDeque::new();
        let mut output = Vec::new();
        callpipe_buffered(
            &mut input,
            &mut output,
            "(ENDCHAR ?) literal apricot| L: lookup | hole ? literal apple| L:",
        )
        .unwrap_or_else(|e| panic!("labeled LOOKUP CALLPIPE failed: {e}"));
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
        install_hook();
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
        nested_callpipe_rexx_continues_after();
        cms_not_equal_operators_run_under_regina();
    }

    fn cms_not_equal_operators_run_under_regina() {
        if !regina_available() {
            return;
        }
        let script = PathBuf::from("cms-pipelines-not-equal.rexx");
        std::fs::write(
            &script,
            "DO FOREVER\n\
             'READTO LINE'\n\
             IF RC = 12 THEN LEAVE\n\
             IF LINE /= 'skip' THEN 'OUTPUT' 'slash'\n\
             IF LINE ^= 'skip' THEN 'OUTPUT' 'hat'\n\
             END\n",
        )
        .unwrap();
        let out = run_rexx_file(&script, &[], &["keep", "skip"]);
        let _ = std::fs::remove_file(&script);
        assert_eq!(
            out,
            vec!["slash".to_string(), "hat".to_string()],
            "Regina should accept CMS /= and ^= after rewrite; skip must not match"
        );
    }

    fn nested_callpipe_rexx_continues_after() {
        // Outer REXX CALLPIPEs a subpipeline that contains another REXX
        // filter, then READTO/OUTPUT after CALLPIPE returns. A watchdog
        // aborts if the old non-reentrant RUN_LOCK/HOST mutexes deadlock.
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        // Pipeline scan treats a leading `/` as a dstring, so the nested
        // `.rexx` verb must be a relative path (not `/tmp/...` or `/workspace/...`).
        let inner = PathBuf::from("cms-pipelines-nested-inner.rexx");
        std::fs::copy(prefix_script(), &inner).unwrap();
        let inner_str = inner.to_string_lossy();
        let outer = PathBuf::from("cms-pipelines-nested-callpipe.rexx");
        std::fs::write(
            &outer,
            format!(
                "'READTO A'\n\
                 'OUTPUT' 'pre:' || A\n\
                 'CALLPIPE' 'strliteral /nest/ | {inner_str} x= | *:'\n\
                 'READTO B'\n\
                 'OUTPUT' 'mid:' || B\n\
                 'CALLPIPE' '*: | {inner_str} y= | *:'\n\
                 'OUTPUT' 'done'\n\
                 'READTO C'\n\
                 'OUTPUT' 'eof:' || RC\n"
            ),
        )
        .unwrap();

        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(20));
            if !flag.load(Ordering::SeqCst) {
                eprintln!("nested CALLPIPE+REXX hung (RUN_LOCK/HOST deadlock?)");
                std::process::abort();
            }
        });

        let out = std::panic::catch_unwind(|| run_rexx_file(&outer, &[], &["one", "two", "three"]));
        done.store(true, Ordering::SeqCst);
        let _ = std::fs::remove_file(&outer);
        let _ = std::fs::remove_file(&inner);
        let out = out.expect("nested CALLPIPE+REXX panicked");
        assert_eq!(
            out,
            vec![
                "pre:one".to_string(),
                "x=nest".to_string(),
                "mid:two".to_string(),
                "y=three".to_string(),
                "done".to_string(),
                "eof:12".to_string(),
            ]
        );
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
            ["other=  spaced  ", "other=  lead", "other=trail  "]
        );
    }
}
