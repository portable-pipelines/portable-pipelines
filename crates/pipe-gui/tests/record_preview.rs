//! Live dispatcher snapshots: mini-table shape and edge outgoing attribution.

use std::time::{Duration, Instant};

use pipe_core::{SessionStatus, StageId};
use pipe_gui::preview::{
    counts_for_focus, history_for, holding_count, mini_table, preview_copy, recent_items,
    records_table, stage_badges, stage_badges_on, waiting_items, FocusLabels, MiniCell, MiniKind,
    ObservedKind, RecordFocus, WaitingKind,
};
use pipe_gui::sheet::{cell_full, sheet_headers};
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
    assert_eq!(sheet_headers(&table), vec!["value".to_string()]);
    assert_eq!(table.rows[0][0], MiniCell::Text("apple".into()));
    let recent = recent_items(&snap.events, focus, 8);
    assert!(
        recent.iter().all(|o| o.kind != ObservedKind::CameIn),
        "first edge must not show the next stage's input: {recent:?}"
    );
    assert_eq!(counts_for_focus(&snap.events, focus).received, 0);
    assert!(counts_for_focus(&snap.events, focus).sent >= 1);
    let hist = history_for(&snap.events, focus, &snap.record_logs);
    let copy = preview_copy(
        counts_for_focus(&snap.events, focus),
        &wait,
        &hist,
        focus,
        &FocusLabels::edge("LITERAL", "LITERAL"),
    );
    assert_eq!(copy.waiting[0].heading, "Waiting for LITERAL to read");
    assert!(copy.sent_still_waiting);
    assert!(!copy.summary.contains("out "));
    assert_eq!(copy.sent_table.rows.len(), hist.sent.len());
    assert!(hist.sent.len() >= 1);
}

#[test]
fn nested_json_grid_keeps_owner_as_nested_cell() {
    let bytes = br#"{"id":"V1","owner":{"name":"Ann","phone":"207-555-0142"}}"#;
    let rec = pipe_core::RecordPreview {
        len: bytes.len(),
        bytes: bytes.to_vec(),
    };
    let table = mini_table(&rec);
    assert_eq!(table.kind, MiniKind::Structured);
    assert!(table.headers.iter().any(|h| h == "id"));
    assert!(table.headers.iter().any(|h| h == "owner"));
    assert_eq!(sheet_headers(&table), table.headers);
    assert!(matches!(table.rows[0][1], MiniCell::Nested { .. }));
    assert!(cell_full(&table.rows[0][1]).contains("Ann"));
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
    assert_eq!(sheet_headers(&t), vec!["value".to_string()]);
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
        &history_for(&snap.events, out_csv, &snap.record_logs),
        out_csv,
        &labels,
    );
    assert!(empty_copy.waiting[0].heading.is_empty());
    assert!(empty_copy.waiting[0]
        .tooltip
        .contains("Idle, waiting for input (V: LOOKUP)"));
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
    assert_eq!(sheet_headers(&t), t.headers);
    let wait_obj = waiting_items(&snap.slots, out_csv);
    let obj_copy = preview_copy(
        counts_for_focus(&snap.events, out_csv),
        &wait_obj,
        &history_for(&snap.events, out_csv, &snap.record_logs),
        out_csv,
        &FocusLabels::edge("CSV", "V: LOOKUP"),
    );
    assert!(obj_copy.summary.starts_with("1 record sent"));
    assert_eq!(obj_copy.sent_heading.as_deref(), Some("Sent"));
    assert_eq!(obj_copy.sent_table.rows.len(), 1);
    assert!(obj_copy.sent_still_waiting);
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn sort_holds_unread_emits_after_ingest() {
    let spec = "literal w5|literal w4|literal w3|literal w2|literal w1|sort|hole";
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.into(),
        cwd: std::env::temp_dir(),
    });
    let sort = runtime_id(spec, "sort");
    let mut holding = None;
    let mut sent = 0usize;
    for _ in 0..24 {
        let evs = step_until(&w, sort);
        let snap = last_snap(&evs);
        let c = counts_for_focus(&snap.events, RecordFocus::Stage(sort));
        holding = holding_count("SORT", c);
        sent = c.sent;
        if sent >= 1 {
            assert_eq!(
                holding,
                Some(c.received.saturating_sub(c.sent + c.discarded)),
                "SORT holding must be received minus sent/discarded: {c:?}"
            );
            assert!(
                holding.unwrap() >= 1,
                "after first emit SORT should still hold the rest: {c:?}"
            );
            assert_eq!(
                stage_badges(&snap.events, &snap.slots, sort, "SORT", false).holding,
                holding
            );
            let peak = holding.unwrap();
            let mut last = peak;
            for _ in 0..16 {
                let evs = step_until(&w, sort);
                let snap = last_snap(&evs);
                let c = counts_for_focus(&snap.events, RecordFocus::Stage(sort));
                let h = holding_count("SORT", c);
                if c.sent > sent {
                    match h {
                        Some(n) => assert!(
                            n < last,
                            "holding must drop as SORT emits: was {last} now {n} {c:?}"
                        ),
                        None => assert!(
                            c.received <= c.sent + c.discarded,
                            "empty bag after drain: {c:?}"
                        ),
                    }
                    return;
                }
                if let Some(n) = h {
                    last = n;
                }
            }
            panic!("SORT emitted but holding did not drop; last={last} sent={sent}");
        }
    }
    panic!("SORT never emitted; last holding={holding:?} sent={sent}");
}

