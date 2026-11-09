//! Marina nested example: Step one record must finish, matching Run.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Workers `set_current_dir`; tests in this file must not run that concurrently.
fn lock_cwd() -> MutexGuard<'static, ()> {
    static M: Mutex<()> = Mutex::new(());
    M.lock().unwrap_or_else(|e| e.into_inner())
}

use pipe_core::{SessionStatus, StageId};
use pipe_gui::preview::{counts_for_stage, holding_count};
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
    let _cwd = lock_cwd();
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
    let _cwd = lock_cwd();
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
    let _cwd = lock_cwd();
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
            let c = counts_for_stage(&snap.events, *id, &snap.io_counts);
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

fn last_snap(evs: &[WorkerEvent]) -> &pipe_gui::worker::Snapshot {
    evs.iter()
        .rev()
        .find_map(|e| match e {
            WorkerEvent::Snapshot(s) => Some(s),
            _ => None,
        })
        .expect("snapshot")
}

fn live_named<'a>(snap: &'a pipe_gui::worker::Snapshot, needle: &str) -> &'a pipe_core::LiveStage {
    let key = needle.to_ascii_lowercase();
    snap.live
        .iter()
        .find(|l| l.name.to_ascii_lowercase().contains(&key))
        .unwrap_or_else(|| {
            let names: Vec<_> = snap.live.iter().map(|l| l.name.as_str()).collect();
            panic!("no live stage {needle}: {names:?}")
        })
}

#[test]
fn marina_run_in_out_badges_on_csv_and_lookup() {
    use pipe_gui::preview::stage_badges_on;
    let _cwd = lock_cwd();
    let dir = temp_dir();
    let out = dir.join("vessels.jsonl");
    let spec = marina_spec(&out);
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.clone(),
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
    assert!(errors(&evs).is_empty(), "{:?}", errors(&evs));
    let snap = last_snap(&evs);
    assert!(matches!(snap.status, SessionStatus::Ended { rc: 0 }));
    let csv = live_named(snap, "csv");
    let csv_b = stage_badges_on(snap, csv.id, &csv.name, csv.ended);
    assert!(
        csv_b.received > 0 && csv_b.sent > 0,
        "CSV should show in and out: {csv_b:?} live={:?}",
        snap.live
    );
    assert!(csv_b.in_tip.starts_with("Read "), "{}", csv_b.in_tip);
    assert!(csv_b.out_tip.starts_with("Sent "), "{}", csv_b.out_tip);
    let lookup = live_named(snap, "lookup");
    let look_b = stage_badges_on(snap, lookup.id, &lookup.name, lookup.ended);
    assert!(
        look_b.received > 0 && look_b.sent > 0,
        "LOOKUP should show in and out: {look_b:?}"
    );
    let disk = snap
        .live
        .iter()
        .find(|l| l.name == "<")
        .expect("file source");
    let disk_b = stage_badges_on(snap, disk.id, &disk.name, disk.ended);
    assert_eq!(
        disk_b.received, 0,
        "file source should not show an in-0 badge ({})",
        disk.name
    );
    let _ = fs::remove_dir_all(&dir);
}

fn csv_histories(
    snap: &pipe_gui::worker::Snapshot,
) -> Vec<(
    StageId,
    String,
    pipe_gui::preview::RecordHistory,
    pipe_gui::preview::StageBadges,
)> {
    use pipe_gui::preview::{history_for, stage_badges_on, RecordFocus};
    snap.live
        .iter()
        .filter(|l| l.name.to_ascii_lowercase().contains("csv"))
        .map(|l| {
            let hist = history_for(&snap.events, RecordFocus::Stage(l.id), &snap.record_logs);
            let badges = stage_badges_on(snap, l.id, &l.name, l.ended);
            (l.id, l.name.clone(), hist, badges)
        })
        .collect()
}

