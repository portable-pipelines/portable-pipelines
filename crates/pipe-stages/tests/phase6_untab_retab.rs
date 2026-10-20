//! UNTAB / RETAB — Author's Edition 1.1.11 pp.575 / 473.
//!
//! Portable ASCII: tabulate is HT X'09' (book cites EBCDIC X'05'); blank is
//! X'20' (book cites X'40'). Hex dstrings are raw bytes.

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

fn run_bytes(verb: &str, args: &[&str], records: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .expect(verb),
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

fn tab_after(text: &str, col: usize) -> String {
    // Insert HT after 1-based column `col` via INSERT (p.392).
    format!("literal {text}| insert x09 after {col}")
}

#[test]
fn default_minus_three_untab() {
    // p.575: default `-3` ≡ 1 4 7 …; tab at column 2 expands to column 4.
    assert_eq!(
        run_spec_collect(&format!("{} | untab", tab_after("ab", 1))),
        vec!["a  b"]
    );
    assert_eq!(
        run_spec_collect(&format!("{} | untab -3", tab_after("ab", 1))),
        vec!["a  b"]
    );
}

#[test]
fn default_minus_three_retab() {
    // p.473: converse of UNTAB. Two blanks from column 2 reach stop 4.
    assert_eq!(
        run_bytes("RETAB", &[], vec![b"a  b".to_vec()]),
        vec![b"a\tb".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &["-3"], vec![b"a  b".to_vec()]),
        vec![b"a\tb".to_vec()]
    );
}

#[test]
fn minus_eight_unix_style_book_example() {
    // p.473: `retab -8` (Unix file stand-in). Stops 1, 9, 17, …
    assert_eq!(
        run_bytes("UNTAB", &["-8"], vec![b"a\tb".to_vec()]),
        vec![b"a       b".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &["-8"], vec![b"a       b".to_vec()]),
        vec![b"a\tb".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &["-8"], vec![b"a  b".to_vec()]),
        vec![b"a  b".to_vec()]
    );
}

#[test]
fn minus_one_every_column() {
    assert_eq!(
        run_bytes("UNTAB", &["-1"], vec![b"a\tb".to_vec()]),
        vec![b"a b".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &["-1"], vec![b"a b".to_vec()]),
        vec![b"a\tb".to_vec()]
    );
}

#[test]
fn positive_list_any_order() {
    // p.473 / p.575: numbers may be in any order.
    let ordered = run_bytes("UNTAB", &["1", "4", "7"], vec![b"a\tb".to_vec()]);
    let shuffled = run_bytes("UNTAB", &["7", "1", "4"], vec![b"a\tb".to_vec()]);
    assert_eq!(ordered, vec![b"a  b".to_vec()]);
    assert_eq!(shuffled, ordered);
    assert_eq!(
        run_bytes("RETAB", &["7", "4", "1"], vec![b"a  b".to_vec()]),
        vec![b"a\tb".to_vec()]
    );
}

#[test]
fn finite_list_is_not_the_infinite_default() {
    // `-3` continues past 7; `1 4 7` does not.
    let rec = {
        let mut v = vec![b'x'; 7];
        v.push(b'\t');
        v.push(b'y');
        v
    };
    assert_eq!(
        run_bytes("UNTAB", &["-3"], vec![rec.clone()]),
        vec![b"xxxxxxx  y".to_vec()]
    );
    assert_eq!(
        run_bytes("UNTAB", &["1", "4", "7"], vec![rec]),
        vec![b"xxxxxxx y".to_vec()]
    );
}

#[test]
fn tab_past_last_stop_is_one_blank() {
    assert_eq!(
        run_bytes("UNTAB", &["1"], vec![b"ab\tc".to_vec()]),
        vec![b"ab c".to_vec()]
    );
    assert_eq!(
        run_bytes("UNTAB", &["1"], vec![b"\t".to_vec()]),
        vec![b" ".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &["1"], vec![b"ab c".to_vec()]),
        vec![b"ab c".to_vec()]
    );
}

#[test]
fn left_margin_discards_prefix_on_retab() {
    // p.473: smallest number is the left margin; discard columns before it.
    assert_eq!(
        run_bytes("RETAB", &["5", "10"], vec![b"ABCD     X".to_vec()]),
        vec![b"\tX".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &["5", "10"], vec![b"ABCD    X".to_vec()]),
        vec![b"    X".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &["5", "10"], vec![b"ab".to_vec()]),
        vec![b"".to_vec()]
    );
}

#[test]
fn untab_does_not_discard_left_margin() {
    // UNTAB has the same stop syntax but no discard Operation paragraph.
    assert_eq!(
        run_bytes("UNTAB", &["5", "10"], vec![b"\tX".to_vec()]),
        vec![b"    X".to_vec()]
    );
    assert_eq!(
        run_bytes("UNTAB", &["5", "10"], vec![b"AB\tX".to_vec()]),
        vec![b"AB  X".to_vec()]
    );
}

#[test]
fn empty_records() {
    assert_eq!(run_spec_collect("literal| untab"), vec![""]);
    assert_eq!(run_spec_collect("literal| retab"), vec![""]);
    assert_eq!(
        run_bytes("UNTAB", &[], vec![b"".to_vec(), b"a\tb".to_vec()]),
        vec![b"".to_vec(), b"a  b".to_vec()]
    );
}

#[test]
fn trailing_blanks() {
    assert_eq!(
        run_bytes("UNTAB", &[], vec![b"a\t".to_vec()]),
        vec![b"a  ".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &[], vec![b"a  ".to_vec()]),
        vec![b"a\t".to_vec()]
    );
    // Trailing blanks that do not reach a stop stay blanks (`-8`).
    assert_eq!(
        run_bytes("RETAB", &["-8"], vec![b"a  ".to_vec()]),
        vec![b"a  ".to_vec()]
    );
}

#[test]
fn no_tabs_untab_is_identity() {
    assert_eq!(run_spec_collect("literal hello| untab"), vec!["hello"]);
    assert_eq!(run_spec_collect("literal a b| untab"), vec!["a b"]);
}

#[test]
fn retab_leaves_a_single_blank_that_misses_the_stop() {
    assert_eq!(
        run_bytes("RETAB", &[], vec![b"a b".to_vec()]),
        vec![b"a b".to_vec()]
    );
}

#[test]
fn converse_round_trip_default() {
    let samples = [b"".as_slice(), b"a", b"a\tb", b"\tX", b"a\tb\tc", b"plain"];
    for s in samples {
        let expanded = run_bytes("UNTAB", &[], vec![s.to_vec()]);
        let compressed = run_bytes("RETAB", &[], expanded.clone());
        let again = run_bytes("UNTAB", &[], compressed);
        assert_eq!(again, expanded, "round-trip {:?}", s);
    }
}

#[test]
fn existing_tab_in_retab_is_one_column() {
    assert_eq!(
        run_bytes("RETAB", &[], vec![b"a\tb".to_vec()]),
        vec![b"a\tb".to_vec()]
    );
}

#[test]
fn leading_tab_default() {
    assert_eq!(
        run_bytes("UNTAB", &[], vec![b"\tX".to_vec()]),
        vec![b"   X".to_vec()]
    );
    assert_eq!(
        run_bytes("RETAB", &[], vec![b"   X".to_vec()]),
        vec![b"\tX".to_vec()]
    );
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | untab pad | hole",
        "literal a | retab BOTH | hole",
        "literal a | untab LEFT | hole",
        "literal a | retab ANYCASE | hole",
        "literal a | untab TABULATE | hole",
        "literal a | retab BLANK | hole",
        "literal a | untab * | hole",
        "literal a | retab +3 | hole",
        "literal a | untab STRING | hole",
    ] {
        let err = err_of(spec);
        assert!(err.contains("unexpected operand"), "{spec}: {err}");
    }
}

