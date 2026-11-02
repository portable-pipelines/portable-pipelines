//! Named stream identifiers: FANOUT / FANIN lists and DEAL / GATHER words.

use std::cell::RefCell;
use std::rc::Rc;

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

fn collect(spec: &str) -> Vec<String> {
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
fn named_fanout_into_named_fanin() {
    let out = collect(
        "(ENDCHAR ?) literal hello| a: fanout LEFT RIGHT | b: fanin LEFT RIGHT | >>COLLECT ? a: | elastic | b:",
    );
    assert_eq!(out, vec!["hello".to_string(), "hello".to_string()]);
}

#[test]
fn fanout_numeric_list_writes_only_those_streams() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["x"]));
    let fan = b.add_stage("FANOUT", reg.create("FANOUT", vec!["0".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["x".to_string()]);
    assert!(s.borrow().is_empty(), "FANOUT 0 must not copy to stream 1");
}

#[test]
fn fanin_named_list_reorders_like_numbers() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let a = b.add_stage("a", emit(&["A"]));
    let c = b.add_stage("c", emit(&["B"]));
    let fan = b.add_stage(
        "FANIN",
        reg.create("FANIN", vec!["SECONDARY".into(), "PRIMARY".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&p)));
    b.connect(a, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(c, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, snk);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["B".to_string(), "A".to_string()]);
}

#[test]
fn deal_and_gather_by_reserved_name() {
    let (p, s) = {
        let p = Rc::new(RefCell::new(Vec::new()));
        let s = Rc::new(RefCell::new(Vec::new()));
        let reg = StageRegistry::default();
        let mut b = PipelineBuilder::new();
        let src_p = b.add_stage("data", emit(&["aa", "bb"]));
        let src_s = b.add_stage("ids", emit(&["PRIMARY", "SECONDARY"]));
        let stage = b.add_stage(
            "DEAL",
            reg.create("DEAL", vec!["SECONDARY".into()]).unwrap(),
        );
        let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
        let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
        b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
        b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
        b.connect_primary(stage, sink_p);
        b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
        b.run().unwrap();
        let primary = p.borrow().clone();
        let secondary = s.borrow().clone();
        (primary, secondary)
    };
    assert_eq!(p, vec!["aa"]);
    assert_eq!(s, vec!["bb"]);

    let gathered = collect(
        "(ENDCHAR ?) literal SECONDARYx| g: gather streamid 1.9 | >>COLLECT ? literal from1| g:",
    );
    assert_eq!(gathered, vec!["from1".to_string()]);
}

#[test]
fn unknown_and_duplicate_errors_are_plain_words() {
    let err = run_spec("literal a | fanout LEFT LEFT | hole")
        .unwrap_err()
        .to_string();
    assert!(err.contains("duplicate stream identifier LEFT"), "{err}");
    let err = run_spec("literal nosuch | gather streamid 1-6 | hole")
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown stream identifier"), "{err}");
}
