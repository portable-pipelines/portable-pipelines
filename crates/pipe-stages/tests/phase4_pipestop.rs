//! PIPESTOP — Author's Edition 1.1.11 pp.458–459
//! (tutorial p.150; DELAY p.322; GATE example p.459).

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

#[test]
fn copies_the_record_through() {
    // p.459: passes the input to the output.
    assert_eq!(
        run_spec_collect("literal hello| pipestop | >>COLLECT"),
        vec!["hello"]
    );
}

#[test]
fn copies_several_records_without_delaying() {
    assert_eq!(
        run_spec_collect("literal a|literal b|literal c| pipestop | >>COLLECT"),
        vec!["c", "b", "a"]
    );
}

#[test]
fn null_record_is_an_ordinary_trigger() {
    assert_eq!(run_spec_collect("literal | pipestop | >>COLLECT"), vec![""]);
}

#[test]
fn last_stage_does_not_write() {
    // p.459 example: `literal +60 | delay | pipestop` is last.
    run_spec("literal go | pipestop").unwrap();
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | pipestop forever | hole",
        "literal a | pipestop strict | hole",
        "literal a | pipestop stop | hole",
        "literal a | pipestop alleof | hole",
        "literal a | pipestop 1 | hole",
        "literal a | pipestop +60 | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        assert!(
            err.to_string().contains("unrecognized operand"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn invented_abbreviation_rejected() {
    // Diagram is PIPESTOP (all caps); pipesto is not an abbreviation.
    let err = run_spec("literal a | pipesto | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("PIPESTO"),
        "{err}"
    );
    assert!(StageRegistry::default().contains("PIPESTOP"));
    assert!(!StageRegistry::default().contains("PIPESTO"));
    assert!(!StageRegistry::default().contains("PIPES"));
}

#[test]
fn delay_wait_is_interrupted() {
    // p.322 / p.150: DELAY stops if a record is passed to PIPESTOP.
    // p.459: `… ? literal +60 | delay | pipestop` — sibling posts ECBs.
    let start = Instant::now();
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal +5 | delay | hole ? literal go| pipestop | >>COLLECT"
        ),
        vec!["go"]
    );
    assert!(
        start.elapsed() < Duration::from_millis(400),
        "DELAY +5 must not run to completion; elapsed {:?}",
        start.elapsed()
    );
}

#[test]
fn delay_does_not_write_the_waited_record() {
    // The in-flight DELAY record is not copied: the wait did not expire.
    let start = Instant::now();
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal +5 late | delay | >>COLLECT ? literal go | pipestop | hole"
        ),
        Vec::<String>::new()
    );
    assert!(start.elapsed() < Duration::from_millis(400));
}

#[test]
fn beat_wait_is_interrupted() {
    // BEAT parks with wait_until_or_input (p.271); PIPESTOP posts that wait.
    let start = Instant::now();
    run_spec("(ENDCHAR ?) literal +5 | delay | B: beat 5 | hole ? B: | hole ? literal go | pipestop | hole")
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_millis(400),
        "BEAT 5 / DELAY +5 must not run to completion; elapsed {:?}",
        start.elapsed()
    );
}

#[test]
fn further_waiting_is_forbidden() {
    // p.458: the indication is irreversible. DUP * would otherwise keep
    // feeding DELAY after the first wait is posted.
    let start = Instant::now();
    run_spec("(ENDCHAR ?) literal +5 | dup * | delay | hole ? literal go | pipestop | hole")
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_millis(400),
        "subsequent DELAY waits must not run; elapsed {:?}",
        start.elapsed()
    );
}

#[test]
fn does_not_terminate_a_running_pipeline() {
    // p.150: only waiting stages stop; a running copy is not terminated.
    assert_eq!(
        run_spec_collect("(ENDCHAR ?) literal a| copy | >>COLLECT ? literal go| pipestop | hole"),
        vec!["a"]
    );
}

#[test]
fn book_timer_then_pipestop() {
    // p.459: `literal +60 | delay | pipestop` with a zero interval.
    let start = Instant::now();
    run_spec("(ENDCHAR ?) literal kept | copy | hole ? literal +0 | delay | pipestop").unwrap();
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[test]
fn extra_input_rejected() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let reg = StageRegistry::default();
    let src_p = b.add_stage("src_p", emit(&["a"]));
    let src_s = b.add_stage("src_s", emit(&["b"]));
    let stop = b.add_stage("PIPESTOP", reg.create("PIPESTOP", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&p)));
    b.connect(src_p, StreamId::PRIMARY, stop, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stop, StreamId::SECONDARY);
    b.connect(stop, StreamId::PRIMARY, snk, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("only the primary input"), "{err}");
}

#[test]
fn peek_write_read_does_not_delay_the_record() {
    // p.459 strictly does not delay: consume after write.
    let events = Rc::new(RefCell::new(Vec::new()));
    let prod_log = Rc::clone(&events);
    let mut b = PipelineBuilder::new();
    let reg = StageRegistry::default();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx| {
            let _ = ctx.write(Record::from("one"));
            prod_log.borrow_mut().push("wrote1".into());
            let _ = ctx.write(Record::from("two"));
            prod_log.borrow_mut().push("wrote2".into());
        }),
    );
    let stop = b.add_stage("PIPESTOP", reg.create("PIPESTOP", vec![]).unwrap());
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
    b.connect_primary(src, stop);
    b.connect_primary(stop, snk);
    b.run().unwrap();
    let got = events.borrow().clone();
    let w1 = got.iter().position(|e| e == "wrote1").unwrap();
    let g1 = got.iter().position(|e| e == "got:one").unwrap();
    let w2 = got.iter().position(|e| e == "wrote2").unwrap();
    let g2 = got.iter().position(|e| e == "got:two").unwrap();
    assert!(
        w1 < g1 && g1 < w2 && w2 < g2,
        "second write must wait until the first record is consumed; {got:?}"
    );
}
