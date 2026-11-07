//! Headless debugger: topology without execution, and live record-step events.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use pipe_core::{CycleKind, DebugConfig, DebugEventKind, SessionStatus, StepOutcome};
use pipe_parser::parse_pipeline;
use pipe_stages::{debug_session, inspect_topology, run_spec, NodeKind, StageRegistry};

fn parse(spec: &str) -> pipe_parser::ParsedPipeline {
    parse_pipeline(spec).unwrap_or_else(|e| panic!("parse {spec:?}: {e}"))
}

fn session(spec: &str) -> pipe_core::DebugSession {
    debug_session(
        &parse(spec),
        &StageRegistry::default(),
        DebugConfig::default(),
    )
    .unwrap_or_else(|e| panic!("session {spec:?}: {e}"))
}

fn temp_file(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "pipe-debug-{}-{}-{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    p
}

fn write_lines(path: &std::path::Path, lines: &[&str]) {
    // Always LF so Windows runners do not inject CR into record bodies.
    let mut body = String::new();
    for line in lines {
        body.push_str(line);
        body.push('\n');
    }
    fs::write(path, body).unwrap();
}

fn path_spec(path: &std::path::Path) -> String {
    // Spec scanner treats whitespace as a token break. Temp paths on this
    // runner do not contain spaces; keep the operand unquoted.
    path.to_string_lossy().replace('\\', "/")
}

#[test]
fn inspect_does_not_create_output_files() {
    let out = temp_file("inspect-out.txt");
    let spec = format!("literal secret | > {}", path_spec(&out));
    let parsed = parse(&spec);
    let topo = inspect_topology(&parsed);
    assert!(
        topo.nodes.iter().any(|n| n.verb == ">"),
        "topology should show the file stage"
    );
    assert!(
        !out.exists(),
        "inspect_topology must not create {}",
        out.display()
    );
    let sess = debug_session(&parsed, &StageRegistry::default(), DebugConfig::default()).unwrap();
    assert_eq!(sess.status(), SessionStatus::Idle);
    assert!(sess.events().is_empty());
    assert!(
        !out.exists(),
        "debug_session construction must not create {}",
        out.display()
    );
    drop(sess);
    assert!(!out.exists());
}

#[test]
fn branched_label_topology_is_one_locate() {
    let spec = "(ENDCHAR ?) literal ax|literal by|literal az | L: locate /a/ | hole ? L: | hole";
    let topo = inspect_topology(&parse(spec));
    let locate: Vec<_> = topo
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.verb == "LOCATE")
        .collect();
    assert_eq!(locate.len(), 1, "one LOCATE node, not a copy per label");
    assert_eq!(locate[0].1.label.as_deref(), Some("L"));
    assert_eq!(locate[0].1.label_occurrence, 0);
    assert_eq!(locate[0].1.kind, NodeKind::Stage);
    assert!(!topo.nodes.iter().any(|n| n.verb == "COPY"));
    let refs: Vec<_> = topo
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::LabelReference)
        .collect();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].label.as_deref(), Some("L"));

    let loc_i = locate[0].0;
    let primary_out = topo
        .edges
        .iter()
        .filter(|e| e.writer == loc_i && e.writer_stream == 0)
        .count();
    let secondary_out = topo
        .edges
        .iter()
        .filter(|e| e.writer == loc_i && e.writer_stream == 1)
        .count();
    assert_eq!(primary_out, 1);
    assert_eq!(secondary_out, 1);
}

#[test]
fn locate_steps_include_discarded_miss() {
    let input = temp_file("locate-in.txt");
    write_lines(&input, &["apple", "banana", "apricot"]);
    let spec = format!("< {} | locate /ap/ | hole", path_spec(&input));
    let mut sess = session(&spec);
    let loc = sess.stage_by_name("LOCATE").expect("LOCATE");
    let mut kinds = Vec::new();
    for i in 0..8 {
        match sess.step_stage(loc) {
            StepOutcome::Cycle(c) => kinds.push(c),
            StepOutcome::Ended { .. } | StepOutcome::PipelineEnded { .. } => break,
            other => panic!("step {i}: {other:?}"),
        }
    }
    let quiet: Vec<_> = kinds
        .iter()
        .filter(|c| c.kind == CycleKind::ReadThenQuiet)
        .collect();
    assert!(
        quiet.iter().any(|c| c.writes_offered >= 1),
        "at least one hit wrote primary: {kinds:?}"
    );
    assert!(
        quiet
            .iter()
            .any(|c| c.writes_discarded >= 1 && c.writes_offered == 0),
        "banana must show as a discarded miss: {kinds:?}"
    );
    let events = sess.events();
    assert!(
        events.iter().any(|e| {
            e.stage == loc
                && matches!(
                    &e.kind,
                    DebugEventKind::WriteDiscard { stream, .. } if stream.0 == 1
                )
        }),
        "live events must include secondary discard, not a canned trace"
    );
    let _ = fs::remove_file(&input);
}

