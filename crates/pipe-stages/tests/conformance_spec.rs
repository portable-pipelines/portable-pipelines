//! SPEC / SPECS — Author's Edition 1.1.11 pp. 496–497 (Phase 3 subset).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
use pipe_stages::StageRegistry;

fn run_primary(verb: &str, records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let recs: Vec<String> = records.iter().map(|x| (*x).to_string()).collect();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for r in recs {
                let _ = ctx.write(Record::from(r));
            }
        }),
    );
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
fn specs_is_the_documented_keyword_synonym() {
    assert!(StageRegistry::default().contains("SPEC"));
    assert!(StageRegistry::default().contains("SPECS"));
    assert_eq!(
        run_primary("SPECS", &["abc"], &["1-*", "1"]),
        vec!["abc"]
    );
}

#[test]
fn word_and_literal_subset() {
    assert_eq!(
        run_primary("SPEC", &["red green blue"], &["WORD", "2", "1"]),
        vec!["green"]
    );
    assert_eq!(
        run_primary("SPEC", &["abc"], &["1-*", "1", "/*/", "next"]),
        vec!["abc*"]
    );
}

#[test]
fn empty_spec_copies_the_record() {
    assert_eq!(run_primary("SPEC", &["keep"], &[]), vec!["keep"]);
}
