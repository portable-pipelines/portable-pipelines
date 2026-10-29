//! STACK — process-wide FIFO / LIFO program stack.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
use pipe_stages::{run_spec, with_cleared_program_stack, StageRegistry};

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

fn push(records: &[&str], args: &[&str]) {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "STACK",
        reg.create("STACK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("STACK"),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
}

fn drain(args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage(
        "STACK",
        reg.create("STACK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("STACK"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn push_and_copy(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "STACK",
        reg.create("STACK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("STACK"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn fifo_preserves_order() {
    with_cleared_program_stack(|| {
        push(&["a", "b", "c"], &["FIFO"]);
        assert_eq!(drain(&["FIFO"]), vec!["a", "b", "c"]);
        assert!(drain(&["FIFO"]).is_empty());
    });
}

#[test]
fn lifo_is_default_and_reverses() {
    with_cleared_program_stack(|| {
        push(&["a", "b", "c"], &[]);
        assert_eq!(drain(&["LIFO"]), vec!["c", "b", "a"]);
    });
}

#[test]
fn sink_copies_through() {
    with_cleared_program_stack(|| {
        assert_eq!(push_and_copy(&["x", "y"], &["FIFO"]), vec!["x", "y"]);
        assert_eq!(drain(&["FIFO"]), vec!["x", "y"]);
    });
}

#[test]
fn empty_stack_is_eof() {
    with_cleared_program_stack(|| {
        assert!(drain(&["FIFO"]).is_empty());
        assert!(drain(&["LIFO"]).is_empty());
    });
}

#[test]
fn rejects_unknown_operand() {
    let err = run_spec("stack sideways | hole").unwrap_err().to_string();
    assert!(err.contains("unrecognized operand"), "{err}");
}