#[test]
fn sort_ingest_then_emit() {
    let input = temp_file("sort-in.txt");
    write_lines(&input, &["c", "a", "b"]);
    let spec = format!("< {} | sort | hole", path_spec(&input));
    let mut sess = session(&spec);
    let sort = sess.stage_by_name("SORT").expect("SORT");
    let mut ingest = 0;
    let mut emit = 0;
    let mut emitted = Vec::new();
    for i in 0..16 {
        match sess.step_stage(sort) {
            StepOutcome::Cycle(c) => match c.kind {
                CycleKind::BufferedInput | CycleKind::InputClosed => {
                    ingest += 1;
                    assert_eq!(c.writes_offered, 0, "ingest must not emit: {c:?}");
                    let slots = sess.slots();
                    let sort_out = slots.iter().find(|s| s.writer == sort);
                    if let Some(slot) = sort_out {
                        assert!(
                            slot.record.is_none(),
                            "SORT output slot must stay empty during ingest"
                        );
                    }
                }
                CycleKind::EmittedOutput | CycleKind::ProducedOutput => {
                    emit += 1;
                    if let Some(ev) = sess.events().iter().rev().find(|e| {
                        e.stage == sort && matches!(e.kind, DebugEventKind::WriteOffered { .. })
                    }) {
                        if let DebugEventKind::WriteOffered { record, .. } = &ev.kind {
                            emitted.push(record.bytes.clone());
                        }
                    }
                }
                other => panic!("step {i}: unexpected {other:?} {c:?}"),
            },
            StepOutcome::Ended { .. } | StepOutcome::PipelineEnded { .. } => break,
            other => panic!("step {i}: {other:?}"),
        }
    }
    assert!(ingest >= 3, "expected three ingest reads, got {ingest}");
    assert!(emit >= 3, "expected three sorted emits, got {emit}");
    assert_eq!(emitted.first().map(|b| b.as_slice()), Some(b"a".as_slice()));
    let _ = fs::remove_file(&input);
}

#[test]
fn fanout_one_step_offers_both_outputs() {
    let spec = "(ENDCHAR ?) literal a|literal b | f: fanout | hole ? f: | hole";
    let mut sess = session(spec);
    let fan = sess.stage_by_name("FANOUT").expect("FANOUT");
    let mut saw_two = false;
    for i in 0..12 {
        match sess.step_stage(fan) {
            StepOutcome::Cycle(c) => {
                if c.kind == CycleKind::ReadThenQuiet && c.writes_offered >= 2 {
                    saw_two = true;
                    let slots = sess.slots();
                    let outs: Vec<_> = slots.iter().filter(|s| s.writer == fan).collect();
                    assert_eq!(outs.len(), 2, "FANOUT should have two output edges");
                    break;
                }
            }
            StepOutcome::Ended { .. } | StepOutcome::PipelineEnded { .. } => break,
            other => panic!("step {i}: {other:?}"),
        }
    }
    assert!(
        saw_two,
        "one FANOUT step must offer both outputs: {:?}",
        sess.events()
    );
}

#[test]
fn take_short_stops_the_producer() {
    let input = temp_file("take-in.txt");
    write_lines(&input, &["a", "b", "c", "d"]);
    let spec = format!("< {} | take 2 | hole", path_spec(&input));
    let mut sess = session(&spec);
    let take = sess.stage_by_name("TAKE").expect("TAKE");
    let mut saw_short = false;
    for i in 0..12 {
        match sess.step_stage(take) {
            StepOutcome::Cycle(c) => {
                if c.saw_short {
                    saw_short = true;
                }
            }
            StepOutcome::Ended { .. } | StepOutcome::PipelineEnded { .. } => break,
            other => panic!("step {i}: {other:?}"),
        }
    }
    assert!(
        saw_short,
        "TAKE 2 must short the producer: {:?}",
        sess.events()
    );
    assert!(
        sess.events().iter().any(|e| matches!(
            e.kind,
            DebugEventKind::ShortInput { .. } | DebugEventKind::Short { .. }
        )),
        "SHORT must appear in live events"
    );
    let _ = fs::remove_file(&input);
}

#[test]
fn pause_keeps_bounded_history() {
    let input = temp_file("hist-in.txt");
    let lines: Vec<String> = (0..80).map(|i| format!("r{i}")).collect();
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    write_lines(&input, &line_refs);
    let spec = format!("< {} | hole", path_spec(&input));
    let mut sess = debug_session(
        &parse(&spec),
        &StageRegistry::default(),
        DebugConfig::bounded(12, 64),
    )
    .unwrap();
    let hole = sess.stage_by_name("HOLE").expect("HOLE");
    for _ in 0..40 {
        match sess.step_stage(hole) {
            StepOutcome::Cycle(_) => {}
            StepOutcome::Ended { .. } | StepOutcome::PipelineEnded { .. } => break,
            StepOutcome::NoProgress { .. } => break,
            other => panic!("{other:?}"),
        }
    }
    let n = sess.events().len();
    assert!(n <= 12, "event ring must cap at 12, got {n}");
    assert!(n > 0);
    sess.request_pause();
    assert!(sess.events().len() <= 12);
    assert!(matches!(
        sess.status(),
        SessionStatus::Paused | SessionStatus::Idle | SessionStatus::Running
    ));
    let _ = fs::remove_file(&input);
}

