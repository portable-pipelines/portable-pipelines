//! Marina nested example: Step one record must finish, matching Run.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pipe_core::{SessionStatus, StageId};
use pipe_gui::preview::{counts_for_focus, holding_count, RecordFocus};
use pipe_gui::worker::{SessionWorker, WorkerCmd, WorkerEvent};
use pipe_parser::parse_pipeline;
use pipe_stages::{inspect_topology, NodeKind};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn marina_spec(out: &Path) -> String {
    let path = repo_root().join("examples/marina/to-nested.pipe");
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let dest = out.to_string_lossy().replace('\\', "/");
    raw.replace("examples/marina/out/vessels.jsonl", &dest)
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

fn last_step(evs: &[WorkerEvent]) -> Option<String> {
    evs.iter().rev().find_map(|e| match e {
        WorkerEvent::Snapshot(s) => s.last_step.clone(),
        _ => None,
    })
}

fn errors(evs: &[WorkerEvent]) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match e {
            WorkerEvent::Error(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn temp_dir() -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!(
        "marina-step-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn run_marina(out: &Path) -> (SessionStatus, String) {
    let spec = marina_spec(out);
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec,
        cwd: repo_root(),
    });
    w.send(WorkerCmd::Run);
    let evs = wait_events(&w, Duration::from_secs(45), |evs| {
        !errors(evs).is_empty()
            || matches!(
                last_status(evs),
                Some(SessionStatus::Ended { .. })
                    | Some(SessionStatus::Failed(_))
                    | Some(SessionStatus::Stopped { .. })
            )
    });
    let errs = errors(&evs);
    assert!(
        errs.is_empty(),
        "marina Run worker error: {errs:?} events={evs:?}"
    );
    let status = last_status(&evs).expect("snapshot status");
    let body = fs::read_to_string(out).unwrap_or_default();
    (status, body)
}

#[test]
fn marina_run_finishes_rc0() {
    let dir = temp_dir();
    let out = dir.join("vessels.jsonl");
    let (status, body) = run_marina(&out);
    assert!(
        matches!(status, SessionStatus::Ended { rc: 0 }),
        "Run status={status:?}"
    );
    assert!(
        body.contains("Kestrel"),
        "run output missing Kestrel: {body:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn marina_step_none_selected_matches_run() {
    let dir = temp_dir();
    let run_out = dir.join("run.jsonl");
    let step_out = dir.join("step.jsonl");
    let (run_status, run_body) = run_marina(&run_out);
    assert!(
        matches!(run_status, SessionStatus::Ended { rc: 0 }),
        "Run status={run_status:?}"
    );

    let spec = marina_spec(&step_out);
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec,
        cwd: repo_root(),
    });
    let _ = wait_events(&w, Duration::from_secs(10), |evs| {
        evs.iter().any(|e| matches!(e, WorkerEvent::Snapshot(_)))
    });

    let mut last_note = String::new();
    let mut same = 0usize;
    for i in 0..400 {
        w.send(WorkerCmd::Step { stage: None });
        let evs = wait_events(&w, Duration::from_secs(20), |evs| {
            !errors(evs).is_empty() || last_step(evs).is_some()
        });
        let errs = errors(&evs);
        assert!(
            errs.is_empty(),
            "worker error at step {i}: {errs:?} events={evs:?}"
        );
        let status = last_status(&evs);
        let note = last_step(&evs).unwrap_or_default();
        if matches!(status, Some(SessionStatus::Ended { rc: 0 }))
            || note.to_ascii_lowercase().contains("pipeline ended (rc 0)")
        {
            let step_body = fs::read_to_string(&step_out).unwrap_or_default();
            assert_eq!(
                step_body, run_body,
                "Step-to-completion output must match Run after {i} steps"
            );
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        if matches!(
            status,
            Some(SessionStatus::Failed(_))
                | Some(SessionStatus::Ended { .. })
                | Some(SessionStatus::Stopped { .. })
        ) {
            panic!("terminal without success at step {i}: {status:?} note={note}");
        }
        if note == last_note {
            same += 1;
        } else {
            same = 0;
        }
        last_note = note.clone();
        assert!(
            same < 8,
            "Step stopped making progress at click {i}: {note}"
        );
    }
    panic!("marina did not finish in 400 Steps; last={last_note}");
}

fn sort_ids(spec: &str) -> Vec<StageId> {
    let topo = inspect_topology(&parse_pipeline(spec).unwrap());
    let mut ids = Vec::new();
    let mut runtime = 0usize;
    for n in &topo.nodes {
        if matches!(n.kind, NodeKind::LabelReference | NodeKind::Connector) {
            continue;
        }
        if n.verb.eq_ignore_ascii_case("SORT") {
            ids.push(StageId(runtime));
        }
        runtime += 1;
    }
    assert!(!ids.is_empty(), "marina spec has SORT stages");
    ids
}

#[test]
fn marina_sort_holds_then_drains() {
    let dir = temp_dir();
    let out = dir.join("step.jsonl");
    let spec = marina_spec(&out);
    let sorts = sort_ids(&spec);
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec,
        cwd: repo_root(),
    });
    let _ = wait_events(&w, Duration::from_secs(10), |evs| {
        evs.iter().any(|e| matches!(e, WorkerEvent::Snapshot(_)))
    });

    let mut peak = vec![0usize; sorts.len()];
    let mut saw = vec![false; sorts.len()];
    let mut dropped = false;
    for i in 0..400 {
        w.send(WorkerCmd::Step { stage: None });
        let evs = wait_events(&w, Duration::from_secs(20), |evs| {
            !errors(evs).is_empty() || last_step(evs).is_some()
        });
        assert!(errors(&evs).is_empty(), "worker error at step {i}");
        let snap = evs
            .iter()
            .rev()
            .find_map(|e| match e {
                WorkerEvent::Snapshot(s) => Some(s),
                _ => None,
            })
            .expect("snapshot");
        for (k, id) in sorts.iter().enumerate() {
            let c = counts_for_focus(&snap.events, RecordFocus::Stage(*id));
            match holding_count("SORT", c) {
                Some(n) => {
                    saw[k] = true;
                    peak[k] = peak[k].max(n);
                    if c.sent > 0 && n < peak[k] {
                        dropped = true;
                    }
                }
                None if saw[k] && c.sent > 0 => dropped = true,
                None => {}
            }
        }
        if dropped {
            assert!(
                saw.iter().any(|&s| s),
                "SORT holding then drain at step {i}"
            );
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        if matches!(
            snap.status,
            SessionStatus::Ended { .. } | SessionStatus::Failed(_) | SessionStatus::Stopped { .. }
        ) {
            panic!(
                "ended before SORT hold-then-drain: {:?} saw={saw:?} peak={peak:?}",
                snap.status
            );
        }
    }
    panic!("marina SORT never held then drained; saw={saw:?} peak={peak:?}");
}
