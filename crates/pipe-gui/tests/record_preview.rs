//! Live dispatcher snapshots: mini-table shape and edge outgoing attribution.

use std::time::{Duration, Instant};

use pipe_core::{SessionStatus, StageId};
use pipe_gui::preview::{
    counts_for_focus, mini_table, preview_copy, recent_items, waiting_items, FocusLabels, MiniCell,
    MiniKind, ObservedKind, RecordFocus, WaitingKind,
};
use pipe_gui::worker::{SessionWorker, WorkerCmd, WorkerEvent};
use pipe_parser::parse_pipeline;
use pipe_stages::{inspect_topology, NodeKind};

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

fn last_snap(evs: &[WorkerEvent]) -> &pipe_gui::worker::Snapshot {
    evs.iter()
        .rev()
        .find_map(|e| match e {
            WorkerEvent::Snapshot(s) => Some(s),
            _ => None,
        })
        .expect("snapshot")
}

fn runtime_id(spec: &str, verb: &str) -> StageId {
    let topo = inspect_topology(&parse_pipeline(spec).unwrap());
    let idx = topo
        .nodes
        .iter()
        .position(|n| n.verb.eq_ignore_ascii_case(verb))
        .unwrap_or_else(|| panic!("missing {verb} in {spec}"));
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
    panic!("no runtime id for {verb}");
}

fn step_until(w: &SessionWorker, stage: StageId) -> Vec<WorkerEvent> {
    w.send(WorkerCmd::Step { stage: Some(stage) });
    wait_events(w, Duration::from_secs(6), |evs| {
        evs.iter().any(|e| match e {
            WorkerEvent::Snapshot(s) => s.last_step.is_some(),
            _ => false,
        })
    })
}

#[test]
fn locate_first_edge_waiting_is_plain_apple_grid() {
    let spec = "literal apple|literal banana|literal apricot|locate /ap/|console";
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.into(),
        cwd: std::env::temp_dir(),
    });
    let writer = runtime_id(spec, "literal");
    let evs = step_until(&w, writer);
    let snap = last_snap(&evs);
    assert!(
        matches!(snap.status, SessionStatus::Paused | SessionStatus::Running),
        "{:?}",
        snap.status
    );
    let focus = RecordFocus::Edge {
        writer,
        writer_stream: 0,
        reader: StageId(writer.0 + 1),
        reader_stream: 0,
    };
    let wait = waiting_items(&snap.slots, focus);
    let waiting_rec = wait
        .iter()
        .find(|w| w.kind == WaitingKind::Waiting)
        .and_then(|w| w.record.as_ref())
        .expect("apple should sit on the first connection");
    let table = mini_table(waiting_rec);
    assert_eq!(table.kind, MiniKind::Text);
    assert!(table.headers.is_empty());
    assert_eq!(table.rows[0][0], MiniCell::Text("apple".into()));
    let recent = recent_items(&snap.events, focus, 8);
    assert!(
        recent.iter().all(|o| o.kind != ObservedKind::CameIn),
        "first edge must not show the next stage's input: {recent:?}"
    );
    assert_eq!(counts_for_focus(&snap.events, focus).received, 0);
    assert!(counts_for_focus(&snap.events, focus).sent >= 1);
    let copy = preview_copy(
        counts_for_focus(&snap.events, focus),
        &wait,
        &recent,
        focus,
        &FocusLabels::edge("LITERAL", "LITERAL"),
        3,
    );
    assert_eq!(copy.waiting[0].heading, "Waiting for LITERAL to read");
    assert!(copy.recent[0].still_waiting);
    assert!(!copy.summary.contains("out "));
    assert!(!copy.recent_heading.to_ascii_lowercase().contains("stream"));
}

