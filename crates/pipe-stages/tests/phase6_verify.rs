//! VERIFY — Author's Edition 1.1.11 p.594.
//!
//! Portable ASCII: the delimited string enumerates allowed bytes. ANYCASE
//! folds ASCII A–Z / a–z via the shared ByteSet helper. `/0-9/` is the
//! characters 0, hyphen, 9 — not an xrange.

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
        "VERIFY",
        reg.create("VERIFY", args.iter().map(|a| (*a).to_string()).collect())
            .expect("VERIFY"),
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
fn book_column_range_digits() {
    // p.594: `verify 5.5 /0123456789/` — columns 5–9 are digits.
    let recs = ["xxxx12345", "xxxx12a45", "ab"];
    let (p, s) = run_split(&recs, &["5.5", "/0123456789/"]);
    assert_eq!(p, vec!["xxxx12345", "ab"]); // `ab` is a null range → selected
    assert_eq!(s, vec!["xxxx12a45"]);
}

#[test]
fn entire_record_character_list() {
    let (p, s) = run_split(&["abc", "abca", "abd", ""], &["/abc/"]);
    assert_eq!(p, vec!["abc", "abca", ""]);
    assert_eq!(s, vec!["abd"]);
}

#[test]
fn hex_and_binary_dstrings() {
    // x30 x31 x32 = ASCII '0' '1' '2'.
    let (p, s) = run_split(&["012", "0", "3", ""], &["x303132"]);
    assert_eq!(p, vec!["012", "0", ""]);
    assert_eq!(s, vec!["3"]);
    let (p, _) = run_split(&["A"], &["b01000001"]);
    assert_eq!(p, vec!["A"]);
}

#[test]
fn anycase_before_the_range() {
    let (p, s) = run_split(&["AbC", "abc", "AbCd"], &["ANYCASE", "/abc/"]);
    assert_eq!(p, vec!["AbC", "abc"]);
    assert_eq!(s, vec!["AbCd"]);
}

#[test]
fn anycase_with_a_range() {
    let (p, s) = run_split(&["xxABC", "xxAbC", "xxABD"], &["ANYCASE", "3-5", "/abc/"]);
    assert_eq!(p, vec!["xxABC", "xxAbC"]);
    assert_eq!(s, vec!["xxABD"]);
}

#[test]
fn anycase_synonyms() {
    for kw in ["CASEANY", "IGNORECASE", "CASEIGNORE", "anycase", "ANY"] {
        let (p, s) = run_split(&["A", "x"], &[kw, "/a/"]);
        assert_eq!(p, vec!["A"], "{kw}");
        assert_eq!(s, vec!["x"], "{kw}");
    }
}

#[test]
fn anycase_min_three() {
    let (p, _) = run_split(&["A"], &["any", "/a/"]);
    assert_eq!(p, vec!["A"]);
    let err = err_of("literal a | verify an /a/ | hole");
    assert!(
        err.contains("unrecognized operand") || err.contains("delimited"),
        "{err}"
    );
}

#[test]
fn word_and_field_range() {
    let (p, s) = run_split(&["id 123", "id 12a"], &["word", "2", "/0123456789/"]);
    assert_eq!(p, vec!["id 123"]);
    assert_eq!(s, vec!["id 12a"]);
}

#[test]
fn empty_allowed_set_selects_only_null_ranges() {
    let (p, s) = run_split(&["", "a"], &["//"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec!["a"]);
}

#[test]
fn hyphen_in_dstring_is_not_xrange() {
    // `/0-9/` enumerates '0', '-', '9' (p.594 "enumerates the characters").
    let (p, s) = run_split(&["0-9", "09", "5", "-"], &["/0-9/"]);
    assert_eq!(p, vec!["0-9", "09", "-"]);
    assert_eq!(s, vec!["5"]);
}

#[test]
fn xrange_operand_is_rejected() {
    let err = err_of("literal a | verify 0-9 | hole");
    // `0-9` parses as inputRange columns 0–9, then the dstring is missing.
    assert!(
        err.contains("delimited") || err.contains("unrecognized"),
        "{err}"
    );
    let err = err_of("literal a | verify a-z /abc/ | hole");
    assert!(
        err.contains("range") || err.contains("unrecognized") || err.contains("delimited"),
        "{err}"
    );
}

#[test]
fn not_verify_is_the_book_converse() {
    // p.594: `not verify 5.5 /0123456789/` keeps records with a non-digit in the range.
    assert_eq!(
        run_spec_collect("literal xxxx12a45|literal xxxx12345| not verify 5.5 /0123456789/"),
        vec!["xxxx12a45"]
    );
}

#[test]
fn anycase_after_the_dstring_rejected() {
    let err = err_of("literal a | verify /abc/ anycase | hole");
    assert!(err.contains("unexpected operand"), "{err}");
}

#[test]
fn missing_dstring_rejected() {
    let err = err_of("literal a | verify | hole");
    assert!(err.to_ascii_uppercase().contains("DELIMITED"), "{err}");
    let err = err_of("literal a | verify 5.5 | hole");
    assert!(err.to_ascii_uppercase().contains("DELIMITED"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | verify anyof /abc/ | hole",
        "literal a | verify string /abc/ | hole",
        "literal a | verify /abc/ extra | hole",
        "literal a | verify first /abc/ | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected") || err.contains("delimited"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn undelimited_list_rejected() {
    let err = err_of("literal a | verify abc | hole");
    assert!(
        err.contains("unrecognized") || err.contains("delimited"),
        "{err}"
    );
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("VERIFY"));
    assert!(!reg.contains("VERIF"));
    assert!(!reg.contains("VER"));
    let err = err_of("literal a | verif /a/ | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("VERIF"),
        "{err}"
    );
}

#[test]
fn secondary_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("VERIFY", reg.create("VERIFY", vec!["/a/".into()]).unwrap());
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
    let stage = b.add_stage("VERIFY", reg.create("VERIFY", vec!["/o/".into()]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "VERIFY should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn discards_when_only_the_other_stream_is_connected() {
    let reg = StageRegistry::default();
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc", "abd"]));
    let stage = b.add_stage(
        "VERIFY",
        reg.create("VERIFY", vec!["/abc/".into()]).unwrap(),
    );
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*secondary.borrow(), vec!["abd".to_string()]);
}