#[test]
fn unselected_step_sort_holding_then_drain() {
    let spec = "literal w5|literal w4|literal w3|literal w2|literal w1|sort|hole";
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.into(),
        cwd: std::env::temp_dir(),
    });
    let sort = runtime_id(spec, "sort");
    let mut peak = 0usize;
    let mut saw = false;
    for _ in 0..40 {
        w.send(WorkerCmd::Step { stage: None });
        let evs = wait_events(&w, Duration::from_secs(6), |evs| {
            evs.iter()
                .any(|e| matches!(e, WorkerEvent::Snapshot(s) if s.last_step.is_some()))
        });
        let snap = last_snap(&evs);
        let c = counts_for_focus(&snap.events, RecordFocus::Stage(sort));
        let h = holding_count("SORT", c);
        if let Some(n) = h {
            saw = true;
            peak = peak.max(n);
        } else if saw && c.sent > 0 {
            assert!(
                peak >= 1,
                "SORT must have held before draining; {c:?} peak={peak}"
            );
            return;
        }
        if c.sent > 0 && saw {
            if let Some(n) = h {
                assert!(n < peak, "holding dropped after emit: peak={peak} now={n}");
            }
            return;
        }
    }
    panic!("unselected Step never showed SORT hold-then-drain; peak={peak}");
}

#[test]
fn ended_stage_reports_done_after_run() {
    use pipe_gui::symbols::{stage_run_mark, RecordMark};
    let spec = "literal apple|hole";
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: spec.into(),
        cwd: std::env::temp_dir(),
    });
    w.send(WorkerCmd::Run);
    let evs = wait_events(&w, Duration::from_secs(6), |evs| {
        evs.iter().rev().any(|e| matches!(e, WorkerEvent::Snapshot(s) if matches!(s.status, SessionStatus::Ended { .. } | SessionStatus::Failed(_))))
    });
    let snap = last_snap(&evs);
    assert!(
        matches!(snap.status, SessionStatus::Ended { rc: 0 }),
        "{:?}",
        snap.status
    );
    assert!(
        !snap.live.is_empty() && snap.live.iter().all(|l| l.ended),
        "every runtime stage must have ended: {:?}",
        snap.live
    );
    for live in &snap.live {
        let b = stage_badges_on(snap, live.id, &live.name, live.ended);
        assert!(b.ended, "{} should be ended", live.name);
        assert_eq!(
            stage_run_mark(b.ended, b.waiting_in),
            RecordMark::Done,
            "{} waiting_in={}",
            live.name,
            b.waiting_in
        );
    }
}

const LOCATE_SPEC: &str = "literal apple|literal banana|literal apricot|locate /ap/|console";

fn run_until_ended(w: &SessionWorker) -> Vec<WorkerEvent> {
    w.send(WorkerCmd::Run);
    wait_events(w, Duration::from_secs(8), |evs| {
        evs.iter().rev().any(|e| {
            matches!(
                e,
                WorkerEvent::Snapshot(s)
                    if matches!(s.status, SessionStatus::Ended { .. } | SessionStatus::Failed(_))
            )
        })
    })
}

fn badges_for(
    snap: &pipe_gui::worker::Snapshot,
    spec: &str,
    verb: &str,
) -> pipe_gui::preview::StageBadges {
    let id = runtime_id(spec, verb);
    let live = snap.live.iter().find(|l| l.id == id);
    let name = live.map(|l| l.name.as_str()).unwrap_or(verb);
    let ended = live.map(|l| l.ended).unwrap_or(false);
    stage_badges_on(snap, id, name, ended)
}

#[test]
fn locate_in_out_badges_after_run() {
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: LOCATE_SPEC.into(),
        cwd: std::env::temp_dir(),
    });
    let evs = run_until_ended(&w);
    let snap = last_snap(&evs);
    assert!(matches!(snap.status, SessionStatus::Ended { rc: 0 }));
    let locate = badges_for(snap, LOCATE_SPEC, "locate");
    assert_eq!(locate.received, 3, "locate in {:?}", snap.events);
    assert_eq!(locate.sent, 2, "locate out");
    assert_eq!(locate.in_tip, "Read 3 records in");
    assert_eq!(locate.out_tip, "Sent 2 records out");
    let first = badges_for(snap, LOCATE_SPEC, "literal");
    assert_eq!(first.received, 0, "source literal has no in badge");
    assert!(first.in_tip.is_empty());
    assert!(first.sent >= 1);
}

