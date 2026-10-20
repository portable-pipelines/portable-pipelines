//! FRTARGET / TOTARGET — Author's Edition 1.1.11 pp.354–355, 560–561.
//!
//! Argument stage is hosted like NOT / CASEI (word [string], parsed once).

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

fn run_both(verb: &str, records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("{verb}")),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

fn run_primary_only(verb: &str, records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("{verb}")),
    );
    let snk = b.add_stage("p", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

#[test]
fn frtarget_locate_book_example() {
    // p.355: records before the first containing `abc` → secondary;
    // trigger and following → primary.
    let recs = &["a", "xabc", "b", "abc", "c"];
    let (p, s) = run_both("FRTARGET", recs, &["locate", "/abc/"]);
    assert_eq!(p, vec!["xabc", "b", "abc", "c"]);
    assert_eq!(s, vec!["a"]);
}

#[test]
fn frtarget_locate_end_to_end() {
    assert_eq!(
        run_spec_collect("literal c|literal abc|literal b|literal a| frtarget locate /abc/"),
        vec!["abc", "c"]
    );
}

#[test]
fn fromtarget_is_a_synonym() {
    // p.355 Note 1.
    assert_eq!(
        run_spec_collect("literal c|literal abc|literal a| fromtarget locate /abc/"),
        vec!["abc", "c"]
    );
}

#[test]
fn frtarget_find_prefix() {
    let recs = &["a", "abc", "b"];
    let (p, s) = run_both("FRTARGET", recs, &["find", "abc"]);
    assert_eq!(p, vec!["abc", "b"]);
    assert_eq!(s, vec!["a"]);
}

#[test]
fn frtarget_primary_only_discards_rejects() {
    let out = run_primary_only("FRTARGET", &["a", "abc", "b"], &["locate", "/abc/"]);
    assert_eq!(out, vec!["abc", "b"]);
}

#[test]
fn frtarget_no_match_all_rejected() {
    let (p, s) = run_both("FRTARGET", &["a", "b", "c"], &["locate", "/z/"]);
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(s, vec!["a", "b", "c"]);
}

#[test]
fn totarget_locate_book_example() {
    // p.561: records up to the first containing `abc` → primary;
    // trigger and rest → secondary.
    let recs = &["a", "xabc", "b", "c"];
    let (p, s) = run_both("TOTARGET", recs, &["locate", "/abc/"]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec!["xabc", "b", "c"]);
}

#[test]
fn totarget_end_to_end() {
    assert_eq!(
        run_spec_collect("literal c|literal abc|literal b|literal a| totarget locate /abc/"),
        vec!["a", "b"]
    );
}

#[test]
fn totarget_no_match_all_primary() {
    let (p, s) = run_both("TOTARGET", &["a", "b"], &["locate", "/z/"]);
    assert_eq!(p, vec!["a", "b"]);
    assert!(s.is_empty(), "{s:?}");
}

#[test]
fn totarget_primary_only_drops_trigger_and_rest() {
    // Secondary not defined: trigger and remaining are not copied.
    let out = run_primary_only("TOTARGET", &["a", "abc", "b"], &["locate", "/abc/"]);
    assert_eq!(out, vec!["a"]);
}

#[test]
fn frtarget_and_totarget_are_converses_with_locate() {
    let recs = &["keep", "hit", "after"];
    let (fp, fs) = run_both("FRTARGET", recs, &["locate", "/hit/"]);
    let (tp, ts) = run_both("TOTARGET", recs, &["locate", "/hit/"]);
    assert_eq!(fp, ts);
    assert_eq!(fs, tp);
}

#[test]
fn word_and_string_are_parsed_once() {
    // p.355 Note 3 / p.561 Note 2: parsed once with the host, not a nested spec.
    let parsed =
        pipe_parser::parse_pipeline("literal x | frtarget locate /abc/ | hole").expect("parse");
    let args: Vec<&str> = parsed.pipelines[0][1]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(args, vec!["locate", "/abc/"]);
}

#[test]
fn missing_word_is_rejected() {
    let err = run_spec("literal a | frtarget | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );
    let err = run_spec("literal a | totarget | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );
}

#[test]
fn unknown_nested_stage_is_rejected() {
    let err = run_spec("literal a | frtarget nosuch /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("NOSUCH"),
        "{err}"
    );
    let err = run_spec("literal a | totarget collate | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("COLLATE"),
        "{err}"
    );
}

#[test]
fn nested_operand_error_is_the_selection_rc() {
    let err = run_spec("literal a | frtarget locate /a/ extra | hole").unwrap_err();
    let t = err.to_string();
    assert!(
        t.contains("unexpected operand") || t.contains("LOCATE"),
        "{err}"
    );
}

#[test]
fn verbs_are_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("FRTARGET"));
    assert!(reg.contains("FROMTARGET"));
    assert!(reg.contains("TOTARGET"));
    assert!(!reg.contains("FRTARGE"));
    assert!(!reg.contains("FROMTARGE"));
    assert!(!reg.contains("TOTARGE"));
    let err = run_spec("literal a | frtar locate /a/ | hole").unwrap_err();
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
            for rec in ["a", "b", "c"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                w.set(w.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "FRTARGET",
        reg.create("FRTARGET", vec!["locate".into(), "/a/".into()])
            .expect("FRTARGET"),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "FRTARGET should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn nlocate_as_argument_stage() {
    let recs = &["ax", "ay", "b"];
    let (p, s) = run_both("FRTARGET", recs, &["nlocate", "/a/"]);
    assert_eq!(p, vec!["b"]);
    assert_eq!(s, vec!["ax", "ay"]);
}

#[test]
fn totarget_find_prefix() {
    let recs = &["a", "abc", "b"];
    let (p, s) = run_both("TOTARGET", recs, &["find", "abc"]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec!["abc", "b"]);
}

#[test]
fn strfind_as_argument_stage() {
    let recs = &["x", "abcZZ", "y"];
    let (p, s) = run_both("FRTARGET", recs, &["strfind", "/abc/"]);
    assert_eq!(p, vec!["abcZZ", "y"]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn extra_output_stream_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let stage = b.add_stage(
        "FRTARGET",
        reg.create("FRTARGET", vec!["locate".into(), "/a/".into()])
            .expect("FRTARGET"),
    );
    let hole = b.add_stage(
        "hole",
        Box::new(|ctx: &mut StageCtx| while ctx.read().ok().flatten().is_some() {}),
    );
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, hole, StreamId::PRIMARY);
    b.connect(stage, StreamId::TERTIARY, hole, StreamId::SECONDARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string().to_ascii_uppercase().contains("FRTARGET")
            || err.to_string().contains("at most two"),
        "{err}"
    );
}
