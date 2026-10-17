//! COPY (p.305) and HOLE (p.382) — Author's Edition 1.1.11.

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

fn run_primary(verb: &str, records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap(),
    );
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn copy_passes_records_including_null() {
    assert_eq!(
        run_primary("COPY", &["a", "", "b"], &[]),
        vec!["a", "", "b"]
    );
}

#[test]
fn copy_rejects_invented_operands() {
    let err = run_spec("literal a | copy first | hole").unwrap_err();
    assert!(err.to_string().contains("no operands"), "{err}");
}

#[test]
fn hole_discards_primary() {
    run_spec("literal a | hole").unwrap();
}

#[test]
fn hole_reads_every_connected_input() {
    // p.382: hole reads from all defined input streams.
    let reg = StageRegistry::default();
    let written = Rc::new(RefCell::new(0u32));
    let c1 = Rc::clone(&written);
    let c2 = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src1 = b.add_stage(
        "src1",
        Box::new(move |ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from("p"));
            *c1.borrow_mut() += 1;
        }),
    );
    let src2 = b.add_stage(
        "src2",
        Box::new(move |ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from("s"));
            *c2.borrow_mut() += 1;
        }),
    );
    let hole = b.add_stage("HOLE", reg.create("HOLE", vec![]).unwrap());
    b.connect(src1, StreamId::PRIMARY, hole, StreamId::PRIMARY);
    b.connect(src2, StreamId::PRIMARY, hole, StreamId::SECONDARY);
    b.run().unwrap();
    assert_eq!(*written.borrow(), 2);
}

#[test]
fn hole_rejects_invented_operands() {
    let err = run_spec("literal a | hole all | console").unwrap_err();
    assert!(err.to_string().contains("no operands"), "{err}");
}
