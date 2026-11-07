//! Time-bounded SessionWorker shutdown. The cases run in a child process so a
//! hung join cannot stall `cargo test`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pipe_core::{SessionStatus, StageId};
use pipe_gui::worker::{SessionWorker, WorkerCmd, WorkerEvent};

fn unique_file(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    // One path segment under temp_dir. `/tmp/dir/file` is parsed as a
    // delimited `/dir/` string; `/tmp/pipe-gui-shut-…-tag` is a Unix path.
    std::env::temp_dir().join(format!(
        "pipe-gui-shut-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

fn unique_dir() -> PathBuf {
    let p = unique_file("cwd");
    fs::create_dir_all(&p).unwrap();
    p
}

fn path_spec(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn wait_events(
    w: &SessionWorker,
    timeout: Duration,
    pred: impl Fn(&[WorkerEvent]) -> bool,
) -> Vec<WorkerEvent> {
    let start = Instant::now();
    let mut all = Vec::new();
    while start.elapsed() < timeout {
        all.extend(w.try_recv());
        if pred(&all) {
            return all;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timeout waiting for worker events: {all:?}");
}

fn last_status(evs: &[WorkerEvent]) -> Option<SessionStatus> {
    evs.iter().rev().find_map(|e| match e {
        WorkerEvent::Snapshot(s) => Some(s.status.clone()),
        _ => None,
    })
}

fn prepare(w: &SessionWorker, spec: &str, cwd: &Path) {
    w.send(WorkerCmd::Prepare {
        spec: spec.to_string(),
        cwd: cwd.to_path_buf(),
    });
    wait_events(w, Duration::from_secs(4), |evs| {
        evs.iter().any(|e| match e {
            WorkerEvent::Info(s) => s.contains("Session ready"),
            WorkerEvent::Snapshot(_) => true,
            _ => false,
        })
    });
}

fn assert_shutdown(mut w: SessionWorker, label: &str) {
    let start = Instant::now();
    if !w.shutdown(Duration::from_secs(5)) {
        std::mem::forget(w);
        panic!("{label}: worker did not stop in 5s");
    }
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{label}: shutdown took {:?}",
        start.elapsed()
    );
}

fn python_required() -> bool {
    std::env::var("PIPE_REQUIRE_PYTHON").ok().as_deref() == Some("1")
}

fn run_cases() {
    close_idle();
    close_idle_prepared();
    close_stepped();
    close_paused_does_not_finish_writes();
    close_running_delay();
    close_running_console_backpressure();
    close_marina_like_fanout();
    close_python_sleep_helper();
}

fn close_idle() {
    let w = SessionWorker::start();
    assert_shutdown(w, "idle");
}

fn close_idle_prepared() {
    let dir = unique_dir();
    let w = SessionWorker::start();
    prepare(&w, "literal hello | hole", &dir);
    assert_shutdown(w, "idle-prepared");
}

fn close_stepped() {
    let dir = unique_dir();
    let w = SessionWorker::start();
    prepare(&w, "literal hello | hole", &dir);
    w.send(WorkerCmd::Step { stage: StageId(0) });
    wait_events(&w, Duration::from_secs(4), |evs| {
        evs.iter()
            .any(|e| matches!(e, WorkerEvent::Snapshot(s) if s.last_step.is_some()))
    });
    assert_shutdown(w, "stepped");
}

fn close_paused_does_not_finish_writes() {
    let dir = unique_dir();
    let output = unique_file("pause-out.txt");
    // DELAY keeps the session live; Quit/stop must abort, not wait out the timer.
    let spec = format!("literal +15 | delay | > {}", path_spec(&output));
    let w = SessionWorker::start();
    prepare(&w, &spec, &dir);
    w.send(WorkerCmd::Run);
    wait_events(&w, Duration::from_secs(4), |evs| {
        matches!(last_status(evs), Some(SessionStatus::Running))
            || evs
                .iter()
                .any(|e| matches!(e, WorkerEvent::Info(s) if s.contains("running")))
    });
    w.request_pause();
    w.send(WorkerCmd::Pause);
    let _ = wait_events(&w, Duration::from_secs(4), |evs| {
        matches!(
            last_status(evs),
            Some(SessionStatus::Paused)
                | Some(SessionStatus::Ended { .. })
                | Some(SessionStatus::Stopped { .. })
        )
    });
    let mid = fs::read(&output).unwrap_or_default();
    assert_shutdown(w, "paused");
    let after = fs::read(&output).unwrap_or_default();
    assert_eq!(
        mid, after,
        "closing a paused session must not resume writes to completion"
    );
    assert!(
        after.is_empty(),
        "stop during DELAY must not wait out the timer and write: {} bytes",
        after.len()
    );
}

fn close_running_delay() {
    let dir = unique_dir();
    let w = SessionWorker::start();
    prepare(&w, "literal +30 | delay | hole", &dir);
    w.send(WorkerCmd::Run);
    std::thread::sleep(Duration::from_millis(80));
    assert_shutdown(w, "running-delay");
}

fn close_running_console_backpressure() {
    let dir = unique_dir();
    let input = unique_file("many.txt");
    let mut body = String::new();
    for i in 0..400 {
        body.push_str(&format!("line{i}\n"));
    }
    fs::write(&input, body).unwrap();
    let spec = format!("< {} | console", path_spec(&input));
    let w = SessionWorker::start();
    prepare(&w, &spec, &dir);
    w.send(WorkerCmd::Run);
    std::thread::sleep(Duration::from_millis(50));
    assert_shutdown(w, "console-backpressure");
}

fn close_marina_like_fanout() {
    let dir = unique_dir();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let src = repo.join("examples/marina/vessels.jsonl");
    let jsonl = unique_file("vessels.jsonl");
    fs::copy(&src, &jsonl).unwrap_or_else(|e| panic!("copy marina jsonl: {e}"));
    let v = unique_file("vessels.csv");
    let o = unique_file("owners.csv");
    let b = unique_file("berths.csv");
    let w = unique_file("work.csv");
    let spec = format!(
        "(ENDCHAR ?) < {jsonl} | f: fanout | json keep $vessel_id $name $type $berth_id $owner_id | csv write | > {v} ? f: | spec $owner 1 | sort $owner_id | unique | csv write | > {o} ? f: | spec $berth 1 | sort $berth_id | unique | csv write | > {b} ? f: | json split $work_orders | sort $work_order_id | csv write | > {w}",
        jsonl = path_spec(&jsonl),
        v = path_spec(&v),
        o = path_spec(&o),
        b = path_spec(&b),
        w = path_spec(&w),
    );
    let worker = SessionWorker::start();
    prepare(&worker, &spec, &dir);
    worker.send(WorkerCmd::Run);
    std::thread::sleep(Duration::from_millis(80));
    assert_shutdown(worker, "marina-fanout");
}

fn close_python_sleep_helper() {
    let dir = unique_dir();
    // Verb must be a relative name: `/tmp/foo.py` is tokenized as delimited `/tmp/`.
    let script = dir.join("sleep.py");
    fs::write(
        &script,
        "import time\nimport pipelines as p\ntime.sleep(30)\nfor rec in p.records():\n    p.output(rec)\n",
    )
    .unwrap();
    let spec = "literal hi | sleep.py | hole";
    let w = SessionWorker::start();
    prepare(&w, &spec, &dir);
    w.send(WorkerCmd::Run);
    std::thread::sleep(Duration::from_millis(200));
    let start = Instant::now();
    let mut w = w;
    if !w.shutdown(Duration::from_secs(5)) {
        std::mem::forget(w);
        if python_required() {
            panic!(
                "python helper did not stop in {:?}; Quit would have hung the UI join",
                start.elapsed()
            );
        }
        eprintln!(
            "skip python helper shutdown: interpreter or script failed to become interruptible"
        );
        return;
    }
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "python helper shutdown took {:?}",
        start.elapsed()
    );
}

#[test]
fn shutdown_cases_in_child_process() {
    if std::env::var_os("PIPE_GUI_SHUTDOWN_CHILD").is_some() {
        run_cases();
        return;
    }
    let exe = std::env::current_exe().expect("test exe");
    let mut child = Command::new(exe)
        .env("PIPE_GUI_SHUTDOWN_CHILD", "1")
        .args(["--exact", "shutdown_cases_in_child_process"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn shutdown child");
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let start = Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(25) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("shutdown child process exceeded 25s; killed the test process only");
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                use std::io::Read;
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut out) = stdout_pipe.take() {
                    let _ = out.read_to_string(&mut stdout);
                }
                if let Some(mut err) = stderr_pipe.take() {
                    let _ = err.read_to_string(&mut stderr);
                }
                assert!(
                    status.success(),
                    "shutdown child failed {status}: stdout={stdout} stderr={stderr}"
                );
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("wait: {e}"),
        }
    }
}

#[test]
fn gui_binary_quit_after_ms_idle() {
    if cfg!(unix)
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
    {
        return;
    }
    let bin = env!("CARGO_BIN_EXE_pipe-gui");
    let mut child = Command::new(bin)
        .args(["--spec", "literal hello | hole", "--quit-after-ms", "900"])
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn pipe-gui");
    let start = Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("pipe-gui --quit-after-ms did not exit in 20s");
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                assert!(status.success(), "pipe-gui idle quit failed: {status}");
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!("wait: {e}"),
        }
    }
}
