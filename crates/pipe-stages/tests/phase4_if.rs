//! IF — Author's Edition 1.1.11 pp.389–390.

use std::cell::RefCell;
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
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn run_spec_both(spec: &str) -> (Vec<String>, Vec<String>) {
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    {
        let p = Rc::clone(&primary);
        reg.register(">>P", move |_| pipe_stages::collect_to(Rc::clone(&p)));
    }
    {
        let s = Rc::clone(&secondary);
        reg.register(">>S", move |_| pipe_stages::collect_to(Rc::clone(&s)));
    }
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

/// if-then topology: src → IF.0 → then-path → IF.1 → sink.
fn run_if_then(
    records: &[&str],
    if_args: &[&str],
    then_verb: &str,
    then_args: &[&str],
) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let iff = b.add_stage(
        "IF",
        reg.create("IF", if_args.iter().map(|a| (*a).to_string()).collect())
            .expect("IF"),
    );
    let then = b.add_stage(
        then_verb,
        reg.create(
            then_verb,
            then_args.iter().map(|a| (*a).to_string()).collect(),
        )
        .unwrap_or_else(|| panic!("missing {then_verb}")),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src, StreamId::PRIMARY, iff, StreamId::PRIMARY);
    b.connect(iff, StreamId::PRIMARY, then, StreamId::PRIMARY);
    b.connect(then, StreamId::PRIMARY, iff, StreamId::SECONDARY);
    b.connect(iff, StreamId::SECONDARY, snk, StreamId::PRIMARY);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn run_if_then_else(
    records: &[&str],
    if_args: &[&str],
    then_verb: &str,
    then_args: &[&str],
    else_verb: &str,
    else_args: &[&str],
) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let iff = b.add_stage(
        "IF",
        reg.create("IF", if_args.iter().map(|a| (*a).to_string()).collect())
            .expect("IF"),
    );
    let then = b.add_stage(
        then_verb,
        reg.create(
            then_verb,
            then_args.iter().map(|a| (*a).to_string()).collect(),
        )
        .unwrap_or_else(|| panic!("missing {then_verb}")),
    );
    let els = b.add_stage(
        else_verb,
        reg.create(
            else_verb,
            else_args.iter().map(|a| (*a).to_string()).collect(),
        )
        .unwrap_or_else(|| panic!("missing {else_verb}")),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src, StreamId::PRIMARY, iff, StreamId::PRIMARY);
    b.connect(iff, StreamId::PRIMARY, then, StreamId::PRIMARY);
    b.connect(then, StreamId::PRIMARY, iff, StreamId::SECONDARY);
    b.connect(iff, StreamId::SECONDARY, els, StreamId::PRIMARY);
    b.connect(els, StreamId::PRIMARY, iff, StreamId::TERTIARY);
    b.connect(iff, StreamId::TERTIARY, snk, StreamId::PRIMARY);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn if_then_locate_copy_preserves_order() {
    // p.390: if strictly does not delay; a non-delaying then-path keeps order.
    let out = run_if_then(&["a", "abc", "b"], &["locate", "/abc/"], "COPY", &[]);
    assert_eq!(out, vec!["a", "abc", "b"]);
}

#[test]
fn if_then_selected_are_transformed() {
    // Selected records go through the then-path; others merge unchanged.
    let out = run_if_then(
        &["upA", "keep", "upB", "skip"],
        &["locate", "/up/"],
        "SPEC",
        &["3-*", "1"],
    );
    assert_eq!(out, vec!["A", "keep", "B", "skip"]);
}

#[test]
fn if_then_else_book_mark_and_shift() {
    // p.390: locate /abc/ then insert /-> / else insert three spaces.
    // INSERT is Phase 6; CHANGE with a null old string prefixes (p.287).
    let out = run_if_then_else(
        &["a", "abc", "b"],
        &["locate", "/abc/"],
        "CHANGE",
        &["//-> /"],
        "CHANGE",
        &["//   /"],
    );
    assert_eq!(out, vec!["   a", "-> abc", "   b"]);
}

#[test]
fn if_then_dup_merges_extra_then_records() {
    // Rejected records merge with the secondary input (then-path return).
    // DUPLICATE delays the extra copy (p.334), so FANINANY may emit a later
    // rejected record before that copy (p.390: order is preserved only when
    // the then-path also does not delay).
    let out = run_if_then(&["a", "b", "c"], &["locate", "/b/"], "DUPLICATE", &[]);
    assert_eq!(out, vec!["a", "b", "c", "b"]);
    assert_eq!(out.iter().filter(|s| *s == "b").count(), 2);
}

#[test]
fn if_then_nlocate() {
    let out = run_if_then(&["ax", "b", "x"], &["nlocate", "/x/"], "COPY", &[]);
    assert_eq!(out, vec!["ax", "b", "x"]);
}

#[test]
fn if_then_take() {
    let out = run_if_then(&["a", "b", "c", "d"], &["take", "2"], "COPY", &[]);
    assert_eq!(out, vec!["a", "b", "c", "d"]);
}

#[test]
fn if_then_pick() {
    let out = run_if_then(
        &["aa", "ab", "bb"],
        &["pick", "1-*", "==", "/aa/"],
        "COPY",
        &[],
    );
    assert_eq!(out, vec!["aa", "ab", "bb"]);
}

#[test]
fn if_note1_copy_is_allowed() {
    // Note 1: IF does not enforce that the argument is a selection stage.
    let out = run_if_then(&["a", "b"], &["copy"], "COPY", &[]);
    assert_eq!(out, vec!["a", "b"]);
}

#[test]
fn if_then_null_records() {
    let out = run_if_then(&["", "x", ""], &["locate", "/x/"], "COPY", &[]);
    assert_eq!(out, vec!["", "x", ""]);
}

#[test]
fn if_then_empty_input() {
    let out = run_if_then(&[], &["locate", "/x/"], "COPY", &[]);
    assert!(out.is_empty());
}

#[test]
fn if_then_end_to_end_parser() {
    let out = run_spec_collect(
        "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | copy | if1: | >>COLLECT",
    );
    assert_eq!(out, vec!["a", "abc", "b"]);
}

#[test]
fn if_then_else_end_to_end_parser() {
    let out = run_spec_collect(
        "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | change // /-> / | if1: | change // /   / | if1: | >>COLLECT",
    );
    assert_eq!(out, vec!["   a", "-> abc", "   b"]);
}

#[test]
fn if_then_spec_drop_prefix() {
    let out = run_spec_collect(
        "(ENDCHAR ?) literal upA keep upB skip | split | if1: if locate /up/ | spec 3-* 1 | if1: | >>COLLECT",
    );
    assert_eq!(out, vec!["A", "keep", "B", "skip"]);
}

#[test]
fn if_word_and_string_are_parsed_once() {
    // Note 2 analogue (ZONE p.610): the nested program is an argument, not a
    // nested pipeline spec. Tokens after the word belong to that stage.
    let parsed = pipe_parser::parse_pipeline("literal x | if locate /abc/ | hole").expect("parse");
    assert_eq!(parsed.pipelines[0][1].verb.to_ascii_uppercase(), "IF");
    let args: Vec<&str> = parsed.pipelines[0][1]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(args, vec!["locate", "/abc/"]);
}

#[test]
fn if_missing_word_is_rejected() {
    let err = run_spec("literal a | if | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );
}

#[test]
fn if_unknown_nested_stage_is_rejected() {
    let err = run_spec("literal a | if nosuch /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("NOSUCH"),
        "{err}"
    );
}

#[test]
fn if_invented_then_keyword_is_rejected() {
    // THEN / ELSE are not IF operands; the first token is the nested verb.
    let err = run_spec("literal a | if then locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("THEN"),
        "{err}"
    );
}

#[test]
fn if_fillup_is_unknown() {
    let err = run_spec("literal a | if fillup | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("FILLUP"),
        "{err}"
    );
}

#[test]
fn if_strfind_selects() {
    run_spec("(ENDCHAR ?) literal a abc b | split | if1: if strfind /abc/ | copy | if1: | hole")
        .unwrap();
}

#[test]
fn if_nested_operand_error_is_the_selection_rc() {
    // p.390 Return Codes: unless IF itself issues a message, the return
    // code is the one from the selection stage.
    let err = run_spec("(ENDCHAR ?) literal a | if1: if locate /a/ extra | copy | if1: | hole")
        .unwrap_err();
    let t = err.to_string();
    assert!(
        t.contains("unexpected operand") || t.contains("LOCATE"),
        "{err}"
    );
}

#[test]
fn if_primary_only_is_rejected() {
    let err = run_spec("literal a | if locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("two streams must be defined"),
        "{err}"
    );
}

#[test]
fn if_four_streams_are_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src0 = b.add_stage("src0", emit(&["a"]));
    let src1 = b.add_stage("src1", emit(&[]));
    let src2 = b.add_stage("src2", emit(&[]));
    let src3 = b.add_stage("src3", emit(&[]));
    let iff = b.add_stage(
        "IF",
        reg.create("IF", vec!["locate".into(), "/a/".into()])
            .expect("IF"),
    );
    let hole = b.add_stage(
        "hole",
        Box::new(|ctx: &mut StageCtx| {
            for s in ctx.connected_inputs() {
                ctx.select_input(s);
                while ctx.read().ok().flatten().is_some() {}
            }
        }),
    );
    b.connect(src0, StreamId::PRIMARY, iff, StreamId::PRIMARY);
    b.connect(src1, StreamId::PRIMARY, iff, StreamId::SECONDARY);
    b.connect(src2, StreamId::PRIMARY, iff, StreamId::TERTIARY);
    b.connect(src3, StreamId::PRIMARY, iff, StreamId(3));
    b.connect(iff, StreamId::PRIMARY, hole, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("at most three streams"), "{err}");
}

#[test]
fn if_verb_is_not_abbreviable() {
    // Diagram is all-caps IF (p.389); `i` is not a verb abbreviation.
    let reg = StageRegistry::default();
    assert!(reg.contains("IF"));
    assert!(!reg.contains("I"));
    let err = run_spec("literal a | i locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("I"),
        "{err}"
    );
}

#[test]
fn if_hex_dstring_is_raw_ascii_bytes() {
    let out = run_if_then(&["ABc", "XYc"], &["locate", "x4142"], "COPY", &[]);
    assert_eq!(out, vec!["ABc", "XYc"]);
}

#[test]
fn if_then_else_identity_both_paths() {
    let out = run_if_then_else(
        &["no", "yes", "no"],
        &["locate", "/yes/"],
        "COPY",
        &[],
        "COPY",
        &[],
    );
    assert_eq!(out, vec!["no", "yes", "no"]);
}

#[test]
fn if_then_hole_discards_selected() {
    // Selected records are consumed by the then-path; rejected still merge.
    let out = run_spec_collect(
        "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | hole | if1: | >>COLLECT",
    );
    assert_eq!(out, vec!["a", "b"]);
}

#[test]
fn if_label_pair_secondary_is_not_a_second_output_file() {
    // if-then: the first label reference is the merge output, not a
    // selection-style secondary of leftover records.
    let (p, s) = run_spec_both(
        "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | copy | if1: | >>P ? >>S",
    );
    assert_eq!(p, vec!["a", "abc", "b"]);
    assert!(s.is_empty(), "unexpected secondary {s:?}");
}
