//! DUPLICATE — Author's Edition 1.1.11 p.334 (tutorial pp.148–149, delay p.322).

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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "DUPLICATE",
        reg.create("DUPLICATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing DUPLICATE"),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn run_primary_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "DUPLICATE",
        reg.create("DUPLICATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing DUPLICATE"),
    );
    let sink_p = b.add_stage("sink_p", sink_bytes(p));
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

fn run_counted(records: &[&str], args: &[&str]) -> (Vec<String>, u32) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let recs: Vec<String> = records.iter().map(|s| (*s).to_string()).collect();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in recs {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "DUPLICATE",
        reg.create("DUPLICATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing DUPLICATE"),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let out = primary.borrow().clone();
    (out, written.get())
}

#[test]
fn duplicate_default_writes_twice() {
    // p.334: default additional count is 1 (write each input record twice).
    assert_eq!(run_primary(&["a", "b"], &[]), vec!["a", "a", "b", "b"]);
}

#[test]
fn duplicate_book_figure_245_star() {
    // p.148 Figure 245: `literal +60 | duplicate * | …`
    // Bounded with TAKE so the infinite supply stops after three records.
    // Spaces around `|` after a LITERAL argument are a trailing blank (p.412);
    // write the argument flush against the separator so the record is `+60`.
    assert_eq!(
        run_spec_collect("literal +60|duplicate * | take 3"),
        vec!["+60", "+60", "+60"]
    );
}

#[test]
fn duplicate_book_dup_star_abbreviation() {
    // p.334 printed example `...| dup * |...` and delay p.322 `dup *`.
    assert_eq!(
        run_spec_collect("literal +60|dup * | take 2"),
        vec!["+60", "+60"]
    );
}

#[test]
fn duplicate_minus_one_consumes_and_writes_nothing() {
    // p.334 Note 1: `duplicate -1` consumes all input and produces no output.
    let (out, n) = run_counted(&["a", "b", "c"], &["-1"]);
    assert!(out.is_empty(), "{out:?}");
    assert_eq!(n, 3);
}

#[test]
fn duplicate_zero_is_pass_through() {
    // 0 additional copies: write each record once. Number must be -1 or larger.
    assert_eq!(run_primary(&["a", "b"], &["0"]), vec!["a", "b"]);
}

#[test]
fn duplicate_two_writes_three() {
    assert_eq!(run_primary(&["x"], &["2"]), vec!["x", "x", "x"]);
}

#[test]
fn duplicate_explicit_one_matches_default() {
    assert_eq!(run_primary(&["z"], &["1"]), vec!["z", "z"]);
}

#[test]
fn duplicate_star_copies_only_the_first_record() {
    // p.334: asterisk = infinite copies of the *first* input record.
    // `literal a|literal b` yields b then a (cascade is reverse order).
    assert_eq!(
        run_spec_collect("literal a|literal b|duplicate * | take 4"),
        vec!["b", "b", "b", "b"]
    );
    let (out, n) = {
        let reg = StageRegistry::default();
        let primary = Rc::new(RefCell::new(Vec::new()));
        let p = Rc::clone(&primary);
        let written = Rc::new(Cell::new(0u32));
        let count = Rc::clone(&written);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(move |ctx: &mut StageCtx| {
                for rec in ["first", "second", "third"] {
                    if ctx.write(Record::from(rec)).is_err() {
                        break;
                    }
                    count.set(count.get() + 1);
                }
            }),
        );
        let dup = b.add_stage(
            "DUPLICATE",
            reg.create("DUPLICATE", vec!["*".into()]).unwrap(),
        );
        let take = b.add_stage("TAKE", reg.create("TAKE", vec!["3".into()]).unwrap());
        let sink_p = b.add_stage("sink_p", sink(p));
        b.connect_primary(src, dup);
        b.connect_primary(dup, take);
        b.connect_primary(take, sink_p);
        b.run().unwrap();
        let out = primary.borrow().clone();
        (out, written.get())
    };
    assert_eq!(out, vec!["first", "first", "first"]);
    assert!(
        !out.iter().any(|s| s == "second" || s == "third"),
        "star copies only the first record, got {out:?}"
    );
    assert!(
        n <= 2,
        "at most the first record plus one in-flight write, wrote {n}"
    );
}

#[test]
fn duplicate_star_with_no_input_writes_nothing() {
    assert!(run_primary(&[], &["*"]).is_empty());
}

#[test]
fn duplicate_null_records() {
    let empty = Vec::new();
    assert_eq!(
        run_primary_bytes(vec![empty.clone()], &[]),
        vec![empty.clone(), empty.clone()]
    );
    assert_eq!(
        run_primary_bytes(vec![empty.clone(), b"x".to_vec(), empty.clone()], &["0"]),
        vec![empty.clone(), b"x".to_vec(), empty]
    );
    let (out, n) = run_counted(&["", "a", ""], &["-1"]);
    assert!(out.is_empty());
    assert_eq!(n, 3);
    assert_eq!(run_spec_collect("literal|duplicate | take 2"), vec!["", ""]);
}

#[test]
fn duplicate_empty_input_writes_nothing() {
    assert!(run_primary(&[], &[]).is_empty());
    assert!(run_primary(&[], &["5"]).is_empty());
}

#[test]
fn duplicate_verb_abbrev_min_3() {
    let reg = StageRegistry::default();
    assert!(reg.contains("DUP"));
    assert!(reg.contains("DUPL"));
    assert!(reg.contains("DUPLICATE"));
    assert!(!reg.contains("DU"));
    assert_eq!(run_spec_collect("literal a|dup"), vec!["a", "a"]);
    let err = run_spec("literal a | du | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("DU"),
        "{err}"
    );
}

#[test]
fn duplicate_end_to_end_through_parser() {
    run_spec("literal a | duplicate | hole").unwrap();
    run_spec("literal a | dup 0 | hole").unwrap();
    run_spec("literal a | duplicate -1 | hole").unwrap();
    run_spec("literal a | duplicate * | take 1 | hole").unwrap();
    run_spec("literal a|literal b | duplicate 2 | hole").unwrap();
}

#[test]
fn duplicate_operand_errors() {
    let err = run_spec("literal a | duplicate -2 | hole").unwrap_err();
    assert!(
        err.to_string().contains("-1") || err.to_string().contains("larger"),
        "{err}"
    );

    let err = run_spec("literal a | duplicate +1 | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | duplicate foo | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | duplicate 1 2 | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | duplicate * 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | duplicate first | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | duplicate anycase | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | duplicate count | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");
}

#[test]
fn duplicate_premature_termination_without_output() {
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
    let stage = b.add_stage("DUPLICATE", reg.create("DUPLICATE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "DUPLICATE should stop reading when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn duplicate_secondary_output_is_unused() {
    // p.334 omits Streams Used; write primary only. A connected secondary is unused.
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let stage = b.add_stage("DUPLICATE", reg.create("DUPLICATE", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(p));
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["a".to_string(), "a".to_string()]);
    assert!(secondary.borrow().is_empty());
}
