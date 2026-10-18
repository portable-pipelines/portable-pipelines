//! PREFACE / APPEND — Author's Edition 1.1.11 pp.461–462 / pp.263–264.

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

fn run_spec_collect(spec: &str) -> Vec<String> {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let sink_out = Rc::clone(&sink_out);
        move |_| pipe_stages::collect_to(Rc::clone(&sink_out))
    });
    let full = with_collect(spec);
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn with_collect(spec: &str) -> String {
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse options");
    format!("{spec} {} >>COLLECT", parsed.options.separator)
}

fn run_spec_collect_err(spec: &str) -> String {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let sink_out = Rc::clone(&sink_out);
        move |_| pipe_stages::collect_to(Rc::clone(&sink_out))
    });
    let full = with_collect(spec);
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg)
        .unwrap_err()
        .to_string()
}

#[test]
fn preface_book_self_escape() {
    // p.462: split is in the nested pipeline; only “a b” is split.
    assert_eq!(
        run_spec_collect("literal c d e| preface literal a b || split"),
        vec!["a", "b", "c d e"]
    );
}

#[test]
fn preface_book_alt_separator() {
    assert_eq!(
        run_spec_collect("(sep ?) literal c d e? preface literal a b | split"),
        vec!["a", "b", "c d e"]
    );
}

#[test]
fn append_book_self_escape() {
    // p.264.
    assert_eq!(
        run_spec_collect("literal c d e| append literal a b || split"),
        vec!["c d e", "a", "b"]
    );
}

#[test]
fn append_book_alt_separator() {
    assert_eq!(
        run_spec_collect("(sep ?) literal c d e? append literal a b | split"),
        vec!["c d e", "a", "b"]
    );
}

#[test]
fn append_literal_trailer() {
    assert_eq!(
        run_spec_collect("literal c d e| append literal *** End of data ***"),
        vec!["c d e", "*** End of data ***"]
    );
}

#[test]
fn preface_single_stage() {
    assert_eq!(
        run_spec_collect("literal body| preface literal head"),
        vec!["head", "body"]
    );
}

#[test]
fn nested_does_not_split_primary() {
    assert_eq!(
        run_spec_collect("literal c d e| preface literal xyz"),
        vec!["xyz", "c d e"]
    );
}

#[test]
fn empty_argument_rejected() {
    let err = run_spec("literal a | preface | hole").unwrap_err();
    assert!(
        err.to_string().contains("PREFACE") && err.to_string().contains("required"),
        "{err}"
    );
    let err = run_spec("literal a | append | hole").unwrap_err();
    assert!(
        err.to_string().contains("APPEND") && err.to_string().contains("required"),
        "{err}"
    );
}

#[test]
fn explicit_connector_rejected() {
    let err = run_spec_collect_err("literal a | preface literal x || *:");
    assert!(err.contains("connector"), "{err}");
}

#[test]
fn unknown_nested_stage_fails() {
    let err = run_spec("literal a | preface nosuch | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") || err.to_string().contains("PREFACE"),
        "{err}"
    );
}

#[test]
fn no_invented_abbreviation() {
    let reg = StageRegistry::default();
    assert!(reg.contains("PREFACE"));
    assert!(!reg.contains("PREF"));
    assert!(reg.contains("APPEND"));
    assert!(!reg.contains("APP"));
    let err = run_spec("literal a | pref literal x | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
    let err = run_spec("literal a | app literal x | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn null_records() {
    assert_eq!(
        run_spec_collect("literal| preface literal"),
        vec!["".to_string(), "".to_string()]
    );
}

#[test]
fn secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "PREFACE",
        reg.create("PREFACE", vec!["literal".into(), "h".into()])
            .unwrap(),
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
    assert!(err.to_string().contains("secondary input"), "{err}");
}

#[test]
fn disconnected_output_does_not_run_nested() {
    let reg = StageRegistry::default();
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["one", "two"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "APPEND",
        reg.create("APPEND", vec!["literal".into(), "tail".into()])
            .unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert_eq!(written.get(), 0, "APPEND should short unread input");
}
