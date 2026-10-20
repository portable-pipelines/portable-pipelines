//! ABBREV — Author's Edition 1.1.11 p.254.
//!
//! Portable ASCII: the first-word delimiter is blank X'20' (book cites
//! EBCDIC X'40'). ANYCASE folds ASCII A–Z / a–z.

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

fn sink(out: Rc<RefCell<Vec<String>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            out.borrow_mut().push(r.to_string_lossy());
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

fn run_split(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "ABBREV",
        reg.create("ABBREV", args.iter().map(|a| (*a).to_string()).collect())
            .expect("ABBREV"),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = primary.borrow().clone();
    let secondary = secondary.borrow().clone();
    (primary, secondary)
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn book_example_string_min_three() {
    // p.254: `pipe literal st str string strings | split | abbrev string 3 | console`
    assert_eq!(
        run_spec_collect("literal st str string strings | split | abbrev string 3"),
        vec!["str", "string"]
    );
}

#[test]
fn secondary_gets_rejected_records() {
    let (p, s) = run_split(&["st", "str", "string", "strings"], &["string", "3"]);
    assert_eq!(p, vec!["str", "string"]);
    assert_eq!(s, vec!["st", "strings"]);
}

#[test]
fn default_min_zero_keeps_any_prefix_including_null() {
    // Default number 0: null record / leading blank / any prefix of the word.
    let (p, s) = run_split(&["", " s", "s", "st", "x", "string"], &["string"]);
    assert_eq!(p, vec!["", " s", "s", "st", "string"]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn no_arguments_selects_null_and_leading_blank() {
    // Note 2.
    let (p, s) = run_split(&["", " leading", "a", "a b"], &[]);
    assert_eq!(p, vec!["", " leading"]);
    assert_eq!(s, vec!["a", "a b"]);
}

#[test]
fn first_word_stops_at_blank_not_skipping_leading_blanks() {
    // Leading columns until a blank — not REXX WORD() which skips blanks.
    let (p, s) = run_split(&["str extra", "  str", "stringx"], &["string", "3"]);
    assert_eq!(p, vec!["str extra"]);
    assert_eq!(s, vec!["  str", "stringx"]);
}

#[test]
fn anycase_after_the_number() {
    let (p, s) = run_split(&["STR", "str", "St", "xy"], &["string", "3", "ANYCASE"]);
    assert_eq!(p, vec!["STR", "str"]);
    assert_eq!(s, vec!["St", "xy"]);
}

#[test]
fn anycase_synonyms() {
    for kw in ["CASEANY", "IGNORECASE", "CASEIGNORE", "anycase", "ANY"] {
        let (p, s) = run_split(&["STR", "xy"], &["string", "3", kw]);
        assert_eq!(p, vec!["STR"], "{kw}");
        assert_eq!(s, vec!["xy"], "{kw}");
    }
}

#[test]
fn anycase_min_three() {
    // Shared is_anycase (no ANYOF on this stage): `any` works, `an` does not.
    let (p, _) = run_split(&["STR"], &["string", "3", "any"]);
    assert_eq!(p, vec!["STR"]);
    let err = err_of("literal a | abbrev string 3 an | hole");
    assert!(err.contains("unexpected operand"), "{err}");
}

#[test]
fn case_sensitive_without_anycase() {
    let (p, s) = run_split(&["STR", "str"], &["string", "3"]);
    assert_eq!(p, vec!["str"]);
    assert_eq!(s, vec!["STR"]);
}

#[test]
fn min_longer_than_the_word_selects_nothing() {
    let (p, s) = run_split(&["string", "str"], &["string", "10"]);
    assert!(p.is_empty());
    assert_eq!(s, vec!["string", "str"]);
}

#[test]
fn min_zero_explicit() {
    let (p, _) = run_split(&["", "s"], &["string", "0"]);
    assert_eq!(p, vec!["", "s"]);
}

#[test]
fn word_that_looks_like_a_number() {
    let (p, s) = run_split(&["12", "1", "123", "x"], &["123", "2"]);
    assert_eq!(p, vec!["12", "123"]);
    assert_eq!(s, vec!["1", "x"]);
}

#[test]
fn discards_when_only_the_other_stream_is_connected() {
    let reg = StageRegistry::default();
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["str", "xy"]));
    let stage = b.add_stage(
        "ABBREV",
        reg.create("ABBREV", vec!["string".into(), "3".into()])
            .unwrap(),
    );
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*secondary.borrow(), vec!["xy".to_string()]);
}

#[test]
fn anycase_without_number_rejected() {
    let err = err_of("literal a | abbrev string anycase | hole");
    assert!(err.to_ascii_uppercase().contains("ANYCASE"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | abbrev string 3 first | hole",
        "literal a | abbrev string 3 anyof | hole",
        "literal a | abbrev string extra | hole",
        "literal a | abbrev string 3 anycase extra | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unexpected") || err.contains("minimum count"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn negative_min_rejected() {
    let err = err_of("literal a | abbrev string -1 | hole");
    assert!(
        err.contains("minimum count") || err.contains("number"),
        "{err}"
    );
}

#[test]
fn verb_is_exact() {
    // Diagram is all-caps (p.254); `abbre` is unknown.
    let reg = StageRegistry::default();
    assert!(reg.contains("ABBREV"));
    assert!(!reg.contains("ABBRE"));
    assert!(!reg.contains("ABB"));
    let err = err_of("literal a | abbre | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("ABBRE"),
        "{err}"
    );
}

#[test]
fn secondary_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("ABBREV", reg.create("ABBREV", vec![]).unwrap());
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
    assert!(err.to_string().contains("secondary input"), "{err}");
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
    let stage = b.add_stage("ABBREV", reg.create("ABBREV", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "ABBREV should stop when no output is connected, wrote {}",
        written.get()
    );
}
