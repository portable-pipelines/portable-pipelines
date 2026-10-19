//! NOT — Author's Edition 1.1.11 pp.441–442.

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

fn run_both(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "NOT",
        reg.create("NOT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing NOT"),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

fn run_primary_only(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "NOT",
        reg.create("NOT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing NOT"),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
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

#[test]
fn not_locate_inverts_streams() {
    // User-requested book-style: `not locate /x/`.
    let recs = &["ax", "b", "x", "keep"];
    let (p, s) = run_both(recs, &["locate", "/x/"]);
    assert_eq!(p, vec!["b", "keep"]);
    assert_eq!(s, vec!["ax", "x"]);
}

#[test]
fn not_locate_primary_only_discards_selected() {
    let out = run_primary_only(&["ax", "b", "x"], &["locate", "/x/"]);
    assert_eq!(out, vec!["b"]);
}

#[test]
fn not_locate_matches_nlocate() {
    let recs = &["ax", "b", "x"];
    let (np, ns) = run_both(recs, &["locate", "/x/"]);
    assert_eq!(np, vec!["b"]);
    assert_eq!(ns, vec!["ax", "x"]);
    assert_eq!(run_spec_collect("literal ax b x | split | nlocate /x/"), np);
}

#[test]
fn not_nlocate_is_locate() {
    let recs = &["ax", "b", "x"];
    let (p, s) = run_both(recs, &["nlocate", "/x/"]);
    assert_eq!(p, vec!["ax", "x"]);
    assert_eq!(s, vec!["b"]);
}

#[test]
fn not_chop_period_book_example() {
    // p.441: `not chop .` — “the part of a record after the first period”.
    // CHOP default is BEFORE the first `.` (p.289): prefix on nested primary,
    // remainder (including the period) on nested secondary. NOT swaps them.
    let recs = &["a.b.c", "no-dot", "x."];
    let (p, s) = run_both(recs, &["chop", "."]);
    assert_eq!(p, vec![".b.c", "", "."]);
    assert_eq!(s, vec!["a", "no-dot", "x"]);
}

#[test]
fn not_chop_period_end_to_end() {
    let out = run_spec_collect("literal a.b.c| not chop .");
    assert_eq!(out, vec![".b.c"]);
}

#[test]
fn not_chop_note1_secondary_before_primary() {
    // p.442 Note 1: not chop writes secondary (prefix) before primary (remainder).
    let recs = &["left.right"];
    let (p, s) = run_both(recs, &["chop", "."]);
    assert_eq!(p, vec![".right"]);
    assert_eq!(s, vec!["left"]);
}

#[test]
fn not_find() {
    let recs = &["abc", "xyz", "abz"];
    let (p, s) = run_both(recs, &["find", "ab"]);
    assert_eq!(p, vec!["xyz"]);
    assert_eq!(s, vec!["abc", "abz"]);
}

#[test]
fn not_take() {
    let recs = &["a", "b", "c", "d"];
    let (p, s) = run_both(recs, &["take", "2"]);
    assert_eq!(p, vec!["c", "d"]);
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn not_between() {
    let recs = &["a", "start", "b", "end", "c"];
    let (p, s) = run_both(recs, &["between", "/start/", "/end/"]);
    assert_eq!(p, vec!["a", "c"]);
    assert_eq!(s, vec!["start", "b", "end"]);
}

#[test]
fn not_copy_is_allowed() {
    // Diagram takes any `word`; COPY only writes primary, so everything
    // lands on NOT secondary (or is discarded when that stream is absent).
    let recs = &["a", "b"];
    let (p, s) = run_both(recs, &["copy"]);
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn not_not_locate_cancels() {
    let recs = &["ax", "b", "x"];
    let (p, s) = run_both(recs, &["not", "locate", "/x/"]);
    assert_eq!(p, vec!["ax", "x"]);
    assert_eq!(s, vec!["b"]);
}

#[test]
fn not_word_and_string_are_parsed_once() {
    // Note 2: parsed once with NOT, not as a nested pipeline spec.
    let parsed = pipe_parser::parse_pipeline("literal x | not locate /abc/ | hole").expect("parse");
    assert_eq!(parsed.pipelines[0][1].verb.to_ascii_uppercase(), "NOT");
    let args: Vec<&str> = parsed.pipelines[0][1]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(args, vec!["locate", "/abc/"]);
}

#[test]
fn not_missing_word_is_rejected() {
    let err = run_spec("literal a | not | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );
}

#[test]
fn not_unknown_nested_stage_is_rejected() {
    let err = run_spec("literal a | not nosuch /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("NOSUCH"),
        "{err}"
    );
}

#[test]
fn not_invented_operand_is_rejected() {
    let err = run_spec("literal a | not forever locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("FOREVER"),
        "{err}"
    );
}

#[test]
fn not_nested_operand_error_is_the_selection_rc() {
    let err = run_spec("literal a | not locate /a/ extra | hole").unwrap_err();
    let t = err.to_string();
    assert!(
        t.contains("unexpected operand") || t.contains("LOCATE"),
        "{err}"
    );
}

#[test]
fn not_verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("NOT"));
    assert!(!reg.contains("NO"));
    assert!(!reg.contains("N"));
    let err = run_spec("literal a | no locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("NO"),
        "{err}"
    );
}

#[test]
fn not_secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "NOT",
        reg.create("NOT", vec!["locate".into(), "/a/".into()])
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
fn not_null_records() {
    let recs = &["", "x", ""];
    let (p, s) = run_both(recs, &["locate", "/x/"]);
    assert_eq!(p, vec!["".to_string(), "".into()]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn not_empty_input() {
    let (p, s) = run_both(&[], &["locate", "/x/"]);
    assert!(p.is_empty());
    assert!(s.is_empty());
}

#[test]
fn not_end_to_end_parser() {
    let collected_p = Rc::new(RefCell::new(Vec::new()));
    let collected_s = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    reg.register(">>P", {
        let c = Rc::clone(&collected_p);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    reg.register(">>S", {
        let c = Rc::clone(&collected_s);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    let spec = "(ENDCHAR ?) literal ax|literal b|literal x| N: not locate /x/ | >>P ? N: | >>S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    // literal cascade is reverse order.
    assert_eq!(*collected_p.borrow(), vec!["b".to_string()]);
    assert_eq!(
        *collected_s.borrow(),
        vec!["x".to_string(), "ax".to_string()]
    );
}

#[test]
fn not_hex_dstring_is_raw_ascii_bytes() {
    let recs = &["ABc", "XYc"];
    let (p, s) = run_both(recs, &["locate", "x4142"]);
    assert_eq!(p, vec!["XYc"]);
    assert_eq!(s, vec!["ABc"]);
}

#[test]
fn not_premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(std::cell::Cell::new(0u32));
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
        "NOT",
        reg.create("NOT", vec!["locate".into(), "/one/".into()])
            .unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "NOT should stop reading when no output is connected, wrote {}",
        written.get()
    );
}
