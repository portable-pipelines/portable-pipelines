//! ESCAPE — Author's Edition 1.1.11 pp.337–338.
//!
//! Insert the first character of the argument in front of every listed
//! character (including the escape itself). Default `/"\|/` is quote,
//! backslant, vertical bar. Filter: primary in / primary out.

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

fn run_through(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "ESCAPE",
        reg.create("ESCAPE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("ESCAPE"),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn book_default_doubles_quotes() {
    // p.338: `This: "a beautiful day"` → `This: ""a beautiful day""`.
    assert_eq!(
        run_through(&[r#"This: "a beautiful day""#], &[]),
        vec![r#"This: ""a beautiful day"""#]
    );
}

#[test]
fn default_escapes_backslash_and_bar() {
    assert_eq!(run_through(&[r"path\to|x"], &[]), vec![r#"path"\to"|x"#]);
}

#[test]
fn listed_characters_prefixed_with_first() {
    assert_eq!(run_through(&["hello"], &["/l/"]), vec!["hellllo"]);
    assert_eq!(run_through(&["abc"], &["/a/"]), vec!["aabc"]);
}

#[test]
fn does_not_reescape_the_inserted_character() {
    // Insert then keep the original byte; the new escape is not scanned again.
    assert_eq!(run_through(&["aa"], &["/a/"]), vec!["aaaa"]);
}

#[test]
fn hex_and_binary_dstrings() {
    // x61 = 'a'; prefix 'a' before 'a'.
    assert_eq!(run_through(&["a"], &["x61"]), vec!["aa"]);
    assert_eq!(run_through(&["A"], &["b01000001"]), vec!["AA"]);
}

#[test]
fn spec_form_default() {
    assert_eq!(
        run_spec_collect(r#"literal hello| escape /l/"#),
        vec!["hellllo"]
    );
}

#[test]
fn change_built_book_record() {
    // Build the book's quoted record without fighting the scanner's quotes.
    assert_eq!(
        run_spec_collect(r#"literal x| change /x/ %This: "day"% | escape"#),
        vec![r#"This: ""day"""#]
    );
}

#[test]
fn null_dstring_rejected() {
    let err = err_of("literal a | escape // | hole");
    assert!(err.contains("escape character"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | escape /a/ extra | hole",
        "literal a | escape ANYCASE /a/ | hole",
        "literal a | escape first | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("ESCAPE"));
    assert!(!reg.contains("ESCAP"));
    assert!(!reg.contains("ESC"));
    let err = err_of("literal a | escap /a/ | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("ESCAP"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("ESCAPE", reg.create("ESCAPE", vec![]).unwrap());
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
    assert!(
        err.to_string()
            .contains("no input stream other than the primary"),
        "{err}"
    );
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
    let stage = b.add_stage("ESCAPE", reg.create("ESCAPE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "ESCAPE should stop when no output is connected, wrote {}",
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
    let stage = b.add_stage("ESCAPE", reg.create("ESCAPE", vec!["/a/".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["aab".to_string()]);
    assert!(secondary.borrow().is_empty());
}
