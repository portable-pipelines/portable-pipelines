//! FANOUTWO — Author's Edition 1.1.11 pp.342–343 (tutorial p.90).

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

fn two_out(records: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let fan = b.add_stage("FANOUTWO", reg.create("FANOUTWO", vec![]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    (primary, secondary)
}

#[test]
fn copies_to_both_outputs() {
    let (p, s) = two_out(&["a", "b"]);
    assert_eq!(p, vec!["a", "b"]);
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn spec_copies_to_both_outputs() {
    assert_eq!(
        run_collect(
            "(ENDCHAR ?) literal c|literal b|literal a| o: fanoutwo | hole ? o: | >>COLLECT"
        ),
        vec!["a", "b", "c"]
    );
}

#[test]
fn writes_primary_before_secondary() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["x"]));
    let fan = b.add_stage("FANOUTWO", reg.create("FANOUTWO", vec![]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();

    let events: Vec<String> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] FANOUTWO "))
        .filter(|l| l.contains(" WRITE stream=") && l.contains(" len="))
        .cloned()
        .collect();
    assert!(
        events.len() >= 2,
        "expected two writes; trace:\n{}",
        trace.borrow().join("\n")
    );
    assert!(
        events[0].contains("WRITE stream=0"),
        "primary first: {events:?}"
    );
    assert!(
        events[1].contains("WRITE stream=1"),
        "secondary second: {events:?}"
    );
}

#[test]
fn strictly_does_not_delay() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["x"]));
    let fan = b.add_stage("FANOUTWO", reg.create("FANOUTWO", vec![]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] FANOUTWO "))
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
        vec!["W", "W", "R"],
        "write both outputs then consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn primary_eof_stops_without_consuming() {
    // p.342: terminate on primary output EOF; do not copy that record to
    // secondary. A one-record consumer shorts immediately after the read so
    // TAKE's write-then-sever window cannot let a second record through.
    let copy = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c"]));
    let fan = b.add_stage("FANOUTWO", reg.create("FANOUTWO", vec![]).unwrap());
    let once = b.add_stage(
        "once",
        Box::new(|ctx: &mut StageCtx| {
            let _ = ctx.read();
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
        }),
    );
    let sink_s = b.add_stage("s", sink(Rc::clone(&copy)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, once, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*copy.borrow(), vec!["a"]);
}

#[test]
fn secondary_eof_continues_on_primary() {
    // p.342: EOF on secondary shorts remaining input to primary.
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c"]));
    let fan = b.add_stage("FANOUTWO", reg.create("FANOUTWO", vec![]).unwrap());
    let take = b.add_stage("TAKE", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, take, StreamId::PRIMARY);
    b.connect_primary(take, sink_s);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a", "b", "c"]);
    assert_eq!(*s.borrow(), vec!["a"]);
}

#[test]
fn primary_eof_stops_the_copy_path() {
    // Tutorial p.90: EOF on the primary output travels backwards, so the
    // secondary copy also stops. TAKE 1 is the primary-side stopper.
    assert_eq!(
        run_collect(
            "(ENDCHAR ?) literal c|literal b|literal a| o: fanoutwo | take 1 | hole ? o: | >>COLLECT"
        ),
        vec!["a"]
    );
}

#[test]
fn two_streams_must_be_defined() {
    let err = run_spec("literal a | fanoutwo | hole").unwrap_err();
    assert!(
        err.to_string().contains("two streams must be defined"),
        "{err}"
    );
}

#[test]
fn extra_output_rejected() {
    let err =
        run_spec("(ENDCHAR ?) literal a | o: fanoutwo | hole ? o: | hole ? o: | hole").unwrap_err();
    assert!(
        err.to_string().contains("only the primary and secondary"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let err =
        run_spec("(ENDCHAR ?) literal a | o: fanoutwo | hole ? literal b | o: | hole").unwrap_err();
    assert!(
        err.to_string()
            .contains("no input stream other than the primary")
            || err.to_string().contains("secondary input"),
        "{err}"
    );
}

#[test]
fn invented_operand_rejected() {
    let err =
        run_spec("(ENDCHAR ?) literal a | o: fanoutwo stop anyeof | hole ? o: | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("FANOUTWO"));
    assert!(!reg.contains("FANOUTW"));
    let err = run_spec("literal a | fanoutw | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn null_records_are_copied() {
    let (p, s) = two_out(&[""]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec![""]);
}
