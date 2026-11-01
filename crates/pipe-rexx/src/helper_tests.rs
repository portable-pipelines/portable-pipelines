//! Out-of-process Regina helper: streaming, concurrency, RC, SHORT, crash.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pipe_core::{PipelineBuilder, Record};
use pipe_stages::run_spec_with_return_code;

use super::*;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn pipe_require_regina() -> bool {
    matches!(std::env::var("PIPE_REQUIRE_REGINA").as_deref(), Ok("1"))
}

fn skip_without_regina() -> bool {
    if regina_available() {
        return false;
    }
    if pipe_require_regina() {
        panic!(
            "PIPE_REQUIRE_REGINA=1 but Regina was not discovered/loaded ({})",
            discovery_hint()
        );
    }
    true
}

struct InprocOff {
    prev: Option<std::ffi::OsString>,
}

impl Drop for InprocOff {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var("PIPE_REGINA_INPROC", v),
            None => std::env::remove_var("PIPE_REGINA_INPROC"),
        }
    }
}

fn force_oop() -> InprocOff {
    let prev = std::env::var_os("PIPE_REGINA_INPROC");
    std::env::remove_var("PIPE_REGINA_INPROC");
    InprocOff { prev }
}

fn write_script(label: &str, body: &str) -> PathBuf {
    let p = PathBuf::from(format!(
        "cms-pipelines-helper-{}-{label}.rexx",
        std::process::id()
    ));
    std::fs::write(&p, body).unwrap();
    p
}

const COPY_UNTIL_EITHER_END: &str = "DO FOREVER\n\
     'READTO REC'\n\
     IF RC = 12 THEN LEAVE\n\
     'OUTPUT' REC\n\
     IF RC = 12 THEN DO\n\
       'SEVER INPUT'\n\
       LEAVE\n\
     END\n\
     END\n";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn with_repo_cwd<T>(f: impl FnOnce() -> T) -> T {
    let prev = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(repo_root()).expect("chdir repo root");
    let result = f();
    let _ = std::env::set_current_dir(prev);
    result
}

#[test]
fn helper_streaming_take_stops_infinite_upstream() {
    let _lock = env_lock();
    if skip_without_regina() {
        return;
    }
    let _oop = force_oop();
    install_hook();

    let script = write_script("stream-copy", COPY_UNTIL_EITHER_END);
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let script_run = script.clone();
    let take = pipe_stages::StageRegistry::default()
        .create("TAKE", vec!["3".into()])
        .expect("TAKE");

    let done = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&done);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        if !flag.load(Ordering::SeqCst) {
            eprintln!("helper streaming test hung (Regina still draining?)");
            std::process::abort();
        }
    });

    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            let mut n = 0u64;
            loop {
                if ctx.write(Record::from(n.to_string())).is_err() {
                    break;
                }
                n += 1;
            }
        }),
    );
    let rexx = b.add_stage(
        "rexx",
        Box::new(move |ctx| {
            if let Err(e) = run_rexx_stage(ctx, script_run.to_str().unwrap(), &[]) {
                ctx.fail(format!("REXX stage: {e}"));
            }
        }),
    );
    let mid = b.add_stage("TAKE", take);
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx| {
            while let Ok(Some(rec)) = ctx.read() {
                out.borrow_mut().push(rec.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, rexx);
    b.connect_primary(rexx, mid);
    b.connect_primary(mid, sink);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| b.run()));
    done.store(true, Ordering::SeqCst);
    let _ = std::fs::remove_file(&script);
    result.expect("streaming test panicked").expect("pipeline");
    assert_eq!(
        collected.borrow().as_slice(),
        ["0", "1", "2"],
        "TAKE 3 must stop the infinite producer; draining would hang"
    );
}

