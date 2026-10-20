//! SUBSTRING — Author's Edition 1.1.11 p.536.
//!
//! Portable ASCII: `inputRange` uses the shared helper (pp.195–196). Word
//! separator is blank X'20'; field separator is HT X'09'.

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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "SUBSTRING",
        reg.create("SUBSTRING", args.iter().map(|a| (*a).to_string()).collect())
            .expect("SUBSTRING"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn run_primary_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "SUBSTRING",
        reg.create("SUBSTRING", args.iter().map(|a| (*a).to_string()).collect())
            .expect("SUBSTRING"),
    );
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&primary)));
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
fn column_span() {
    assert_eq!(
        run_spec_collect("literal abcdef| substring 2-4"),
        vec!["bcd"]
    );
    assert_eq!(run_primary(&["abcdef"], &["1-3"]), vec!["abc"]);
    assert_eq!(run_primary(&["abcdef"], &["1-*"]), vec!["abcdef"]);
    assert_eq!(run_primary(&["abcdef"], &["*-*"]), vec!["abcdef"]);
}

#[test]
fn length_form_and_single_column() {
    assert_eq!(run_primary(&["abcdef"], &["3.2"]), vec!["cd"]);
    assert_eq!(run_primary(&["abcdef"], &["3"]), vec!["c"]);
}

#[test]
fn negative_columns() {
    assert_eq!(run_primary(&["abcdef"], &["-1"]), vec!["f"]);
    assert_eq!(run_primary(&["abcdef"], &["-2;-1"]), vec!["ef"]);
}

#[test]
fn short_record_clips_the_end() {
    assert_eq!(run_primary(&["ab"], &["1-5"]), vec!["ab"]);
}

#[test]
fn missing_range_writes_a_null_record() {
    // SPEC optimisation (p.536 Note 1): one output per input; missing → null.
    assert_eq!(run_primary(&["ab"], &["10"]), vec![""]);
    assert_eq!(run_primary(&["one two"], &["word", "9"]), vec![""]);
}

#[test]
fn word_and_field_ranges() {
    assert_eq!(run_primary(&["one two three"], &["word", "2"]), vec!["two"]);
    assert_eq!(run_primary(&["one two three"], &["w1"]), vec!["one"]);
    let rec = "a\tb\tc";
    assert_eq!(run_primary(&[rec], &["field", "2"]), vec!["b"]);
    assert_eq!(run_primary(&[rec], &["f3"]), vec!["c"]);
}

#[test]
fn nested_substring_of() {
    let rec = "What a rum fellow.";
    assert_eq!(
        run_primary(&[rec], &["substring", "2", "of", "word", "3"]),
        vec!["u"]
    );
    assert_eq!(
        run_primary(&[rec], &["substr", "1-2", "of", "word", "3"]),
        vec!["ru"]
    );
}

#[test]
fn word_separator_override() {
    assert_eq!(
        run_primary(&["a,b,c"], &["wordseparator", ",", "word", "2"]),
        vec!["b"]
    );
}

#[test]
fn verb_abbrev_min_six() {
    // SUBSTRing p.536: substr works; subst is shorter than the uppercase prefix.
    assert_eq!(run_spec_collect("literal abcdef| substr 2-4"), vec!["bcd"]);
    let err = err_of("literal abcdef | subst 2-4 | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("SUBST"),
        "{err}"
    );
}

#[test]
fn missing_range_operand_rejected() {
    let err = err_of("literal a | substring | hole");
    assert!(err.contains("inputRange"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | substring anycase 1 | hole",
        "literal a | substring 1-2 extra | hole",
        "literal a | substring string /a/ | hole",
        "literal a | substring first | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected") || err.contains("range"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn extra_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage(
        "SUBSTRING",
        reg.create("SUBSTRING", vec!["1".into()]).unwrap(),
    );
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
    let stage = b.add_stage(
        "SUBSTRING",
        reg.create("SUBSTRING", vec!["1-*".into()]).unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "SUBSTRING should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert!(run_primary(&[], &["1-*"]).is_empty());
}

#[test]
fn binary_bytes() {
    let out = run_primary_bytes(vec![vec![0x00, 0xff, 0x7e]], &["2"]);
    assert_eq!(out, vec![vec![0xff]]);
}

#[test]
fn null_input_record() {
    assert_eq!(run_primary(&[""], &["1-*"]), vec![""]);
    assert_eq!(run_primary(&[""], &["1"]), vec![""]);
}
