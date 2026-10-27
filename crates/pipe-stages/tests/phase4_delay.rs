//! DELAY — Author's Edition 1.1.11 pp.321–322
//! (tutorial Ch.13 pp.148–150; PIPMOD/PIPESTOP p.150).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, with_civil_now_since_midnight, StageRegistry};

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

fn run_records(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let delay = b.add_stage(
        "DELAY",
        reg.create(
            "DELAY",
            args.iter().map(|a| (*a).to_string()).collect(),
        )
        .expect("DELAY"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src, StreamId::PRIMARY, delay, StreamId::PRIMARY);
    b.connect(delay, StreamId::PRIMARY, snk, StreamId::PRIMARY);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn zero_interval_copies_immediately() {
    // +0 is a relative interval of zero (p.321).
    assert_eq!(run_spec_collect("literal +0| delay | >>COLLECT"), vec!["+0"]);
}

#[test]
fn note_3_literal_zero_never_waits() {
    // p.322 Note 3: literal 0 | delay never waits.
    let start = Instant::now();
    assert_eq!(run_spec_collect("literal 0| delay | >>COLLECT"), vec!["0"]);
    assert!(
        start.elapsed() < Duration::from_millis(200),
        "0 is a time of day already past (except exactly at midnight)"
    );
}

#[test]
fn copies_whole_record_ignoring_rest() {
    // p.321: remainder of the record is not inspected; the input is copied.
    assert_eq!(
        run_spec_collect("literal +0 hello world| delay | >>COLLECT"),
        vec!["+0 hello world"]
    );
}

#[test]
fn tiny_interval_waits() {
    let start = Instant::now();
    assert_eq!(
        run_spec_collect("literal +0.02| delay | >>COLLECT"),
        vec!["+0.02"]
    );
    assert!(
        start.elapsed() >= Duration::from_millis(10),
        "elapsed {:?}",
        start.elapsed()
    );
}

#[test]
fn records_keep_order_when_later_interval_is_shorter() {
    // p.321: relative order from one filter is unchanged (no reordering).
    let start = Instant::now();
    assert_eq!(
        run_records(&["+0.02 first", "+0 second"], &[]),
        vec!["+0.02 first".to_string(), "+0 second".to_string()]
    );
    assert!(start.elapsed() >= Duration::from_millis(10));
}

#[test]
fn past_tod_copies_immediately() {
    // p.321: a time of day already reached is copied when read.
    with_civil_now_since_midnight(Duration::from_secs(15 * 3600), || {
        assert_eq!(run_records(&["3 already past"], &[]), vec!["3 already past"]);
        assert_eq!(run_records(&["0"], &[]), vec!["0"]);
    });
}

#[test]
fn future_tod_waits_until_civil_time() {
    // 1:00:00.025 is 25 ms after a pinned 1:00:00 civil clock.
    with_civil_now_since_midnight(Duration::from_secs(3600), || {
        let start = Instant::now();
        assert_eq!(
            run_records(&["1:00:00.025"], &[]),
            vec!["1:00:00.025"]
        );
        assert!(
            start.elapsed() >= Duration::from_millis(10),
            "elapsed {:?}",
            start.elapsed()
        );
    });
}

#[test]
fn twenty_four_is_next_midnight() {
    // Note 3: use 24 to wait until the next midnight.
    with_civil_now_since_midnight(
        Duration::from_secs(24 * 3600) - Duration::from_millis(25),
        || {
            let start = Instant::now();
            assert_eq!(run_records(&["24"], &[]), vec!["24"]);
            assert!(
                start.elapsed() >= Duration::from_millis(10),
                "elapsed {:?}",
                start.elapsed()
            );
        },
    );
}

#[test]
fn null_record_is_missing_time_word() {
    let err = run_spec("literal | delay | hole").unwrap_err();
    assert!(
        err.to_string().contains("missing time word"),
        "{err}"
    );
}

#[test]
fn invalid_time_word_is_rejected() {
    let err = run_spec("literal abc| delay | hole").unwrap_err();
    assert!(
        err.to_string().contains("invalid time word"),
        "{err}"
    );
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal +0| delay forever | hole",
        "literal +0| delay +60 | hole",
        "literal +0| delay 1 | hole",
        "literal +0| delay strict | hole",
        "literal +0| delay stop | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        assert!(
            err.to_string().contains("unrecognized operand"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn must_not_be_first_stage() {
    // p.321 Placement.
    let err = run_spec("delay | hole").unwrap_err();
    assert!(
        err.to_string().contains("must not be a first stage"),
        "{err}"
    );
}

#[test]
fn invented_abbreviation_rejected() {
    // Diagram is DELAY (all caps); dela is not an abbreviation.
    let err = run_spec("literal +0| dela | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("DELA"),
        "{err}"
    );
}

#[test]
fn premature_termination_when_output_not_connected() {
    // p.321: terminates when the output stream is not connected (no wait).
    let start = Instant::now();
    run_spec("literal +5| delay").unwrap();
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[test]
fn extra_input_rejected() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let reg = StageRegistry::default();
    let src_p = b.add_stage("src_p", emit(&["+0"]));
    let src_s = b.add_stage("src_s", emit(&["+0"]));
    let delay = b.add_stage("DELAY", reg.create("DELAY", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&p)));
    b.connect(src_p, StreamId::PRIMARY, delay, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, delay, StreamId::SECONDARY);
    b.connect(delay, StreamId::PRIMARY, snk, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string().contains("only the primary input"),
        "{err}"
    );
}

#[test]
fn wait_does_not_freeze_a_sibling_pipeline() {
    // Dispatcher sleeps only while this stage is suspended.
    let fast_at = Rc::new(RefCell::new(None));
    let slow_at = Rc::new(RefCell::new(None));
    let fast_sink = Rc::clone(&fast_at);
    let slow_sink = Rc::clone(&slow_at);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let slow_src = b.add_stage("slow_src", emit(&["+0.04"]));
    let delay = b.add_stage("DELAY", reg.create("DELAY", vec![]).unwrap());
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
    b.connect_primary(slow_src, delay);
    b.connect_primary(delay, slow_out);
    b.connect_primary(fast_src, fast_out);
    b.run().unwrap();
    let fast = fast_at.borrow().expect("fast");
    let slow = slow_at.borrow().expect("slow");
    assert!(
        slow.saturating_duration_since(fast) >= Duration::from_millis(15),
        "sibling must finish while DELAY waits; lag={:?}",
        slow.saturating_duration_since(fast)
    );
}

#[test]
fn peek_write_read_does_not_delay_the_record() {
    // p.321 strictly does not delay: the producer cannot put the next record
    // until DELAY has written the current one (slot held during the wait).
    let events = Rc::new(RefCell::new(Vec::new()));
    let prod_log = Rc::clone(&events);
    let mut b = PipelineBuilder::new();
    let reg = StageRegistry::default();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx| {
            let _ = ctx.write(Record::from("+0.02"));
            prod_log.borrow_mut().push("wrote1".into());
            let _ = ctx.write(Record::from("+0"));
            prod_log.borrow_mut().push("wrote2".into());
        }),
    );
    let delay = b.add_stage("DELAY", reg.create("DELAY", vec![]).unwrap());
    let snk_log = Rc::clone(&events);
    let snk = b.add_stage(
        "sink",
        Box::new(move |ctx| {
            while let Ok(Some(r)) = ctx.read() {
                snk_log
                    .borrow_mut()
                    .push(format!("got:{}", r.to_string_lossy()));
            }
        }),
    );
    b.connect_primary(src, delay);
    b.connect_primary(delay, snk);
    b.run().unwrap();
    let got = events.borrow().clone();
    let w1 = got.iter().position(|e| e == "wrote1").unwrap();
    let g1 = got.iter().position(|e| e == "got:+0.02").unwrap();
    let w2 = got.iter().position(|e| e == "wrote2").unwrap();
    let g2 = got.iter().position(|e| e == "got:+0").unwrap();
    assert!(
        w1 < g1 && g1 < w2 && w2 < g2,
        "second write must wait until the first record is consumed; {got:?}"
    );
}