#[test]
fn helper_two_regina_stages_in_one_pipeline() {
    let _lock = env_lock();
    if skip_without_regina() {
        return;
    }
    let _oop = force_oop();
    install_hook();
    let rc = with_repo_cwd(|| {
        run_spec_with_return_code(
            "(ENDCHAR ?) literal hello | a: fanout | examples/upper.rexx | hole ? a: | examples/upper.rexx | hole",
        )
        .expect("two Regina stages")
    });
    assert_eq!(rc, 0);

    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let upper = repo_root().join("examples/upper.rexx");
    let u1 = upper.clone();
    let u2 = upper;
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            let _ = ctx.write(Record::from("ab"));
        }),
    );
    let r1 = b.add_stage(
        "r1",
        Box::new(move |ctx| {
            run_rexx_stage(ctx, u1.to_str().unwrap(), &[]).unwrap();
        }),
    );
    let r2 = b.add_stage(
        "r2",
        Box::new(move |ctx| {
            run_rexx_stage(ctx, u2.to_str().unwrap(), &[]).unwrap();
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
    b.connect_primary(src, r1);
    b.connect_primary(r1, r2);
    b.connect_primary(r2, sink);
    b.run().unwrap();
    assert_eq!(collected.borrow().as_slice(), ["AB"]);
}

#[test]
fn helper_exit_and_setrc() {
    let _lock = env_lock();
    if skip_without_regina() {
        return;
    }
    let _oop = force_oop();
    install_hook();

    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let script = write_script("exit5", "EXIT 5\n");
    let script_run = script.clone();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            let _ = ctx.write(Record::from("x"));
        }),
    );
    let rexx = b.add_stage(
        "rexx",
        Box::new(move |ctx| {
            run_rexx_stage(ctx, script_run.to_str().unwrap(), &[]).unwrap();
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
    let _ = std::fs::remove_file(&script);
    assert!(collected.borrow().is_empty());
    assert_eq!(rc, 5);

    let script = write_script("setrc7", "'SETRC 7'\n'OUTPUT' 'ok'\n");
    let script_run = script.clone();
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", Box::new(|_ctx| {}));
    let rexx = b.add_stage(
        "rexx",
        Box::new(move |ctx| {
            run_rexx_stage(ctx, script_run.to_str().unwrap(), &[]).unwrap();
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
    let _ = std::fs::remove_file(&script);
    assert_eq!(collected.borrow().as_slice(), ["ok"]);
    assert_eq!(rc, 7);
}

#[test]
fn helper_short_stops_producer() {
    let _lock = env_lock();
    if skip_without_regina() {
        return;
    }
    let _oop = force_oop();
    install_hook();

    // SHORT both sides would drop the in-flight OUTPUT (the parent has not
    // suspended, so the sink has not consumed the slot yet). SEVER INPUT
    // stops the producer and keeps the record already written.
    let script = write_script("short1", "'READTO REC'\n'OUTPUT' REC\n'SEVER INPUT'\n");
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let script_run = script.clone();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            for rec in ["one", "two", "three"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
            }
        }),
    );
    let rexx = b.add_stage(
        "rexx",
        Box::new(move |ctx| {
            run_rexx_stage(ctx, script_run.to_str().unwrap(), &[]).unwrap();
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
    let _ = std::fs::remove_file(&script);
    assert_eq!(collected.borrow().as_slice(), ["one"]);
}

#[test]
fn helper_short_command_does_not_emit_after() {
    let _lock = env_lock();
    if skip_without_regina() {
        return;
    }
    let _oop = force_oop();
    install_hook();

    let script = write_script(
        "short-cmd",
        "'READTO REC'\n'OUTPUT' REC\n'SHORT'\n'OUTPUT' 'after'\n",
    );
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let script_run = script.clone();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            for rec in ["one", "two", "three"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
            }
        }),
    );
    let rexx = b.add_stage(
        "rexx",
        Box::new(move |ctx| {
            run_rexx_stage(ctx, script_run.to_str().unwrap(), &[]).unwrap();
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
    let _ = std::fs::remove_file(&script);
    assert!(
        !collected.borrow().iter().any(|s| s == "after"),
        "SHORT must stop further OUTPUT, got {:?}",
        collected.borrow()
    );
    assert!(
        !collected
            .borrow()
            .iter()
            .any(|s| s == "two" || s == "three"),
        "producer must stop, got {:?}",
        collected.borrow()
    );
}

#[test]
fn helper_crash_is_a_clear_error() {
    let _lock = env_lock();
    if skip_without_regina() {
        return;
    }
    let _oop = force_oop();
    install_hook();
    crate::helper::crash_next_helper();

    let script = write_script("crash", "'OUTPUT' 'x'\n");
    let script_run = script.clone();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            let _ = ctx.write(Record::from("x"));
        }),
    );
    let err = Rc::new(RefCell::new(None));
    let err_slot = Rc::clone(&err);
    let rexx = b.add_stage(
        "rexx",
        Box::new(
            move |ctx| match run_rexx_stage(ctx, script_run.to_str().unwrap(), &[]) {
                Ok(()) => {}
                Err(e) => {
                    *err_slot.borrow_mut() = Some(e.to_string());
                    ctx.fail(e.to_string());
                }
            },
        ),
    );
    let sink = b.add_stage(
        "sink",
        Box::new(|ctx| while ctx.read().ok().flatten().is_some() {}),
    );
    b.connect_primary(src, rexx);
    b.connect_primary(rexx, sink);
    let result = b.run();
    let _ = std::fs::remove_file(&script);
    let msg = err
        .borrow()
        .clone()
        .or_else(|| result.err().map(|e| e.to_string()))
        .unwrap_or_default();
    assert!(
        msg.contains("exited unexpectedly") || msg.contains("REXX helper"),
        "expected a clear helper-crash error, got {msg}"
    );
}
