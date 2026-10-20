//! FANINTWO — Author's Edition 1.1.11 pp.340–341.

use std::cell::{Cell, RefCell};
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

fn fanintwo(primary: &[&str], secondary: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(primary));
    let src_s = b.add_stage("src_s", emit(secondary));
    let fan = b.add_stage("FANINTWO", reg.create("FANINTWO", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src_p, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn secondary_has_priority() {
    let got = fanintwo(&["P1", "P2"], &["S1"]);
    assert_eq!(got, vec!["S1", "P1", "P2"]);
}

#[test]
fn primary_only_when_secondary_is_empty() {
    let got = fanintwo(&["P1", "P2"], &[]);
    assert_eq!(got, vec!["P1", "P2"]);
}

#[test]
fn secondary_only() {
    let got = fanintwo(&[], &["S1", "S2"]);
    assert_eq!(got, vec!["S1", "S2"]);
}

#[test]
fn spec_label_secondary_priority() {
    assert_eq!(
        run_collect("(ENDCHAR ?) literal PRIMARY| f: fanintwo | >>COLLECT ? literal SECONDARY| f:"),
        vec!["SECONDARY", "PRIMARY"]
    );
}

#[test]
fn spec_primary_only() {
    assert_eq!(
        run_collect("literal b|literal a| fanintwo | >>COLLECT"),
        vec!["a", "b"]
    );
}

#[test]
fn secondary_does_not_delay() {
    // p.340: strictly does not delay secondary — WRITE then READ.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src_p = b.add_stage("src_p", emit(&[]));
    let src_s = b.add_stage("src_s", emit(&["S"]));
    let fan = b.add_stage("FANINTWO", reg.create("FANINTWO", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src_p, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["S"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] FANINTWO "))
        .filter_map(|l| {
            if l.contains(" WRITE ") && l.contains(" len=") {
                Some("W")
            } else if l.contains(" READ stream=1 ") && l.contains(" len=") {
                Some("R1")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        events,
        vec!["W", "R1"],
        "secondary must be written before consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn primary_consumed_after_write_and_secondary_drain() {
    // p.340: after writing a primary-originated record, drain secondary
    // before consuming the primary.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let produced = Rc::new(Cell::new(0usize));
    let flag = Rc::clone(&produced);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src_p = b.add_stage(
        "src_p",
        Box::new(move |ctx: &mut StageCtx| {
            flag.set(1);
            if ctx.write(Record::from("P")).is_err() {
                return;
            }
            flag.set(2);
        }),
    );
    let src_s = b.add_stage("src_s", emit(&[]));
    let fan = b.add_stage("FANINTWO", reg.create("FANINTWO", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src_p, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["P"]);
    assert_eq!(produced.get(), 2);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] FANINTWO "))
        .filter_map(|l| {
            if l.contains(" WRITE ") && l.contains(" len=") {
                Some("W")
            } else if l.contains(" READ stream=0 ") && l.contains(" len=") {
                Some("R0")
            } else if l.contains(" YIELD ") {
                Some("Y")
            } else {
                None
            }
        })
        .collect();
    assert!(
        events.windows(2).any(|w| w == ["W", "R0"]),
        "primary write then consume; events={events:?}\n{}",
        trace.borrow().join("\n")
    );
    assert!(
        events.contains(&"Y"),
        "primary path must yield; events={events:?}"
    );
}

#[test]
fn eof_on_output_stops_without_consuming_primary() {
    // TAKE 1 severs after the first record; FANINTWO must not consume the
    // primary record that it cannot write (EOF backwards, p.341).
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src_p = b.add_stage("src_p", emit(&["A", "B"]));
    let src_s = b.add_stage("src_s", emit(&[]));
    let fan = b.add_stage("FANINTWO", reg.create("FANINTWO", vec![]).unwrap());
    let take = b.add_stage("TAKE", reg.create("TAKE", vec!["1".into()]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src_p, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, take);
    b.connect_primary(take, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["A"]);
    let primary_reads = trace
        .borrow()
        .iter()
        .filter(|l| {
            l.contains("] FANINTWO ") && l.contains(" READ stream=0 ") && l.contains(" len=")
        })
        .count();
    assert_eq!(
        primary_reads,
        1,
        "second primary record must remain unconsumed; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | fanintwo strict | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn extra_output_rejected() {
    let err = run_spec("(ENDCHAR ?) literal a | f: fanintwo | hole ? f: | hole").unwrap_err();
    assert!(
        err.to_string().contains("only connected output")
            || err.to_string().contains("primary output"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let err =
        run_spec("(ENDCHAR ?) literal a | f: fanintwo | hole ? literal b | f: ? literal c | f:")
            .unwrap_err();
    assert!(
        err.to_string().contains("only the primary and secondary"),
        "{err}"
    );
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("FANINTWO"));
    assert!(!reg.contains("FANINTW"));
    let err = run_spec("literal a | fanintw | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("FANINTW"),
        "{err}"
    );
}

#[test]
fn premature_when_primary_output_missing() {
    run_spec("(ENDCHAR ?) literal a | f: fanintwo ? literal b | f:").unwrap();
}
