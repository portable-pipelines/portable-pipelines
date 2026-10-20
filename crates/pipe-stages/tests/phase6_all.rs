//! ALL — Author's Edition 1.1.11 pp.259–260.
//!
//! Boolean of LOCATE substring tests. Portable ASCII: the book's `x4f`
//! vertical bar is EBCDIC X'4F'; ASCII `|` is `x7c`. The not sign `¬` is
//! UTF-8 U+00AC; `^` and `\` are accepted as PICK Note 1 (p.456) mappings.

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
        "ALL",
        reg.create("ALL", args.iter().map(|a| (*a).to_string()).collect())
            .expect("ALL"),
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
fn book_or_string_or_vertical_bar() {
    // p.259: `all /abc/ ! x4f` — portable ASCII bar is x7c.
    let recs = ["xxabc", "a|b", "xyz", "abc|"];
    let (p, s) = run_split(&recs, &["/abc/", "!", "x7c"]);
    assert_eq!(p, vec!["xxabc", "a|b", "abc|"]);
    assert_eq!(s, vec!["xyz"]);
}

#[test]
fn book_grouped_or_and_exclamation() {
    // p.260: `all (/abc/ ! /def/) & /!/`
    let recs = ["abc!", "def!", "abc", "!", "xyz!", "abcdef!"];
    let (p, s) = run_split(&recs, &["(", "/abc/", "!", "/def/", ")", "&", "/!/"]);
    assert_eq!(p, vec!["abc!", "def!", "abcdef!"]);
    assert_eq!(s, vec!["abc", "!", "xyz!"]);
}

#[test]
fn spec_form_with_blanks() {
    // LITERAL cascade is reverse order (p.413).
    assert_eq!(
        run_spec_collect("literal abc|literal def|literal xyz| all /abc/ ! /def/"),
        vec!["def", "abc"]
    );
}

#[test]
fn glued_or_without_blanks() {
    assert_eq!(
        run_spec_collect("literal abc|literal def|literal xyz| all /abc/!/def/"),
        vec!["def", "abc"]
    );
}

#[test]
fn self_escaped_bar_is_or() {
    // `||` is one ordinary `|` in the argument (p.114).
    assert_eq!(
        run_spec_collect("literal abc|literal def|literal xyz| all /abc/ || /def/"),
        vec!["def", "abc"]
    );
}

#[test]
fn not_sign_utf8_caret_and_backslash() {
    let recs = ["abc", "xyz"];
    for not in ["\u{00AC}", "^", "\\"] {
        let (p, s) = run_split(&recs, &[not, "/abc/"]);
        assert_eq!(p, vec!["xyz"], "not={not}");
        assert_eq!(s, vec!["abc"], "not={not}");
    }
}

#[test]
fn and_requires_every_term() {
    let recs = ["ab", "a", "b", "ba", "c"];
    let (p, s) = run_split(&recs, &["/a/", "&", "/b/"]);
    assert_eq!(p, vec!["ab", "ba"]);
    assert_eq!(s, vec!["a", "b", "c"]);
}

#[test]
fn precedence_and_tighter_than_or() {
    // a & b ! c  ≡  (a & b) or c
    let recs = ["ab", "c", "a", "b"];
    let (p, s) = run_split(&recs, &["/a/", "&", "/b/", "!", "/c/"]);
    assert_eq!(p, vec!["ab", "c"]);
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn grouping_overrides_precedence() {
    let recs = ["ac", "bc", "ab", "c"];
    let (p, s) = run_split(&recs, &["(", "/a/", "!", "/b/", ")", "&", "/c/"]);
    assert_eq!(p, vec!["ac", "bc"]);
    assert_eq!(s, vec!["ab", "c"]);
}

#[test]
fn null_dstring_like_locate() {
    let recs = ["", "a"];
    let (p, s) = run_split(&recs, &["//"]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec![""]);
}

#[test]
fn hex_and_binary_dstrings() {
    let (p, s) = run_split(&["AB", "A", "C"], &["x4142"]);
    assert_eq!(p, vec!["AB"]);
    assert_eq!(s, vec!["A", "C"]);
    let (p, _) = run_split(&["A"], &["b01000001"]);
    assert_eq!(p, vec!["A"]);
}

#[test]
fn contains_not_prefix() {
    let (p, s) = run_split(&["xxabc", "abc", "ab"], &["/abc/"]);
    assert_eq!(p, vec!["xxabc", "abc"]);
    assert_eq!(s, vec!["ab"]);
}

#[test]
fn not_all_is_the_converse() {
    assert_eq!(
        run_spec_collect("literal abc|literal xyz| not all /abc/"),
        vec!["xyz"]
    );
}

#[test]
fn missing_expression_rejected() {
    let err = err_of("literal a | all | hole");
    assert!(err.to_ascii_uppercase().contains("EXPRESSION"), "{err}");
}

#[test]
fn debug_dump_rejected() {
    let err = err_of("literal a | all %debug /a/ | hole");
    assert!(err.contains("%debug"), "{err}");
    let err = err_of("literal a | all %dump /a/ | hole");
    assert!(err.contains("%dump"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | all anycase /a/ | hole",
        "literal a | all /a/ extra | hole",
        "literal a | all first /a/ | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected") || err.contains("extra"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn unclosed_paren_rejected() {
    let err = err_of("literal a | all (/abc/ | hole");
    assert!(
        err.contains("parenthes") || err.contains("unrecognized") || err.contains("expression"),
        "{err}"
    );
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("ALL"));
    assert!(!reg.contains("AL"));
    let err = err_of("literal a | al /a/ | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("AL"),
        "{err}"
    );
}

#[test]
fn secondary_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("ALL", reg.create("ALL", vec!["/a/".into()]).unwrap());
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
    let stage = b.add_stage("ALL", reg.create("ALL", vec!["/o/".into()]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "ALL should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn discards_when_only_the_other_stream_is_connected() {
    let reg = StageRegistry::default();
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc", "xyz"]));
    let stage = b.add_stage("ALL", reg.create("ALL", vec!["/abc/".into()]).unwrap());
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*secondary.borrow(), vec!["xyz".to_string()]);
}

#[test]
fn secondary_gets_rejected_records() {
    let (p, s) = run_split(&["abc", "xyz"], &["/abc/"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["xyz"]);
}
