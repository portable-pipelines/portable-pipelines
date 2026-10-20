//! STRLITERAL — Author's Edition 1.1.11 pp.412–413, 534.
//!
//! Portable ASCII: hex/binary dstrings are raw bytes (p.194).

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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "STRLITERAL",
        reg.create(
            "STRLITERAL",
            args.iter().map(|a| (*a).to_string()).collect(),
        )
        .expect("STRLITERAL"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn run_first(args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage(
        "STRLITERAL",
        reg.create(
            "STRLITERAL",
            args.iter().map(|a| (*a).to_string()).collect(),
        )
        .expect("STRLITERAL"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

#[test]
fn default_preface_matches_literal() {
    // p.412: default PREFACE writes the record then copies input.
    assert_eq!(
        run_spec_collect("literal a| strliteral /x/"),
        vec!["x", "a"]
    );
    // LITERAL remainder keeps a blank before `|`; compare PREFACE via args.
    assert_eq!(run_primary(&["a"], &["/x/"]), vec!["x", "a"]);
}

#[test]
fn omitted_dstring_writes_a_null_record() {
    // p.412 railroad: delimitedString is optional; omitted is a null record.
    let out = run_primary(&["a"], &[]);
    assert_eq!(out, vec!["", "a"]);
    assert_eq!(run_first(&["//"]), vec![""]);
}

#[test]
fn preface_keyword_is_explicit_default() {
    assert_eq!(
        run_primary(&["a", "b"], &["PREFACE", "/x/"]),
        vec!["x", "a", "b"]
    );
}

#[test]
fn append_writes_after_input() {
    // p.413: APPEND writes after passing the input.
    assert_eq!(
        run_spec_collect("literal a| strliteral append /x/"),
        vec!["a", "x"]
    );
    assert_eq!(
        run_primary(&["a", "b"], &["APPEND", "/z/"]),
        vec!["a", "b", "z"]
    );
}

#[test]
fn append_with_no_input_still_writes() {
    assert_eq!(run_first(&["APPEND", "/x/"]), vec!["x"]);
}

#[test]
fn conditional_writes_only_when_there_is_input() {
    // p.413: CONDITIONAL writes only when there is input.
    assert_eq!(
        run_primary(&["a"], &["APPEND", "COND", "/x/"]),
        vec!["a", "x"]
    );
    assert_eq!(
        run_first(&["APPEND", "CONDITIONAL", "/x/"]),
        Vec::<String>::new()
    );
}

#[test]
fn ifempty_writes_only_when_there_is_no_input() {
    // p.413: IFEMPTY writes the literal only when there are no input records.
    assert_eq!(run_first(&["IFEMPTY", "/def/"]), vec!["def"]);
    assert_eq!(run_primary(&["a"], &["IFEMPTY", "/def/"]), vec!["a"]);
}

#[test]
fn hex_and_binary_dstrings() {
    // p.534: delimited strings include hex and binary.
    assert_eq!(run_first(&["x4142"]), vec!["AB"]);
    assert_eq!(run_first(&["b01000001"]), vec!["A"]);
}

#[test]
fn cascade_is_not_always_reverse() {
    // p.413 Note 1: PREFACE cascades reverse; APPEND does not.
    assert_eq!(
        run_spec_collect("literal a| strliteral /b/ | strliteral /c/"),
        vec!["c", "b", "a"]
    );
    assert_eq!(
        run_spec_collect("literal a| strliteral append /b/ | strliteral append /c/"),
        vec!["a", "b", "c"]
    );
}

#[test]
fn book_netdata_trailer_shape() {
    // p.413: `strliteral append /\INMR06/` after input.
    assert_eq!(
        run_spec_collect(r"literal body| strliteral append /\INMR06/"),
        vec!["body", r"\INMR06"]
    );
}

#[test]
fn dstring_with_blanks() {
    assert_eq!(run_first(&["/a b/"]), vec!["a b"]);
}

#[test]
fn invented_operands_rejected() {
    let err = run_spec("literal a | strliteral foo | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
    let err = run_spec("literal a | strliteral /x/ extra | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");
    let err = run_spec("literal a | strliteral ifempty conditional /x/ | hole").unwrap_err();
    assert!(
        err.to_string().to_ascii_uppercase().contains("IFEMPTY")
            || err.to_string().to_ascii_uppercase().contains("CONDITIONAL"),
        "{err}"
    );
    let err = run_spec("literal a | strliteral conditional /x/ | hole").unwrap_err();
    assert!(
        err.to_string().to_ascii_uppercase().contains("CONDITIONAL"),
        "{err}"
    );
}

#[test]
fn extra_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "STRLITERAL",
        reg.create("STRLITERAL", vec!["/z/".into()])
            .expect("STRLITERAL"),
    );
    let hole = b.add_stage(
        "hole",
        Box::new(|ctx: &mut StageCtx| while ctx.read().ok().flatten().is_some() {}),
    );
    b.connect_primary(src, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string().to_ascii_uppercase().contains("STRLITERAL"),
        "{err}"
    );
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("STRLITERAL"));
    assert!(!reg.contains("STRLITERA"));
    assert!(!reg.contains("STRLIT"));
    let err = run_spec("literal a | strlit /x/ | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn premature_termination_without_output() {
    let written = Rc::new(Cell::new(0usize));
    let w = Rc::clone(&written);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                w.set(w.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "STRLITERAL",
        reg.create("STRLITERAL", vec!["/x/".into()])
            .expect("STRLITERAL"),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "STRLITERAL should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn parsed_once_as_filter_dstrings() {
    // p.534 / p.412: STR form uses delimited-string scanning, not remainder.
    let parsed = pipe_parser::parse_pipeline("strliteral append /a b/").expect("parse");
    let args: Vec<&str> = parsed.pipelines[0][0]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(args, vec!["append", "/a b/"]);
}
