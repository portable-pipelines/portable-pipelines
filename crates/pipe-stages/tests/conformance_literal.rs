//! LITERAL — Author's Edition 1.1.11 pp. 412–413.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
use pipe_stages::StageRegistry;

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

fn run_args(records: &[&str], args: Vec<String>) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&out);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage("LITERAL", reg.create("LITERAL", args).unwrap());
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                sink_out.borrow_mut().push(r.to_string_lossy());
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
fn omitted_argument_writes_a_null_record() {
    // p.413: "literal writes a null record when the parameter string is omitted."
    assert_eq!(run_args(&[], vec![]), vec!["".to_string()]);
}

#[test]
fn writes_then_passes_input() {
    assert_eq!(
        run_args(&["in"], vec!["hi".into()]),
        vec!["hi".to_string(), "in".to_string()]
    );
}

#[test]
fn cascade_is_reverse_order() {
    // p.413 note 1 / Figure 62: a cascade of LITERAL stages is reverse order.
    let out = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let a = b.add_stage("LITERAL", reg.create("LITERAL", vec!["a".into()]).unwrap());
    let lit_b = b.add_stage("LITERAL", reg.create("LITERAL", vec!["b".into()]).unwrap());
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                sink_out.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(a, lit_b);
    b.connect_primary(lit_b, sink);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["b".to_string(), "a".to_string()]);
}

#[test]
fn extra_blanks_are_kept() {
    let p = pipe_parser::parse_pipeline("literal  hello  world|hole").unwrap();
    assert_eq!(p.stages[0].args[0].as_str(), " hello  world");
}
