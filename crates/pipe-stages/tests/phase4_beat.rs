//! BEAT — Author's Edition 1.1.11 pp.271–272
//! (example p.272; See Also delay / gate / synchronise).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

fn emit(records: &[&str]) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<String> = records.iter().map(|s| (*s).to_string()).collect();
    Box::new(move |ctx| {
        for rec in records {
            if ctx.write(Record::from(rec)).is_err() {
                break;
            }
        }
    })
}

fn emit_after(wait: Duration, records: &[&str]) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<String> = records.iter().map(|s| (*s).to_string()).collect();
    Box::new(move |ctx| {
        if !wait.is_zero() {
            let _ = ctx.wait_until(Instant::now() + wait);
        }
        for rec in records {
            if ctx.write(Record::from(rec)).is_err() {
                break;
            }
        }
    })
}

fn sink(out: Rc<RefCell<Vec<String>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            out.borrow_mut().push(r.to_string_lossy());
        }
    })
}

fn run_spec_collect(spec: &str) -> Vec<String> {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let sink_out = Rc::clone(&sink_out);
        move |_| pipe_stages::collect_to(Rc::clone(&sink_out))
    });
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn run_beat(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    run_beat_from(emit(records), args)
}

fn run_beat_from(
    src_fn: Box<dyn FnOnce(&mut StageCtx)>,
    args: &[&str],
) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", src_fn);
    let beat = b.add_stage(
        "BEAT",
        reg.create("BEAT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("BEAT"),
    );
    let snk_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    let snk_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
    b.connect(src, StreamId::PRIMARY, beat, StreamId::PRIMARY);
    b.connect(beat, StreamId::PRIMARY, snk_p, StreamId::PRIMARY);
    b.connect(beat, StreamId::SECONDARY, snk_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

#[test]
fn copies_ready_records_without_waiting() {
    // Records already waiting are copied; EOF ends before a timeout.
    let start = Instant::now();
    let (p, s) = run_beat(&["a", "b"], &["10"]);
    assert_eq!(p, vec!["a", "b"]);
    assert!(
        s.is_empty(),
        "no heartbeat when records arrive immediately: {s:?}"
    );
    assert!(
        start.elapsed() < Duration::from_millis(200),
        "elapsed {:?}",
        start.elapsed()
    );
}

#[test]
fn default_marker_is_a_null_record() {
    // p.271: the default string is a null string.
    let (p, s) = run_beat_from(emit_after(Duration::from_millis(25), &[]), &["0.01"]);
    assert!(p.is_empty());
    assert!(
        !s.is_empty() && s.iter().all(|r| r.is_empty()),
        "default marker is a null record; got {s:?}"
    );
}

#[test]
fn dstring_marker_on_secondary() {
    let (p, s) = run_beat_from(
        emit_after(Duration::from_millis(25), &[]),
        &["0.01", "/tick/"],
    );
    assert!(p.is_empty());
    assert!(s.iter().all(|r| r == "tick") && !s.is_empty(), "got {s:?}");
}

#[test]
fn hex_marker() {
    let (_, s) = run_beat_from(
        emit_after(Duration::from_millis(25), &[]),
        &["0.01", "x4142"],
    );
    assert!(s.iter().all(|r| r == "AB") && !s.is_empty(), "got {s:?}");
}

#[test]
fn copies_then_heartbeats_when_quiet() {
    let start = Instant::now();
    let (p, s) = run_beat_from(
        emit_after(Duration::from_millis(40), &["late"]),
        &["0.015", "/tick/"],
    );
    assert_eq!(p, vec!["late"]);
    assert!(
        !s.is_empty() && s.iter().all(|r| r == "tick"),
        "quiet gap must write the marker; got {s:?}"
    );
    assert!(
        start.elapsed() >= Duration::from_millis(25),
        "elapsed {:?}",
        start.elapsed()
    );
}

#[test]
fn once_writes_one_marker_per_input_record() {
    // p.271: ONCE — a record can be written to secondary once per input record.
    let (p, s) = run_beat_from(
        emit_after(Duration::from_millis(45), &["late"]),
        &["ONCE", "0.012", "/tick/"],
    );
    assert_eq!(p, vec!["late"]);
    assert_eq!(
        s,
        vec!["tick"],
        "ONCE must not keep ticking while waiting: {s:?}"
    );
}

#[test]
fn omitted_once_restarts_after_marker_is_consumed() {
    // p.271: without ONCE, another timeout starts after the marker is consumed.
    let (p, s) = run_beat_from(
        emit_after(Duration::from_millis(50), &[]),
        &["0.012", "/tick/"],
    );
    assert!(p.is_empty());
    assert!(
        s.len() >= 2,
        "repeated timeout must write more than one marker; got {s:?}"
    );
    assert!(s.iter().all(|r| r == "tick"), "{s:?}");
}

#[test]
fn arriving_record_is_not_held_for_the_rest_of_the_interval() {
    // Strictly does not delay the copied record (p.271).
    let start = Instant::now();
    let (p, _) = run_beat_from(emit_after(Duration::from_millis(15), &["late"]), &["0.12"]);
    assert_eq!(p, vec!["late"]);
    assert!(
        start.elapsed() < Duration::from_millis(80),
        "must copy when the record arrives, not after 120ms; elapsed {:?}",
        start.elapsed()
    );
}

#[test]
fn eof_does_not_write_a_trailing_heartbeat() {
    // End-of-file is not a quiet live source (Note 1: async host interface).
    let (p, s) = run_beat(&["a"], &["0.02", "/tick/"]);
    assert_eq!(p, vec!["a"]);
    assert!(s.is_empty(), "EOF after a copy is not a timeout: {s:?}");
}

#[test]
fn unconnected_secondary_terminates_on_timeout() {
    // p.272: `? b:` defines secondary but does not connect it; writing the
    // null marker then terminates, so later records are not copied.
    let events = Rc::new(RefCell::new(Vec::new()));
    let prod_log = Rc::clone(&events);
    let mut b = PipelineBuilder::new();
    let reg = StageRegistry::default();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx| {
            let _ = ctx.write(Record::from("a"));
            prod_log.borrow_mut().push("wrote-a".into());
            let _ = ctx.wait_until(Instant::now() + Duration::from_millis(30));
            let wrote = ctx.write(Record::from("b"));
            prod_log.borrow_mut().push(format!("wrote-b:{wrote:?}"));
        }),
    );
    let beat = b.add_stage("BEAT", reg.create("BEAT", vec!["0.012".into()]).unwrap());
    let out = Rc::new(RefCell::new(Vec::new()));
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, beat);
    b.connect_primary(beat, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a".to_string()]);
}