#[test]
fn mixed_negative_and_positive_rejected() {
    let err = err_of("literal a | retab -3 1 | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | untab 1 -3 | hole");
    assert!(err.contains("negative"), "{err}");
}

#[test]
fn zero_and_minus_zero_rejected() {
    let err = err_of("literal a | untab 0 | hole");
    assert!(err.contains("must be 1"), "{err}");
    let err = err_of("literal a | retab -0 | hole");
    assert!(err.contains("must be 1"), "{err}");
}

#[test]
fn verbs_are_exact() {
    // Diagrams are all-caps (pp.473, 575); no prefix.
    let reg = StageRegistry::default();
    assert!(reg.contains("UNTAB"));
    assert!(reg.contains("RETAB"));
    assert!(!reg.contains("UNTA"));
    assert!(!reg.contains("RETA"));
    assert!(!reg.contains("UNT"));
    assert!(!reg.contains("RET"));
    let err = err_of("literal a | unta | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("UNTA"),
        "{err}"
    );
    let err = err_of("literal a | reta | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("RETA"),
        "{err}"
    );
}

#[test]
fn extra_input_is_rejected() {
    for verb in ["UNTAB", "RETAB"] {
        let reg = StageRegistry::default();
        let mut b = PipelineBuilder::new();
        let src = b.add_stage("src", emit(&["a"]));
        let extra = b.add_stage("extra", emit(&["b"]));
        let stage = b.add_stage(verb, reg.create(verb, vec![]).unwrap());
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
        assert!(err.to_string().contains("primary"), "{verb}: {err}");
    }
}

#[test]
fn premature_termination_without_output() {
    for verb in ["UNTAB", "RETAB"] {
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
        let stage = b.add_stage(verb, reg.create(verb, vec![]).unwrap());
        b.connect_primary(src, stage);
        b.run().unwrap();
        assert!(
            written.get() <= 1,
            "{verb} should stop when no output is connected, wrote {}",
            written.get()
        );
    }
}

#[test]
fn secondary_output_is_unused() {
    for verb in ["UNTAB", "RETAB"] {
        let reg = StageRegistry::default();
        let primary = Rc::new(RefCell::new(Vec::new()));
        let secondary = Rc::new(RefCell::new(Vec::new()));
        let mut b = PipelineBuilder::new();
        let src = b.add_stage("src", emit(&["ab"]));
        let stage = b.add_stage(verb, reg.create(verb, vec![]).unwrap());
        let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
        let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
        b.connect_primary(src, stage);
        b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
        b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
        b.run().unwrap();
        assert_eq!(*primary.borrow(), vec!["ab".to_string()]);
        assert!(secondary.borrow().is_empty(), "{verb}");
    }
}

#[test]
fn visible_change_standin_for_readme() {
    // README shows tabs as `*` via CHANGE (xlate `09 *` is an input range).
    assert_eq!(
        run_spec_collect("literal a  b| retab | change x09 /*/"),
        vec!["a*b"]
    );
    assert_eq!(
        run_spec_collect("literal a*b| change /*/ x09 | untab"),
        vec!["a  b"]
    );
}
