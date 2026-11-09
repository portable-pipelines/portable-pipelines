//! First-step, Step→Run vs Run, and CONSOLE capture for the locate spec.

use std::time::{Duration, Instant};

use pipe_core::{SessionStatus, StageId};
use pipe_gui::worker::{SessionWorker, WorkerCmd, WorkerEvent};
use pipe_parser::parse_pipeline;
use pipe_stages::{inspect_topology, NodeKind};

const SPEC: &str = "literal apple|literal banana|literal apricot|locate /ap/|console";

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

fn console_lines(evs: &[WorkerEvent]) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match e {
            WorkerEvent::Console(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn last_step(evs: &[WorkerEvent]) -> Option<String> {
    evs.iter().rev().find_map(|e| match e {
        WorkerEvent::Snapshot(s) => s.last_step.clone(),
        _ => None,
    })
}

fn last_status(evs: &[WorkerEvent]) -> Option<SessionStatus> {
    evs.iter().rev().find_map(|e| match e {
        WorkerEvent::Snapshot(s) => Some(s.status.clone()),
        _ => None,
    })
}

fn locate_id() -> StageId {
    let topo = inspect_topology(&parse_pipeline(SPEC).unwrap());
    let idx = topo
        .nodes
        .iter()
        .position(|n| n.verb == "LOCATE")
        .expect("LOCATE");
    let mut runtime = 0usize;
    for (i, n) in topo.nodes.iter().enumerate() {
        if matches!(n.kind, NodeKind::LabelReference | NodeKind::Connector) {
            continue;
        }
        if i == idx {
            return StageId(runtime);
        }
        runtime += 1;
    }
    panic!("no runtime id");
}

fn cwd() -> std::path::PathBuf {
    std::env::temp_dir()
}

fn prepare_and_run(w: &SessionWorker, spec: &str) -> Vec<WorkerEvent> {
    w.send(WorkerCmd::Prepare {
        spec: spec.into(),
        cwd: cwd(),
    });
    w.send(WorkerCmd::Run);
    wait_events(w, Duration::from_secs(6), |evs| {
        matches!(
            last_status(evs),
            Some(SessionStatus::Ended { .. }) | Some(SessionStatus::Failed(_))
        )
    })
}

#[test]
fn uninterrupted_run_console_is_apricot_then_apple() {
    let w = SessionWorker::start();
    let evs = prepare_and_run(&w, SPEC);
    let lines = console_lines(&evs);
    assert_eq!(
        lines,
        vec!["apricot".to_string(), "apple".to_string()],
        "uninterrupted Run; events={evs:?}"
    );
    assert!(matches!(
        last_status(&evs),
        Some(SessionStatus::Ended { rc: 0 })
    ));
}

#[test]
fn first_prepare_plus_step_is_not_idle_only() {
    let w = SessionWorker::start();
    let id = locate_id();
    w.send(WorkerCmd::Prepare {
        spec: SPEC.into(),
        cwd: cwd(),
    });
    w.send(WorkerCmd::Step { stage: Some(id) });
    let evs = wait_events(&w, Duration::from_secs(6), |evs| last_step(evs).is_some());
    let note = last_step(&evs).expect("first click must complete a step");
    assert!(
        !note.contains("nothing has run yet"),
        "first Step must not stop at idle prepare: {note} evs={evs:?}"
    );
    let cons = console_lines(&evs);
    assert!(
        note.contains("read") || note.contains("wrote") || note.contains("produced"),
        "first Step must be a record cycle, not idle: {note} console={cons:?}"
    );
}

#[test]
fn step_then_resume_run_does_not_duplicate_console() {
    let w = SessionWorker::start();
    let id = locate_id();
    w.send(WorkerCmd::Prepare {
        spec: SPEC.into(),
        cwd: cwd(),
    });
    let mut all = Vec::new();
    loop {
        w.send(WorkerCmd::Step { stage: Some(id) });
        let evs = wait_events(&w, Duration::from_secs(6), |evs| last_step(evs).is_some());
        all.extend(evs);
        let note = last_step(&all).unwrap_or_default();
        let low = note.to_ascii_lowercase();
        if low.contains("this stage ended") || low.contains("pipeline ended") {
            break;
        }
        if all.len() > 80 {
            panic!("too many steps without locate ending: {all:?}");
        }
    }
    let after_steps = console_lines(&all);
    w.send(WorkerCmd::Run);
    let rest = wait_events(&w, Duration::from_secs(6), |evs| {
        matches!(
            last_status(evs),
            Some(SessionStatus::Ended { .. }) | Some(SessionStatus::Failed(_))
        )
    });
    all.extend(rest);
    let after_run = console_lines(&all);
    assert_eq!(
        after_run,
        vec!["apricot".to_string(), "apple".to_string()],
        "Step then resume Run must match uninterrupted Run, not reprint. after_steps={after_steps:?} all={all:?}"
    );
    assert!(
        after_run.starts_with(after_steps.as_slice())
            || after_steps.iter().all(|s| after_run.contains(s)),
        "step output should be a prefix of the same records, not extras: steps={after_steps:?}"
    );
}

#[test]
fn fresh_run_after_ended_replays_same_records() {
    let w = SessionWorker::start();
    let first = console_lines(&prepare_and_run(&w, SPEC));
    let second = console_lines(&prepare_and_run(&w, SPEC));
    assert_eq!(first, vec!["apricot".to_string(), "apple".to_string()]);
    assert_eq!(second, first);
}

#[test]
fn branched_locate_primary_and_secondary_are_distinct() {
    let spec =
        "(ENDCHAR ?) literal apple|literal banana|literal apricot| L: locate /ap/| console ? L: | hole";
    let w = SessionWorker::start();
    let evs = prepare_and_run(&w, spec);
    assert_eq!(
        console_lines(&evs),
        vec!["apricot".to_string(), "apple".to_string()]
    );
}

#[test]
fn inspect_python_verb_is_not_unknown_builtin() {
    let parsed = parse_pipeline("literal a | examples/marina/tuck.py owner | hole").unwrap();
    let topo = inspect_topology(&parsed);
    assert!(topo.nodes.iter().any(|n| n.verb.ends_with("tuck.py")));
    assert!(pipe_stages::is_language_stage_verb(
        &topo
            .nodes
            .iter()
            .find(|n| n.verb.ends_with("tuck.py"))
            .unwrap()
            .verb
    ));
    assert!(!pipe_stages::StageRegistry::default().contains(
        &topo
            .nodes
            .iter()
            .find(|n| n.verb.ends_with("tuck.py"))
            .unwrap()
            .verb
    ));
}
