//! XRANGE device driver — Author's Edition 1.1.11 pp.609–610.
//!
//! Writes one record containing an inclusive character range. Must be first.
//! Portable ASCII: xorc hex is the byte value (book `f0 f9` → digits is EBCDIC).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

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

fn run_xrange(args: &[&str]) -> Vec<u8> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage(
        "XRANGE",
        reg.create("XRANGE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("XRANGE"),
    );
    let snk = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().first().cloned().unwrap_or_default();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn book_letter_forms() {
    // p.610: a-i / a.9 / a i → abcdefghi
    assert_eq!(run_spec_collect("xrange a-i"), vec!["abcdefghi"]);
    assert_eq!(run_spec_collect("xrange a.9"), vec!["abcdefghi"]);
    assert_eq!(run_spec_collect("xrange a i"), vec!["abcdefghi"]);
}

#[test]
fn wrap_fe_01() {
    // p.610: xrange fe 01 | spec 1-* c2x → FEFF0001
    assert_eq!(
        run_spec_collect("xrange fe 01 | spec 1-* c2x"),
        vec!["FEFF0001"]
    );
    assert_eq!(
        run_spec_collect("xrange fe-01 | spec 1-* c2x"),
        vec!["FEFF0001"]
    );
}

#[test]
fn wrap_nine_to_zero_count() {
    // p.610: xrange 9 0 | count bytes → 248
    assert_eq!(run_spec_collect("xrange 9 0 | count bytes"), vec!["248"]);
}

#[test]
fn hex_xorc_is_portable_ascii() {
    // Book `xrange f0 f9` prints 0123456789 only in EBCDIC.
    assert_eq!(
        run_spec_collect("xrange f0 f9 | spec 1-* c2x"),
        vec!["F0F1F2F3F4F5F6F7F8F9"]
    );
    assert_eq!(run_spec_collect("xrange 30 39"), vec!["0123456789"]);
}

#[test]
fn default_is_neutral_table() {
    let rec = run_xrange(&[]);
    assert_eq!(rec.len(), 256);
    assert_eq!(rec[0], 0);
    assert_eq!(rec[255], 255);
    assert_eq!(run_spec_collect("xrange | count bytes"), vec!["256"]);
    assert_eq!(run_spec_collect("xrange 00-ff | count bytes"), vec!["256"]);
    assert_eq!(run_spec_collect("xrange 00.256 | count bytes"), vec!["256"]);
}

#[test]
fn single_xorc_and_keywords() {
    assert_eq!(run_spec_collect("xrange Q"), vec!["Q"]);
    assert_eq!(run_spec_collect("xrange blank | spec 1-* c2x"), vec!["20"]);
    assert_eq!(run_spec_collect("xrange tab | spec 1-* c2x"), vec!["09"]);
    assert_eq!(run_spec_collect("xrange space"), vec![" "]);
}

#[test]
fn two_word_blank_range() {
    assert_eq!(run_spec_collect("xrange blank 22"), vec![" !\""]);
}

#[test]
fn must_be_first() {
    let err = err_of("literal a | xrange a-i | hole");
    assert!(err.contains("must be a first stage"), "{err}");
}

#[test]
fn invented_and_extra_operands_rejected() {
    for spec in [
        "xrange a i z | hole",
        "xrange FROM a | hole",
        "xrange a--i | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected") || err.contains("invalid"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("XRANGE"));
    assert!(!reg.contains("XRANG"));
    assert!(!reg.contains("XRAN"));
    let err = err_of("xrang a-i | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("XRANG"),
        "{err}"
    );
}

#[test]
fn premature_termination_without_output() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let _stage = b.add_stage("XRANGE", reg.create("XRANGE", vec!["a-i".into()]).unwrap());
    // No output connection: XRANGE must not hang.
    b.run().unwrap();
}

#[test]
fn one_record_only() {
    assert_eq!(run_xrange(&["a", "c"]).len(), 3);
    let recs = run_spec_collect("xrange a-c");
    assert_eq!(recs, vec!["abc"]);
}

#[test]
fn secondary_output_is_unused() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage("XRANGE", reg.create("XRANGE", vec!["a-c".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["abc".to_string()]);
    assert!(secondary.borrow().is_empty());
}
