//! FANIN (p.338), FANINANY (p.339), FANOUT (p.341) — Author's Edition 1.1.11.

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

#[test]
fn fanin_primary_then_secondary() {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let a = b.add_stage("a", emit(&["A1", "A2"]));
    let c = b.add_stage("c", emit(&["B1"]));
    let fan = b.add_stage("FANIN", reg.create("FANIN", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(a, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(c, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, snk);
    b.run().unwrap();
    assert_eq!(
        *out.borrow(),
        vec!["A1".to_string(), "A2".to_string(), "B1".to_string()]
    );
}

#[test]
fn fanin_stream_list_reorders() {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let a = b.add_stage("a", emit(&["A"]));
    let c = b.add_stage("c", emit(&["B"]));
    let fan = b.add_stage(
        "FANIN",
        reg.create("FANIN", vec!["1".into(), "0".into()]).unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(a, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(c, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["B".to_string(), "A".to_string()]);
}

#[test]
fn fanin_named_stream_stays_subset() {
    let err = run_spec("literal a | fanin stat | hole").unwrap_err();
    assert!(
        err.to_string().contains("not implemented"),
        "{err}"
    );
}

#[test]
fn faninany_reads_whichever_is_ready() {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let a = b.add_stage("a", emit(&["A"]));
    let c = b.add_stage("c", emit(&["B"]));
    let fan = b.add_stage("FANINANY", reg.create("FANINANY", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(a, StreamId::PRIMARY, fan, StreamId::PRIMARY);
    b.connect(c, StreamId::PRIMARY, fan, StreamId::SECONDARY);
    b.connect_primary(fan, snk);
    b.run().unwrap();
    let got = out.borrow().clone();
    assert_eq!(got.len(), 2);
    assert!(got.contains(&"A".to_string()));
    assert!(got.contains(&"B".to_string()));
}

#[test]
fn faninany_strict_is_accepted() {
    run_spec("(ENDCHAR ?) literal a|literal b | fanin | hole").unwrap();
    let reg = StageRegistry::default();
    assert!(reg.create("FANINANY", vec!["STRICT".into()]).is_some());
}

#[test]
fn fanout_copies_to_every_connected_output() {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["x"]));
    let fan = b.add_stage("FANOUT", reg.create("FANOUT", vec![]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["x".to_string()]);
    assert_eq!(*s.borrow(), vec!["x".to_string()]);
}

#[test]
fn fanout_stop_anyeof() {
    run_spec("literal a | fanout stop anyeof | hole").unwrap();
}

#[test]
fn fanout_invented_operand_rejected() {
    let err = run_spec("literal a | fanout forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized"), "{err}");
}
