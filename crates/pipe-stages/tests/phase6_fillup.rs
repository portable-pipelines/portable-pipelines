//! FILLUP — Author's Edition 1.1.11 p.345.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId, TraceConfig};
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

fn run_collect(spec: &str) -> Vec<String> {
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
fn primary_only_copies_through() {
    assert_eq!(
        run_collect("literal c|literal b|literal a| fillup | >>COLLECT"),
        vec!["a", "b", "c"]
    );
}

#[test]
fn switches_when_primary_fills() {
    // TAKE 1 severs FILLUP's primary output; remaining records go to secondary.
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c"]));
    let fill = b.add_stage("FILLUP", reg.create("FILLUP", vec![]).unwrap());
    let take = b.add_stage("TAKE", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fill);
    b.connect(fill, StreamId::PRIMARY, take, StreamId::PRIMARY);
    b.connect_primary(take, sink_p);
    b.connect(fill, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a"]);
    assert_eq!(*s.borrow(), vec!["b", "c"]);
}

#[test]
fn three_outputs_fill_in_order() {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c", "d"]));
    let fill = b.add_stage("FILLUP", reg.create("FILLUP", vec![]).unwrap());
    let take_p = b.add_stage("take_p", reg.create("TAKE", vec!["1".into()]).unwrap());
    let take_s = b.add_stage("take_s", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("t", sink(Rc::clone(&t)));
    b.connect_primary(src, fill);
    b.connect(fill, StreamId::PRIMARY, take_p, StreamId::PRIMARY);
    b.connect_primary(take_p, sink_p);
    b.connect(fill, StreamId::SECONDARY, take_s, StreamId::PRIMARY);
    b.connect_primary(take_s, sink_s);
    b.connect(fill, StreamId::TERTIARY, sink_t, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a"]);
    assert_eq!(*s.borrow(), vec!["b"]);
    assert_eq!(*t.borrow(), vec!["c", "d"]);
}

#[test]
fn strictly_does_not_delay() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["x", "y"]));
    let fill = b.add_stage("FILLUP", reg.create("FILLUP", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, fill);
    b.connect_primary(fill, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["x", "y"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] FILLUP "))
        .filter_map(|l| {
            if l.contains(" WRITE ") && l.contains(" len=") {
                Some("W")
            } else if l.contains(" READ ") && l.contains(" len=") {
                Some("R")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        events,
        vec!["W", "R", "W", "R"],
        "FILLUP must write before consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn last_remaining_gets_the_record_that_filled_the_previous() {
    // The record that could not be written to the filled stream is the first
    // record of the next stream (not dropped).
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b"]));
    let fill = b.add_stage("FILLUP", reg.create("FILLUP", vec![]).unwrap());
    let take = b.add_stage("TAKE", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fill);
    b.connect(fill, StreamId::PRIMARY, take, StreamId::PRIMARY);
    b.connect_primary(take, sink_p);
    b.connect(fill, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a"]);
    assert_eq!(*s.borrow(), vec!["b"]);
}

#[test]
fn eof_when_last_output_fills() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c"]));
    let fill = b.add_stage("FILLUP", reg.create("FILLUP", vec![]).unwrap());
    let take = b.add_stage("TAKE", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    b.connect_primary(src, fill);
    b.connect(fill, StreamId::PRIMARY, take, StreamId::PRIMARY);
    b.connect_primary(take, sink_p);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a"]);
}

#[test]
fn premature_when_primary_output_not_connected() {
    run_spec("literal a | fillup").unwrap();
}

#[test]
fn premature_even_if_secondary_output_is_connected() {
    // p.345: premature termination is the primary output, not “any” output.
    run_spec("(ENDCHAR ?) literal a | f: fillup ? f: | hole").unwrap();
}

#[test]
fn extra_input_rejected() {
    let err = run_spec("(ENDCHAR ?) literal a | f: fillup | hole ? literal b | f:").unwrap_err();
    assert!(
        err.to_string()
            .contains("no input stream other than the primary"),
        "{err}"
    );
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | fillup stop | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("FILLUP"));
    assert!(!reg.contains("FILLU"));
    let err = run_spec("literal a | fillu | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("FILLU"),
        "{err}"
    );
}

#[test]
fn null_records_are_copied() {
    assert_eq!(run_collect("literal | fillup | >>COLLECT"), vec![""]);
}
