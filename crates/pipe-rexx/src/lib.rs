//! REXX stage bridge for Portable Pipelines.
//!
//! Provides IBM pipeline subcommands (`READTO`, `PEEKTO`, `OUTPUT`, `CALLPIPE`,
//! and the other Chapter 21 verbs) to REXX stage programs.
//!
//! **Runtime:** if a user-installed **Regina** shared library is found, `.rexx`
//! / `.rex` stages run through it (ADDRESS `PIPELINE`). Otherwise the embedded
//! subset interpreter is used.

mod cms_ops;
mod interp;
mod regina;
mod subcommands;
mod varpool;

#[cfg(test)]
mod cmd_tests;

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
        while let Some(r) = ctx.read()? {
            incoming.push(r.to_string_lossy());
        }
        let mut host = RexxPipeHost::buffered_from(ctx, incoming);
        match regina::run_with_regina(&mut host, &path, stage_args) {
            Ok(exit_rc) => {
                let stage_rc = if exit_rc != 0 {
                    exit_rc
                } else {
                    host.stage_rc.unwrap_or(exit_rc)
                };
                let input_severed = host.input_severed();
                let output_severed = host.output_severed();
                let sel_in = host.selected_in_id();
                let sel_out = host.selected_out_id();
                for rec in host.into_output() {
                    ctx.write(Record::from(rec))?;
                }
                if input_severed {
                    ctx.select_input(sel_in);
                    ctx.short_input();
                }
                if output_severed {
                    // Wait until written records are consumed, then sever.
                    // short_output would drop the in-flight slot.
                    ctx.select_output(sel_out);
                    ctx.sever_output();
                }
                ctx.set_return_code(stage_rc);
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
        pipeline_commands_under_regina();
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

    fn write_script(label: &str, body: &str) -> PathBuf {
        let p = PathBuf::from(format!(
            "cms-pipelines-cmd-{}-{label}.rexx",
            std::process::id()
        ));
        std::fs::write(&p, body).unwrap();
        p
    }

    fn run_rexx_rc(script: &Path, args: &[&str], input: &[&str]) -> (Vec<String>, i32) {
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
                    let _ = ctx.write(Record::from(rec));
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
        let rc = b.run_with_return_code().unwrap();
        let records = collected.borrow().clone();
        (records, rc)
    }

    fn pipeline_commands_under_regina() {
        if !regina_available() {
            return;
        }
        regina_peekto();
        regina_readto();
        regina_addstream();
        regina_begoutput();
        regina_eofreport();
        regina_getrange();
        regina_issuemsg();
        regina_maxstream();
        regina_message();
        regina_nocommit();
        regina_resolve();
        regina_scanrange();
        regina_scanstring();
        regina_setrc_and_exit();
        regina_sever();
        regina_stagenum();
        regina_streamnum();
        regina_streamstate();
        regina_suspend();
        regina_select_and_commit();
        regina_rexx_command();
    }

    fn run_body(label: &str, body: &str, input: &[&str]) -> Vec<String> {
        let p = write_script(label, body);
        let out = run_rexx_file(&p, &[], input);
        let _ = std::fs::remove_file(&p);
        out
    }

    fn run_body_rc(label: &str, body: &str, input: &[&str]) -> (Vec<String>, i32) {
        let p = write_script(label, body);
        let result = run_rexx_rc(&p, &[], input);
        let _ = std::fs::remove_file(&p);
        result
    }

    fn joined_contains(out: &[String], needle: &str) {
        let joined = out.join("\n");
        assert!(joined.contains(needle), "missing {needle} in {joined}");
    }

    fn regina_peekto() {
        let out = run_body(
            "peekto-star",
            "'PEEKTO LINE'\n\
             'OUTPUT' 'peek:' || LINE || ':' || RC\n\
             'CALLPIPE' '*: | xlate 1-* upper | *:'\n",
            &["hello"],
        );
        assert_eq!(out, vec!["peek:hello:0".to_string(), "HELLO".to_string()]);

        let out = run_body(
            "peekto-repeat",
            "'PEEKTO A'\n\
             'OUTPUT' 'a=' || A\n\
             'PEEKTO B'\n\
             'OUTPUT' 'b=' || B || ':' || RC\n\
             'READTO C'\n\
             'OUTPUT' 'c=' || C\n\
             'PEEKTO D'\n\
             'OUTPUT' 'drc=' || RC\n",
            &["one"],
        );
        assert_eq!(
            out,
            vec![
                "a=one".to_string(),
                "b=one:0".to_string(),
                "c=one".to_string(),
                "drc=12".to_string(),
            ]
        );

        let out = run_body("peekto-eof", "'PEEKTO LINE'\n'OUTPUT' 'rc=' || RC\n", &[]);
        assert_eq!(out, vec!["rc=12".to_string()]);

        let out = run_body(
            "peekto-multi",
            "'PEEKTO A'\n'OUTPUT' A\n'READTO A'\n\
             'PEEKTO B'\n'OUTPUT' B\n'READTO B'\n\
             'PEEKTO C'\n'OUTPUT' C\n",
            &["x", "y"],
        );
        assert_eq!(out, vec!["x".to_string(), "y".to_string(), "y".to_string()]);

        let out = run_body(
            "peekto-extra",
            "'PEEKTO LINE EXTRA'\n'OUTPUT' 'rc=' || RC\n",
            &["x"],
        );
        assert_eq!(out, vec!["rc=-112".to_string()]);
    }

    fn regina_readto() {
        let out = run_body(
            "readto-star",
            "'READTO LINE'\n'CALLPIPE' '*: | xlate 1-* upper | *:'\n",
            &["gone", "kept"],
        );
        assert_eq!(out, vec!["KEPT".to_string()]);

        let out = run_body(
            "readto-eof",
            "'READTO X'\n'READTO X'\n'OUTPUT' 'rc=' || RC\n",
            &["one"],
        );
        assert_eq!(out, vec!["rc=12".to_string()]);

        let out = run_body(
            "readto-extra",
            "'READTO LINE EXTRA'\n'OUTPUT' 'rc=' || RC\n",
            &["x"],
        );
        assert_eq!(out, vec!["rc=-112".to_string()]);
    }

    fn regina_addstream() {
        let out = run_body(
            "addstream",
            "'ADDSTREAM'\n'OUTPUT' 'n=' || RC\n\
             'ADDSTREAM BOTH'\n'OUTPUT' 'both=' || RC\n\
             'ADDSTREAM foo'\n'OUTPUT' 'bad=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "n=1");
        joined_contains(&out, "both=-164");
        joined_contains(&out, "bad=-178");
    }

    fn regina_begoutput() {
        let out = run_body(
            "begoutput",
            "'BEGOUTPUT'\n'OUTPUT' 'ok=' || RC\n\
             'BEGOUTPUT extra'\n'OUTPUT' 'extra=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "ok=0");
        joined_contains(&out, "extra=-112");
    }

    fn regina_eofreport() {
        let (out, _) = run_body_rc(
            "eofreport-sever",
            "'EOFREPORT CURRENT'\n\
             'SEVER INPUT'\n\
             'READTO X'\n\
             'OUTPUT' 'rc=' || RC\n",
            &["x"],
        );
        assert_eq!(out, vec!["rc=8".to_string()]);

        let out = run_body(
            "eofreport-bad",
            "'EOFREPORT FOO'\n'OUTPUT' 'rc=' || RC\n",
            &["x"],
        );
        assert_eq!(out, vec!["rc=-4".to_string()]);
    }

    fn regina_getrange() {
        let out = run_body(
            "getrange",
            "'SCANRANGE RNG REST 2-4 leftover'\n\
             'GETRANGE RNG PIECE abcdef'\n\
             'OUTPUT' 'piece=' || PIECE || '/' || RC\n\
             'GETRANGE'\n\
             'OUTPUT' 'miss=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "piece=bcd/0");
        joined_contains(&out, "miss=-42");
    }

    fn regina_issuemsg() {
        let out = run_body(
            "issuemsg",
            "'ISSUEMSG 1 PORTABLE token'\n'OUTPUT' 'ok=' || RC\n\
             'ISSUEMSG'\n'OUTPUT' 'miss=' || RC\n\
             'ISSUEMSG xyz'\n'OUTPUT' 'bad=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "ok=0");
        joined_contains(&out, "miss=-42");
        joined_contains(&out, "bad=-42");
    }

    fn regina_maxstream() {
        let out = run_body(
            "maxstream",
            "'MAXSTREAM OUTPUT'\n'OUTPUT' 'out=' || RC\n\
             'MAXSTREAM'\n'OUTPUT' 'miss=' || RC\n\
             'MAXSTREAM BOTH'\n'OUTPUT' 'both=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "out=0");
        joined_contains(&out, "miss=-163");
        joined_contains(&out, "both=-164");
    }

    fn regina_message() {
        let out = run_body(
            "message",
            "'MESSAGE pipeline-cmd-test'\n'OUTPUT' 'rc=' || RC\n",
            &["x"],
        );
        assert_eq!(out, vec!["rc=0".to_string()]);
    }

    fn regina_nocommit() {
        let out = run_body(
            "nocommit",
            "'NOCOMMIT'\n'OUTPUT' 'ok=' || RC\n\
             'NOCOMMIT extra'\n'OUTPUT' 'extra=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "ok=0");
        joined_contains(&out, "extra=-112");
    }

    fn regina_resolve() {
        let out = run_body(
            "resolve",
            "'RESOLVE COPY'\n'OUTPUT' 'copy=' || RC\n\
             'RESOLVE JEREMY'\n'OUTPUT' 'miss=' || RC\n\
             'RESOLVE'\n'OUTPUT' 'arg=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "copy=1");
        joined_contains(&out, "miss=0");
        joined_contains(&out, "arg=-42");
    }

    fn regina_scanrange() {
        let out = run_body(
            "scanrange",
            "'SCANRANGE RNG REST 2-4 leftover'\n\
             'OUTPUT' 'scan=' || RNG || '/' || REST || '/' || RC\n\
             'SCANRANGE'\n'OUTPUT' 'miss=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "scan=2-4/leftover/0");
        joined_contains(&out, "miss=-42");
    }

    fn regina_scanstring() {
        let out = run_body(
            "scanstring",
            "'SCANSTRING S R /hi/ there'\n\
             'OUTPUT' 'scan=' || S || '/' || R || '/' || RC\n\
             'SCANSTRING S R undelimited'\n'OUTPUT' 'bad=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "scan=hi/there/0");
        joined_contains(&out, "bad=8");
    }

    fn regina_setrc_and_exit() {
        let (out, rc) = run_body_rc("setrc", "'SETRC 7'\n'OUTPUT' 'ok'\n", &[]);
        assert_eq!(out, vec!["ok".to_string()]);
        assert_eq!(rc, 7, "SETRC 7 should become the stage return code");

        let (out, rc) = run_body_rc("exit", "EXIT 5\n", &["x"]);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(rc, 5, "EXIT 5 should become the stage return code");

        let (out, rc) = run_body_rc("setrc-exit", "'SETRC 7'\nEXIT 3\n", &["x"]);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(rc, 3, "nonzero EXIT overrides SETRC");

        let (out, rc) = run_body_rc("setrc-exit0", "'SETRC 7'\nEXIT 0\n", &["x"]);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(rc, 7, "EXIT 0 keeps SETRC (same as falling off the end)");

        let out = run_body("setrc-bad", "'SETRC foo'\n'OUTPUT' 'rc=' || RC\n", &["x"]);
        assert_eq!(out, vec!["rc=-2147483648".to_string()]);
    }

    fn regina_sever() {
        let (out, _) = run_body_rc(
            "sever",
            "'OUTPUT' 'before'\n'SEVER OUTPUT'\n'OUTPUT' 'after'\n",
            &["x"],
        );
        assert_eq!(out, vec!["before".to_string()]);

        let out = run_body(
            "sever-err",
            "'SEVER'\n'OUTPUT' 'miss=' || RC\n\
             'SEVER BOTH'\n'OUTPUT' 'both=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "miss=-163");
        joined_contains(&out, "both=-164");
    }

    fn regina_stagenum() {
        let out = run_body(
            "stagenum",
            "'STAGENUM'\n'OUTPUT' 'n=' || RC\n\
             'STAGENUM extra'\n'OUTPUT' 'extra=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "n=2");
        joined_contains(&out, "extra=-112");
    }

    fn regina_streamnum() {
        let out = run_body(
            "streamnum",
            "'STREAMNUM INPUT'\n'OUTPUT' 'in=' || RC\n\
             'STREAMNUM INPUT 9'\n'OUTPUT' 'miss=' || RC\n\
             'STREAMNUM'\n'OUTPUT' 'arg=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "in=0");
        joined_contains(&out, "miss=-102");
        joined_contains(&out, "arg=-163");
    }

    fn regina_streamstate() {
        let out = run_body(
            "streamstate",
            "'STREAMSTATE OUTPUT'\n'OUTPUT' 'out=' || RC\n\
             'STREAMSTATE OUTPUT 1'\n'OUTPUT' 'undef=' || RC\n\
             'STREAMSTATE ALL ST'\n'OUTPUT' 'stem=' || ST.0\n",
            &["x"],
        );
        joined_contains(&out, "out=0");
        joined_contains(&out, "undef=-4");
        let joined = out.join("\n");
        assert!(
            joined.contains("stem=")
                && joined
                    .lines()
                    .any(|l| l.starts_with("stem=") && l[5..].parse::<i32>().unwrap_or(0) >= 1),
            "{joined}"
        );
    }

    fn regina_suspend() {
        let out = run_body(
            "suspend",
            "'SUSPEND'\n'OUTPUT' 'ok=' || RC\n\
             'SUSPEND extra'\n'OUTPUT' 'extra=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "ok=0");
        joined_contains(&out, "extra=-112");
    }

    fn regina_select_and_commit() {
        let out = run_body(
            "select",
            "'SELECT BOTH 0'\n'OUTPUT' 'both=' || RC\n\
             'SELECT ANYINPUT'\n'OUTPUT' 'any=' || RC\n\
             'SELECT 99'\n'OUTPUT' 'miss=' || RC\n\
             'COMMIT 0'\n'OUTPUT' 'commit=' || RC\n\
             'COMMIT'\n'OUTPUT' 'cmiss=' || RC\n",
            &["x"],
        );
        joined_contains(&out, "both=0");
        joined_contains(&out, "any=0");
        joined_contains(&out, "miss=4");
        joined_contains(&out, "commit=0");
        joined_contains(&out, "cmiss=-2147483648");
    }

    fn regina_rexx_command() {
        let inner = write_script(
            "rexx-inner",
            "DO FOREVER\n  'READTO L'\n  IF RC = 12 THEN LEAVE\n  'OUTPUT' 'in:' || L\nEND\n",
        );
        let inner_str = inner.to_string_lossy().into_owned();
        let out = run_body("rexx-cmd", &format!("'REXX {inner_str}'\n"), &["a", "b"]);
        let _ = std::fs::remove_file(&inner);
        assert_eq!(out, vec!["in:a".to_string(), "in:b".to_string()]);

        let out = run_body("rexx-miss", "'REXX'\n'OUTPUT' 'rc=' || RC\n", &["x"]);
        assert_eq!(out, vec!["rc=-42".to_string()]);
    }

    fn run_subset(source: &str, input: &[&str]) -> (Vec<String>, i32) {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let incoming: Vec<String> = input.iter().map(|s| (*s).to_string()).collect();
        let source = source.to_string();
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(move |ctx| {
                for rec in incoming {
                    let _ = ctx.write(Record::from(rec));
                }
            }),
        );
        let rexx = b.add_stage(
            "rexx",
            Box::new(move |ctx| {
                eval_rexx(ctx, &source).unwrap();
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
        let rc = b.run_with_return_code().unwrap();
        let records = collected.borrow().clone();
        (records, rc)
    }

    #[test]
    fn peekto_live_then_callpipe_star() {
        install_hook();
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("hello"));
            }),
        );
        let rexx = b.add_stage(
            "rexx",
            Box::new(|ctx| {
                eval_rexx(
                    ctx,
                    "'PEEKTO LINE'\n\
                     'OUTPUT' 'peek:' || LINE\n\
                     'CALLPIPE' '*: | xlate 1-* upper | *:'\n",
                )
                .unwrap();
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
        assert_eq!(collected.borrow().as_slice(), ["peek:hello", "HELLO"]);
    }

    #[test]
    fn subset_peekto_then_readto_and_eof() {
        let (out, _) = run_subset(
            "'PEEKTO A'\n'OUTPUT' A\n\
             'PEEKTO B'\n'OUTPUT' B\n\
             'READTO C'\n\
             'PEEKTO D'\n'OUTPUT' 'd=' || D || ':' || RC\n",
            &["one", "two"],
        );
        assert_eq!(
            out,
            vec!["one".to_string(), "one".to_string(), "d=two:0".to_string()]
        );

        let (out, _) = run_subset("'PEEKTO X'\n'OUTPUT' 'rc=' || RC\n", &[]);
        assert_eq!(out, vec!["rc=12".to_string()]);
    }

    #[test]
    fn subset_setrc_versus_exit() {
        let (out, rc) = run_subset("'SETRC 7'\n'OUTPUT' 'ok'\n", &[]);
        assert_eq!(out, vec!["ok".to_string()]);
        assert_eq!(rc, 7);

        let (out, rc) = run_subset("EXIT 5\n", &["x"]);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(rc, 5);

        let (_, rc) = run_subset("'SETRC 7'\nEXIT 3\n", &["x"]);
        assert_eq!(rc, 3);

        let (_, rc) = run_subset("'SETRC 7'\nEXIT 0\n", &["x"]);
        assert_eq!(
            rc, 7,
            "EXIT 0 should keep SETRC, matching the Regina runner"
        );
    }

    #[test]
    fn subset_sever_and_eofreport() {
        let (out, _) = run_subset(
            "'OUTPUT' 'before'\n'SEVER OUTPUT'\n'OUTPUT' 'after'\n",
            &["x"],
        );
        assert_eq!(out, vec!["before".to_string()]);

        let (out, _) = run_subset(
            "'EOFREPORT CURRENT'\n'SEVER INPUT'\n'READTO X'\n'OUTPUT' 'rc=' || RC\n",
            &["x"],
        );
        assert_eq!(out, vec!["rc=8".to_string()]);
    }

    #[test]
    fn subset_stream_and_resolve_commands() {
        let (out, _) = run_subset(
            "'STAGENUM'\n'OUTPUT' 'stage=' || RC\n\
             'STREAMNUM INPUT'\n'OUTPUT' 'snum=' || RC\n\
             'STREAMSTATE OUTPUT'\n'OUTPUT' 'sstate=' || RC\n\
             'MAXSTREAM OUTPUT'\n'OUTPUT' 'max=' || RC\n\
             'RESOLVE COPY'\n'OUTPUT' 'copy=' || RC\n\
             'RESOLVE JEREMY'\n'OUTPUT' 'miss=' || RC\n\
             'ADDSTREAM'\n'OUTPUT' 'add=' || RC\n\
             'SELECT BOTH 0'\n'OUTPUT' 'sel=' || RC\n\
             'COMMIT 0'\n'OUTPUT' 'c=' || RC\n\
             'SUSPEND'\n'OUTPUT' 'sus=' || RC\n\
             'NOCOMMIT'\n'OUTPUT' 'nc=' || RC\n\
             'BEGOUTPUT'\n'OUTPUT' 'bo=' || RC\n",
            &["x"],
        );
        let joined = out.join("\n");
        for needle in [
            "stage=2", "snum=0", "sstate=0", "max=0", "copy=1", "miss=0", "add=1", "sel=0", "c=0",
            "sus=0", "nc=0", "bo=0",
        ] {
            assert!(joined.contains(needle), "missing {needle} in {joined}");
        }
    }

    #[test]
    fn subset_scan_getrange_message() {
        let (out, _) = run_subset(
            "'SCANRANGE RNG REST 2-4 leftover'\n\
             'GETRANGE RNG PIECE abcdef'\n\
             'OUTPUT' 'g=' || PIECE || '/' || RC\n\
             'SCANSTRING S R /hi/ there'\n\
             'OUTPUT' 's=' || S || '/' || R\n\
             'MESSAGE hi'\n'OUTPUT' 'm=' || RC\n\
             'ISSUEMSG 1 PORTABLE'\n'OUTPUT' 'i=' || RC\n",
            &["x"],
        );
        let joined = out.join("\n");
        assert!(joined.contains("g=bcd/0"), "{joined}");
        assert!(joined.contains("s=hi/there"), "{joined}");
        assert!(joined.contains("m=0"), "{joined}");
        assert!(joined.contains("i=0"), "{joined}");
    }

    #[test]
    fn subset_wrong_operands_are_return_codes() {
        let (out, _) = run_subset(
            "'PEEKTO A B'\n'OUTPUT' 'peek=' || RC\n\
             'SEVER'\n'OUTPUT' 'sever=' || RC\n\
             'STAGENUM extra'\n'OUTPUT' 'stage=' || RC\n\
             'EOFREPORT FOO'\n'OUTPUT' 'eof=' || RC\n",
            &["x"],
        );
        let joined = out.join("\n");
        assert!(joined.contains("peek=-112"), "{joined}");
        assert!(joined.contains("sever=-163"), "{joined}");
        assert!(joined.contains("stage=-112"), "{joined}");
        assert!(joined.contains("eof=-4"), "{joined}");
    }
}