#[test]
fn locate_step_run_and_resume_share_in_out_counts() {
    let cwd = std::env::temp_dir();
    let run_w = SessionWorker::start();
    run_w.send(WorkerCmd::Prepare {
        spec: LOCATE_SPEC.into(),
        cwd: cwd.clone(),
    });
    let run_evs = run_until_ended(&run_w);
    let run_snap = last_snap(&run_evs);
    let run_loc = badges_for(run_snap, LOCATE_SPEC, "locate");

    let step_w = SessionWorker::start();
    step_w.send(WorkerCmd::Prepare {
        spec: LOCATE_SPEC.into(),
        cwd: cwd.clone(),
    });
    let locate = runtime_id(LOCATE_SPEC, "locate");
    for _ in 0..40 {
        let evs = step_until(&step_w, locate);
        if matches!(
            last_snap(&evs).status,
            SessionStatus::Ended { .. } | SessionStatus::Failed(_)
        ) {
            break;
        }
    }
    let step_evs = step_until(&step_w, locate);
    let step_snap = last_snap(&step_evs);
    let step_loc = badges_for(step_snap, LOCATE_SPEC, "locate");
    assert_eq!(step_loc.received, run_loc.received);
    assert_eq!(step_loc.sent, run_loc.sent);

    let resume_w = SessionWorker::start();
    resume_w.send(WorkerCmd::Prepare {
        spec: LOCATE_SPEC.into(),
        cwd,
    });
    let _ = step_until(&resume_w, locate);
    let mid_evs = step_until(&resume_w, locate);
    let mid = badges_for(last_snap(&mid_evs), LOCATE_SPEC, "locate");
    assert!(mid.received < run_loc.received || mid.sent < run_loc.sent || mid.received > 0);
    let resume_evs = run_until_ended(&resume_w);
    let resume_snap = last_snap(&resume_evs);
    let resume_loc = badges_for(resume_snap, LOCATE_SPEC, "locate");
    assert_eq!(
        resume_loc.received, run_loc.received,
        "Step then Run must not double reads"
    );
    assert_eq!(
        resume_loc.sent, run_loc.sent,
        "Step then Run must not double sends"
    );
    let resume_id = runtime_id(LOCATE_SPEC, "locate");
    let resume_hist = history_for(
        &resume_snap.events,
        RecordFocus::Stage(resume_id),
        &resume_snap.record_logs,
    );
    assert_eq!(
        resume_hist.received.len(),
        3,
        "3 received rows after resume"
    );
    assert_eq!(resume_hist.sent.len(), 2, "2 sent rows after resume");
}

fn stage_history<'a>(
    snap: &'a pipe_gui::worker::Snapshot,
    spec: &str,
    verb: &str,
) -> pipe_gui::preview::RecordHistory {
    let id = runtime_id(spec, verb);
    history_for(&snap.events, RecordFocus::Stage(id), &snap.record_logs)
}

#[test]
fn locate_grids_have_three_received_and_two_sent_after_run() {
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: LOCATE_SPEC.into(),
        cwd: std::env::temp_dir(),
    });
    let evs = run_until_ended(&w);
    let snap = last_snap(&evs);
    assert!(matches!(snap.status, SessionStatus::Ended { rc: 0 }));
    let hist = stage_history(snap, LOCATE_SPEC, "locate");
    assert_eq!(hist.received.len(), 3);
    assert_eq!(hist.sent.len(), 2);
    assert_eq!(hist.received_total, 3);
    assert_eq!(hist.sent_total, 2);
    let recv = records_table(&hist.received);
    let sent = records_table(&hist.sent);
    assert_eq!(recv.rows.len(), 3);
    assert_eq!(sent.rows.len(), 2);
    let recv_text: Vec<String> = recv
        .rows
        .iter()
        .map(|row| match &row[0] {
            MiniCell::Text(s) => s.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(recv_text, vec!["apricot", "banana", "apple"]);
    let edge = RecordFocus::Edge {
        writer: runtime_id(LOCATE_SPEC, "locate"),
        writer_stream: 0,
        reader: runtime_id(LOCATE_SPEC, "console"),
        reader_stream: 0,
    };
    let edge_hist = history_for(&snap.events, edge, &snap.record_logs);
    assert_eq!(
        edge_hist.sent.len(),
        2,
        "edge preview must keep both sent records, not 1"
    );
}

#[test]
fn locate_grids_match_after_step_then_run() {
    let cwd = std::env::temp_dir();
    let w = SessionWorker::start();
    w.send(WorkerCmd::Prepare {
        spec: LOCATE_SPEC.into(),
        cwd,
    });
    let locate = runtime_id(LOCATE_SPEC, "locate");
    let _ = step_until(&w, locate);
    let _ = step_until(&w, locate);
    let evs = run_until_ended(&w);
    let snap = last_snap(&evs);
    assert!(matches!(snap.status, SessionStatus::Ended { rc: 0 }));
    let hist = history_for(&snap.events, RecordFocus::Stage(locate), &snap.record_logs);
    assert_eq!(hist.received.len(), 3);
    assert_eq!(hist.sent.len(), 2);
}