#[test]
fn stop_drains_and_does_not_keep_writing() {
    let input = temp_file("stop-in.txt");
    let output = temp_file("stop-out.txt");
    write_lines(&input, &["one", "two", "three", "four", "five"]);
    let spec = format!("< {} | > {}", path_spec(&input), path_spec(&output));
    let mut sess = session(&spec);
    let writer = sess.stage_by_name(">").expect(">");
    let first = sess.step_stage(writer);
    match first {
        StepOutcome::Cycle(_) | StepOutcome::Ended { .. } | StepOutcome::PipelineEnded { .. } => {}
        other => panic!("first step: {other:?}"),
    }
    assert!(output.exists(), "first record should have reached the file");
    let after_step = fs::read(&output).unwrap_or_default();
    let stopped = sess.stop();
    assert!(
        matches!(stopped, SessionStatus::Stopped { .. }),
        "{stopped:?}"
    );
    let after_stop = fs::read(&output).unwrap_or_default();
    assert_eq!(after_step, after_stop, "stop must not continue writing");
    let full = run_spec(&spec);
    assert!(full.is_ok());
    let complete = fs::read(&output).unwrap();
    assert!(
        complete.len() > after_stop.len(),
        "a full run writes more than the stopped session"
    );
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&output);
}

#[test]
fn ordinary_run_spec_without_debug_still_works() {
    let out = run_spec("literal hello| hole");
    assert!(out.is_ok(), "{out:?}");
}

#[test]
fn inspect_unknown_verb_does_not_instantiate() {
    let out = temp_file("unknown-out.txt");
    let spec = format!("notastage | > {}", path_spec(&out));
    let parsed = parse(&spec);
    let topo = inspect_topology(&parsed);
    assert!(topo.nodes.iter().any(|n| n.verb == "NOTASTAGE"));
    assert!(!out.exists());
    let err = debug_session(&parsed, &StageRegistry::default(), DebugConfig::default());
    assert!(err.is_err(), "unknown verb must fail at session build");
    assert!(!out.exists());
}

#[test]
fn runpipe_topology_stays_opaque() {
    // Nested PIPE text is a record, not a parsed subgraph.
    let spec = "literal PIPE literal hi|hole | runpipe";
    let topo = inspect_topology(&parse(spec));
    assert_eq!(topo.nodes.iter().filter(|n| n.verb == "LITERAL").count(), 1);
    assert_eq!(topo.nodes.iter().filter(|n| n.verb == "RUNPIPE").count(), 1);
    assert_eq!(topo.nodes.iter().filter(|n| n.verb == "HOLE").count(), 1);
}

#[test]
fn preface_callpipe_is_blocked_not_one_record() {
    let mut sess = session("literal hello | preface literal PREFIX | hole");
    let pref = sess.stage_by_name("PREFACE").expect("PREFACE");
    let mut children = None;
    for i in 0..8 {
        match sess.step_stage(pref) {
            StepOutcome::BlockedOnCallPipe { children: c } => {
                children = Some(c);
                break;
            }
            StepOutcome::Cycle(_) | StepOutcome::NoProgress { .. } => {}
            StepOutcome::Ended { .. } | StepOutcome::PipelineEnded { .. } => break,
            other => panic!("step {i}: {other:?}"),
        }
    }
    let children =
        children.expect("PREFACE must report BlockedOnCallPipe, not a fake record cycle");
    assert!(
        !children.is_empty(),
        "nested CALLPIPE stages must be listed: {:?}",
        sess.live_stages()
    );
    let live = sess.live_stages();
    assert!(
        live.iter().any(|s| s.waiting_callpipe),
        "caller should still be waiting on CALLPIPE: {live:?}"
    );
    assert!(
        children.iter().any(|id| live.iter().any(|s| s.id == *id)),
        "listed children must appear in live_stages: children={children:?} live={live:?}"
    );
}

#[test]
fn debug_run_to_end_matches_hole_drain() {
    let mut sess = session("literal hello| hole");
    match sess.run() {
        SessionStatus::Ended { rc } => assert_eq!(rc, 0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn poll_then_stop_does_not_run_to_completion() {
    let input = temp_file("poll-in.txt");
    let output = temp_file("poll-out.txt");
    write_lines(&input, &["one", "two", "three", "four", "five"]);
    let spec = format!("< {} | > {}", path_spec(&input), path_spec(&output));
    let mut sess = session(&spec);
    for _ in 0..4 {
        let st = sess.poll();
        if st.is_terminal() {
            break;
        }
    }
    let mid = fs::read(&output).unwrap_or_default();
    let stopped = sess.stop();
    assert!(
        matches!(stopped, SessionStatus::Stopped { .. }),
        "{stopped:?}"
    );
    let after = fs::read(&output).unwrap_or_default();
    assert_eq!(mid, after, "stop after poll must not keep writing");
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&output);
}