#[test]
fn spec_unconnected_secondary_example() {
    // p.272: `(end ?) ... | b: beat 1 | ... ? b:`
    let start = Instant::now();
    assert_eq!(
        run_spec_collect("(ENDCHAR ?) literal a|literal b| B: beat 10 | >>COLLECT ? B:"),
        vec!["b", "a"]
    );
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[test]
fn premature_termination_when_primary_output_not_connected() {
    let start = Instant::now();
    run_spec("literal a | beat 5").unwrap();
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[test]
fn secondary_input_rejected() {
    let mut b = PipelineBuilder::new();
    let reg = StageRegistry::default();
    let src_p = b.add_stage("src_p", emit(&["a"]));
    let src_s = b.add_stage("src_s", emit(&["x"]));
    let beat = b.add_stage("BEAT", reg.create("BEAT", vec!["1".into()]).unwrap());
    let snk_p = b.add_stage("sink_p", sink(Rc::new(RefCell::new(Vec::new()))));
    let snk_s = b.add_stage("sink_s", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect(src_p, StreamId::PRIMARY, beat, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, beat, StreamId::SECONDARY);
    b.connect(beat, StreamId::PRIMARY, snk_p, StreamId::PRIMARY);
    b.connect(beat, StreamId::SECONDARY, snk_s, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .contains("no other input stream may be connected"),
        "{err}"
    );
}

#[test]
fn wait_does_not_freeze_a_sibling_pipeline() {
    let fast_at = Rc::new(RefCell::new(None));
    let slow_at = Rc::new(RefCell::new(None));
    let fast_sink = Rc::clone(&fast_at);
    let slow_sink = Rc::clone(&slow_at);
    let mut b = PipelineBuilder::new();
    let slow_src = b.add_stage("slow_src", emit_after(Duration::from_millis(40), &["slow"]));
    let beat = b.add_stage(
        "BEAT",
        StageRegistry::default()
            .create("BEAT", vec!["0.04".into()])
            .unwrap(),
    );
    let slow_out = b.add_stage(
        "slow_out",
        Box::new(move |ctx| {
            while let Ok(Some(_)) = ctx.read() {
                *slow_sink.borrow_mut() = Some(Instant::now());
            }
        }),
    );
    let fast_src = b.add_stage("fast_src", emit(&["fast"]));
    let fast_out = b.add_stage(
        "fast_out",
        Box::new(move |ctx| {
            while let Ok(Some(_)) = ctx.read() {
                *fast_sink.borrow_mut() = Some(Instant::now());
            }
        }),
    );
    let ticks = b.add_stage("ticks", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect_primary(slow_src, beat);
    b.connect_primary(beat, slow_out);
    b.connect(beat, StreamId::SECONDARY, ticks, StreamId::PRIMARY);
    b.connect_primary(fast_src, fast_out);
    b.run().unwrap();
    let fast = fast_at.borrow().expect("fast");
    let slow = slow_at.borrow().expect("slow");
    assert!(
        slow.saturating_duration_since(fast) >= Duration::from_millis(15),
        "sibling must finish while BEAT waits; lag={:?}",
        slow.saturating_duration_since(fast)
    );
}

#[test]
fn peek_write_read_does_not_delay_the_record() {
    // p.271 strictly does not delay: peek, write, then consume. A slow
    // consumer keeps the output slot full, so the third producer write
    // cannot proceed until the first output record is taken (COPY would
    // have consumed the second record already and let the third in).
    let events = Rc::new(RefCell::new(Vec::new()));
    let prod_log = Rc::clone(&events);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx| {
            let _ = ctx.write(Record::from("a"));
            prod_log.borrow_mut().push("wrote1".into());
            let _ = ctx.write(Record::from("b"));
            prod_log.borrow_mut().push("wrote2".into());
            let _ = ctx.write(Record::from("c"));
            prod_log.borrow_mut().push("wrote3".into());
        }),
    );
    let beat = b.add_stage(
        "BEAT",
        StageRegistry::default()
            .create("BEAT", vec!["10".into()])
            .unwrap(),
    );
    let snk_log = Rc::clone(&events);
    let snk = b.add_stage(
        "sink",
        Box::new(move |ctx| {
            let _ = ctx.wait_until(Instant::now() + Duration::from_millis(25));
            while let Ok(Some(r)) = ctx.read() {
                snk_log
                    .borrow_mut()
                    .push(format!("got:{}", r.to_string_lossy()));
            }
        }),
    );
    let ticks = b.add_stage("ticks", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect_primary(src, beat);
    b.connect_primary(beat, snk);
    b.connect(beat, StreamId::SECONDARY, ticks, StreamId::PRIMARY);
    b.run().unwrap();
    let got = events.borrow().clone();
    let w3 = got.iter().position(|e| e == "wrote3").unwrap();
    let g1 = got.iter().position(|e| e == "got:a").unwrap();
    assert!(
        g1 < w3,
        "third write must wait until the first output record is taken; {got:?}"
    );
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | beat | hole",
        "literal a | beat forever | hole",
        "literal a | beat 1 once | hole",
        "literal a | beat 1 tick | hole",
        "literal a | beat +1 | hole",
        "literal a | beat 1:30 | hole",
        "literal a | beat onc 1 | hole",
        "literal a | beat 1.0000000 | hole",
        "literal a | bea 1 | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        let t = err.to_string();
        assert!(
            t.contains("BEAT")
                || t.contains("unrecognized operand")
                || t.contains("invalid interval")
                || t.contains("an interval")
                || t.contains("unknown stage"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn invented_abbreviation_rejected() {
    // Diagram is BEAT (all caps); bea is not an abbreviation.
    let err = run_spec("literal a | bea 1 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("BEA"),
        "{err}"
    );
}

#[test]
fn once_is_a_whole_word() {
    let err = run_spec("literal a | beat onc 1 | hole").unwrap_err();
    assert!(
        err.to_string().contains("invalid interval") || err.to_string().contains("unrecognized"),
        "{err}"
    );
}

#[test]
fn missing_interval_rejected() {
    let err = run_spec("literal a | beat once | hole").unwrap_err();
    assert!(err.to_string().contains("interval"), "{err}");
}

#[test]
fn fractional_seconds_in_a_spec() {
    assert_eq!(
        run_spec_collect("(ENDCHAR ?) literal hi| B: beat 0.02 /tick/ | >>COLLECT ? B: | hole"),
        vec!["hi"]
    );
}