#[test]
fn csv_header_then_object_outgoing_attribution() {
    let dir = std::env::temp_dir().join(format!("pipe-gui-csv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("vessels.csv");
    std::fs::write(&file, "vessel_id,name\nV1,Kestrel\n").unwrap();
    let spec = format!("< {} | csv | hole", file.display());
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.clone(),
        cwd: dir.clone(),
    });
    let disk = runtime_id(&spec, "<");
    let csv = runtime_id(&spec, "csv");
    let hole = runtime_id(&spec, "hole");

    let evs = step_until(&w, disk);
    let snap = last_snap(&evs);
    let into_csv = RecordFocus::Edge {
        writer: disk,
        writer_stream: 0,
        reader: csv,
        reader_stream: 0,
    };
    let wait = waiting_items(&snap.slots, into_csv);
    let rec = wait
        .iter()
        .find(|w| w.record.is_some())
        .and_then(|w| w.record.as_ref())
        .expect("header line waiting for CSV");
    let t = mini_table(rec);
    assert_eq!(t.kind, MiniKind::Text, "raw CSV line is not structured");
    assert!(
        t.headers.is_empty(),
        "must not invent CSV columns: {:?}",
        t.headers
    );
    match &t.rows[0][0] {
        MiniCell::Text(s) => assert!(s.contains("vessel_id"), "{s}"),
        other => panic!("{other:?}"),
    }

    let evs = step_until(&w, csv);
    let snap = last_snap(&evs);
    let out_csv = RecordFocus::Edge {
        writer: csv,
        writer_stream: 0,
        reader: hole,
        reader_stream: 0,
    };
    assert_eq!(
        counts_for_focus(&snap.events, out_csv).sent,
        0,
        "header is consumed, not written: events={:?}",
        snap.events
    );
    assert!(recent_items(&snap.events, out_csv, 8).is_empty());
    let wait_out = waiting_items(&snap.slots, out_csv);
    assert!(
        wait_out.iter().all(|w| w.record.is_none()),
        "no outgoing waiting record before a CSV write: {wait_out:?}"
    );
    let labels = FocusLabels::edge("CSV", "V: LOOKUP");
    let empty_copy = preview_copy(
        counts_for_focus(&snap.events, out_csv),
        &wait_out,
        &recent_items(&snap.events, out_csv, 3),
        out_csv,
        &labels,
        3,
    );
    assert!(empty_copy.waiting[0].heading.is_empty());
    assert!(empty_copy.waiting[0]
        .tooltip
        .contains("Nothing waiting for V: LOOKUP"));
    assert_eq!(
        empty_copy.recent_empty.as_deref(),
        Some("Nothing sent yet.")
    );
    assert_eq!(empty_copy.summary, "0 records sent · 0 discarded");

    let evs = step_until(&w, csv);
    let snap = last_snap(&evs);
    let sent = counts_for_focus(&snap.events, out_csv).sent;
    assert!(
        sent >= 1,
        "CSV should emit the first object, sent={sent} events={:?}",
        snap.events
    );
    let recent = recent_items(&snap.events, out_csv, 8);
    assert!(
        recent.iter().all(|o| o.kind != ObservedKind::CameIn),
        "outgoing edge must not list CSV's header read: {recent:?}"
    );
    let rec = recent
        .iter()
        .rev()
        .find(|o| o.kind == ObservedKind::WentOut)
        .and_then(|o| o.record.as_ref())
        .expect("JSON object went out on CSV output");
    let t = mini_table(rec);
    assert_eq!(t.kind, MiniKind::Structured);
    assert!(
        t.headers.iter().any(|h| h == "vessel_id"),
        "{:?}",
        t.headers
    );
    assert!(t.headers.iter().any(|h| h == "name"), "{:?}", t.headers);
    let wait_obj = waiting_items(&snap.slots, out_csv);
    let obj_copy = preview_copy(
        counts_for_focus(&snap.events, out_csv),
        &wait_obj,
        &recent,
        out_csv,
        &FocusLabels::edge("CSV", "V: LOOKUP"),
        8,
    );
    assert!(obj_copy.summary.starts_with("1 record sent"));
    assert_eq!(
        obj_copy.recent_heading,
        "Recently sent from CSV (last 8 kept)"
    );
    assert!(obj_copy.recent.iter().any(|r| r.still_waiting));
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir(&dir);
}