fn assert_csv_grids_match_counts(snap: &pipe_gui::worker::Snapshot) {
    use pipe_gui::preview::{history_for, records_table, RecordFocus};
    let csvs = csv_histories(snap);
    assert!(
        !csvs.is_empty(),
        "marina has CSV stages: {:?}",
        snap.live
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>()
    );
    let mut saw_vessels = false;
    let mut saw_berths = false;
    for (id, name, hist, badges) in &csvs {
        assert_eq!(
            hist.received.len(),
            badges.received,
            "{name} received rows vs in-count"
        );
        assert_eq!(
            hist.sent.len(),
            badges.sent,
            "{name} sent rows vs out-count"
        );
        assert_eq!(hist.received_total, badges.received);
        assert_eq!(hist.sent_total, badges.sent);
        assert!(
            hist.received.len() > 1,
            "{name} must list every read, not 1 row (kept {})",
            hist.received.len()
        );
        assert!(
            hist.sent.len() > 1,
            "{name} must list every write, not 1 row (kept {})",
            hist.sent.len()
        );
        let recv = records_table(&hist.received);
        let sent = records_table(&hist.sent);
        assert_eq!(recv.rows.len(), hist.received.len());
        assert_eq!(sent.rows.len(), hist.sent.len());
        let out_edge = RecordFocus::Edge {
            writer: *id,
            writer_stream: 0,
            reader: StageId(id.0 + 1),
            reader_stream: 0,
        };
        let edge_hist = history_for(&snap.events, out_edge, &snap.record_logs);
        assert_eq!(
            edge_hist.sent.len(),
            hist.sent.len(),
            "{name} edge sent rows"
        );
        if hist.received.len() == 6 && hist.sent.len() == 5 {
            saw_vessels = true;
        }
        if hist.received.len() == 7 && hist.sent.len() == 6 {
            saw_berths = true;
        }
    }
    assert!(
        saw_vessels,
        "vessels.csv CSV should keep 6 received / 5 sent (header consumed)"
    );
    assert!(saw_berths, "berths.csv CSV should keep 7 received / 6 sent");
}

#[test]
fn marina_csv_grids_match_in_out_after_run() {
    use pipe_gui::preview::stage_badges_on;
    let _cwd = lock_cwd();
    let dir = temp_dir();
    let out = dir.join("vessels.jsonl");
    let spec = marina_spec(&out);
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.clone(),
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
    assert!(errors(&evs).is_empty(), "{:?}", errors(&evs));
    let snap = last_snap(&evs);
    assert!(matches!(snap.status, SessionStatus::Ended { rc: 0 }));
    let csv = live_named(snap, "csv");
    let csv_b = stage_badges_on(snap, csv.id, &csv.name, csv.ended);
    assert!(csv_b.received > 1 && csv_b.sent > 1);
    assert_csv_grids_match_counts(snap);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn marina_csv_grids_match_after_step_then_run() {
    let _cwd = lock_cwd();
    let dir = temp_dir();
    let out = dir.join("vessels.jsonl");
    let spec = marina_spec(&out);
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.clone(),
        cwd: repo_root(),
    });
    let csv = {
        let evs = wait_events(&w, Duration::from_secs(10), |evs| {
            evs.iter().any(|e| matches!(e, WorkerEvent::Snapshot(_)))
        });
        last_snap(&evs)
            .live
            .iter()
            .find(|l| l.name.to_ascii_lowercase().contains("csv"))
            .map(|l| l.id)
            .expect("csv stage")
    };
    w.send(WorkerCmd::Step { stage: Some(csv) });
    let step_evs = wait_events(&w, Duration::from_secs(20), |evs| {
        !errors(evs).is_empty() || last_step(evs).is_some()
    });
    assert!(errors(&step_evs).is_empty(), "{:?}", errors(&step_evs));
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
    assert!(errors(&evs).is_empty(), "{:?}", errors(&evs));
    let snap = last_snap(&evs);
    assert!(matches!(snap.status, SessionStatus::Ended { rc: 0 }));
    assert_csv_grids_match_counts(snap);
    let _ = fs::remove_dir_all(&dir);
}
