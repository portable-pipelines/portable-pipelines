//! REVERSE — Author's Edition 1.1.11 p.474.

use std::cell::{Cell, RefCell};
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

fn emit_bytes(records: Vec<Vec<u8>>) -> Box<dyn FnOnce(&mut StageCtx)> {
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

fn sink_bytes(out: Rc<RefCell<Vec<Vec<u8>>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            out.borrow_mut().push(r.as_bytes().to_vec());
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
    let full = format!("{spec} | >>COLLECT");
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn run_primary(records: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage("REVERSE", reg.create("REVERSE", vec![]).expect("REVERSE"));
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn book_example_hello_world() {
    // p.474: `pipe literal Hello, World! | reverse | console` → `!dlroW ,olleH`
    assert_eq!(
        run_spec_collect("literal Hello, World!| reverse"),
        vec!["!dlroW ,olleH"]
    );
    // The book’s printed `literal Hello, World! | reverse` has a space before
    // `|` (p.412 trailing blank). Reverse then starts with a blank.
    assert_eq!(
        run_spec_collect("literal Hello, World! | reverse"),
        vec![" !dlroW ,olleH"]
    );
}

#[test]
fn book_example_find_records_that_end_in_xyz() {
    // p.474: `... | reverse | find zyx| reverse | ...`
    // No space before `|` after zyx: the FIND string is three bytes.
    assert_eq!(
        run_spec_collect(
            "literal fooxyz|literal abxyz|literal xyz|literal xy| reverse | find zyx| reverse"
        ),
        vec!["xyz", "abxyz", "fooxyz"]
    );
}

#[test]
fn reverses_each_record_not_the_file() {
    assert_eq!(run_primary(&["ab", "cd"]), vec!["ba", "dc"]);
}

#[test]
fn null_and_one_byte_records() {
    assert_eq!(run_primary(&[""]), vec![""]);
    assert_eq!(run_primary(&["a"]), vec!["a"]);
}

#[test]
fn reverses_raw_bytes() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(vec![vec![0xff, 0x00, 0x01]]));
    let stage = b.add_stage("REVERSE", reg.create("REVERSE", vec![]).unwrap());
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec![vec![0x01, 0x00, 0xff]]);
}

#[test]
fn operands_are_rejected() {
    let err = err_of("literal a | reverse 1 | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | reverse LEFT | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | reverse ANYCASE | hole");
    assert!(err.contains("unexpected operand"), "{err}");
}

#[test]
fn verb_is_exact() {
    let err = err_of("literal a | rev | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("REV"),
        "{err}"
    );
}

#[test]
fn extra_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("REVERSE", reg.create("REVERSE", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect_primary(src, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("primary"), "{err}");
}

#[test]
fn premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["one", "two", "three", "four"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("REVERSE", reg.create("REVERSE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "REVERSE should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn secondary_output_is_unused() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["ab"]));
    let stage = b.add_stage("REVERSE", reg.create("REVERSE", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["ba".to_string()]);
    assert!(secondary.borrow().is_empty());
}
