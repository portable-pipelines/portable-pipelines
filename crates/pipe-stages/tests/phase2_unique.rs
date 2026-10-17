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

fn run_both(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "UNIQUE",
        reg.create("UNIQUE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing UNIQUE"),
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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    run_both(records, args).0
}

fn run_primary_only(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "UNIQUE",
        reg.create("UNIQUE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing UNIQUE"),
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

fn run_both_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "UNIQUE",
        reg.create("UNIQUE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing UNIQUE"),
    );
    let sink_p = b.add_stage("sink_p", sink_bytes(p));
    let sink_s = b.add_stage("sink_s", sink_bytes(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

const DUP_INPUT: &[&str] = &["aaa1", "aaa2", "bbb1", "ccc1", "ccc2", "ccc3"];

#[test]
fn unique_book_figure_116() {
    // 1.1.11 p.56 Figure 116 — printed command and printed output.
    let out = run_spec_collect("literal a a a b b b a a a c c c | split | unique");
    assert_eq!(out, vec!["a", "b", "a", "c"]);
}

#[test]
fn unique_book_chapter19_range_mult_first() {
    // 1.1.11 p.574 BOTHDISK sample: `sort | unique 1.17 mult | unique 1.17 first`.
    // Columns 1–17 are fn+ft; the file mode is outside the key.
    let recs = &[
        "ONLYONE  DATA    A1",
        "PROFILE  EXEC    A1",
        "PROFILE  EXEC    B1",
        "PROFILE  EXEC    C1",
    ];
    let (p, s) = run_both(recs, &["1.17", "MULT"]);
    assert_eq!(
        p,
        vec![
            "PROFILE  EXEC    A1",
            "PROFILE  EXEC    B1",
            "PROFILE  EXEC    C1"
        ]
    );
    assert_eq!(s, vec!["ONLYONE  DATA    A1"]);

    let (p, s) = run_both(recs, &["1.17", "FIRST"]);
    assert_eq!(p, vec!["ONLYONE  DATA    A1", "PROFILE  EXEC    A1"]);
    assert_eq!(s, vec!["PROFILE  EXEC    B1", "PROFILE  EXEC    C1"]);

    let out = run_spec_collect(
        "literal PROFILE  EXEC    A1|literal PROFILE  EXEC    B1|literal ONLYONE  DATA    A1|sort|unique 1.17 mult|unique 1.17 first",
    );
    assert_eq!(out, vec!["PROFILE  EXEC    A1"]);
}

#[test]
fn unique_default_is_last() {
    let (p, s) = run_both(DUP_INPUT, &["1.3"]);
    assert_eq!(p, vec!["aaa2", "bbb1", "ccc3"]);
    assert_eq!(s, vec!["aaa1", "ccc1", "ccc2"]);
}

#[test]
fn unique_first_vs_last() {
    let (p, s) = run_both(DUP_INPUT, &["1.3", "FIRST"]);
    assert_eq!(p, vec!["aaa1", "bbb1", "ccc1"]);
    assert_eq!(s, vec!["aaa2", "ccc2", "ccc3"]);

    let (p, s) = run_both(DUP_INPUT, &["1.3", "LAST"]);
    assert_eq!(p, vec!["aaa2", "bbb1", "ccc3"]);
    assert_eq!(s, vec!["aaa1", "ccc1", "ccc2"]);
}

#[test]
fn unique_singles_and_multiple() {
    let recs = &["a", "a", "b", "c", "c"];
    let (p, s) = run_both(recs, &["SINGLES"]);
    assert_eq!(p, vec!["b"]);
    assert_eq!(s, vec!["a", "a", "c", "c"]);

    let (p, s) = run_both(recs, &["MULTIPLE"]);
    assert_eq!(p, vec!["a", "a", "c", "c"]);
    assert_eq!(s, vec!["b"]);
}

#[test]
fn unique_pairwise_unequal_pairs_to_primary() {
    // 1.1.11 p.573: both records of a pair go to primary when keys differ.
    let recs = &["a", "b", "a", "a", "c", "d"];
    let (p, s) = run_both(recs, &["PAIRWISE"]);
    assert_eq!(p, vec!["a", "b", "c", "d"]);
    assert_eq!(s, vec!["a", "a"]);
}

#[test]
fn unique_pairwise_odd_leftover_to_primary() {
    let (p, s) = run_both(&["a", "b", "c"], &["PAIR"]);
    assert_eq!(p, vec!["a", "b", "c"]);
    assert!(s.is_empty());
}

#[test]
fn unique_count_last_prefixes_run_length() {
    // COUNT + LAST: primary prefix is the run length (p.573).
    let (p, s) = run_both(&["a", "a", "a", "b"], &["COUNT"]);
    assert_eq!(p, vec!["         3a", "         1b"]);
    assert_eq!(s, vec!["         1a", "         2a"]);
}

#[test]
fn unique_count_first_prefixes_position() {
    let (p, s) = run_both(&["a", "a", "a", "b"], &["COUNT", "FIRST"]);
    assert_eq!(p, vec!["         1a", "         1b"]);
    assert_eq!(s, vec!["         2a", "         3a"]);
}

#[test]
fn unique_anycase_and_synonyms() {
    let recs = &["Foo", "foo", "BAR", "bar", "baz"];
    let out = run_primary(recs, &["ANYCASE"]);
    assert_eq!(out, vec!["foo", "bar", "baz"]);

    let out = run_primary(recs, &["ANY"]);
    assert_eq!(out, vec!["foo", "bar", "baz"]);

    let out = run_primary(recs, &["CASEANY", "FIRST"]);
    assert_eq!(out, vec!["Foo", "BAR", "baz"]);

    let out = run_primary(recs, &["IGNORECASE", "FIRST"]);
    assert_eq!(out, vec!["Foo", "BAR", "baz"]);

    let out = run_primary(recs, &["CASEIGNORE", "FIRST"]);
    assert_eq!(out, vec!["Foo", "BAR", "baz"]);
}

#[test]
fn unique_pad_extends_shorter_key() {
    // Range 1-5: "ab" vs "ab   " compare equal only when padded with blank.
    let recs = &["ab", "ab   ", "cd"];
    let (p, s) = run_both(recs, &["1-5"]);
    assert_eq!(p, vec!["ab", "ab   ", "cd"]);
    assert!(s.is_empty());

    let (p, s) = run_both(recs, &["PAD", "BLANK", "1-5"]);
    assert_eq!(p, vec!["ab   ", "cd"]);
    assert_eq!(s, vec!["ab"]);
}

#[test]
fn unique_nopad_is_default_and_explicit() {
    let recs = &["ab", "ab   "];
    let out = run_primary(recs, &["NOPAD", "1-5"]);
    assert_eq!(out, vec!["ab", "ab   "]);
}

#[test]
fn unique_paren_range_list_and_per_field_pad() {
    let recs = &["ABxx12", "ABxx12  ", "CDxx99"];
    // Fields 1-2 and 5-8. Without PAD, "12" vs "12  " differ.
    let out = run_primary(recs, &["(", "1-2", "5-8", ")", "FIRST"]);
    assert_eq!(out, vec!["ABxx12", "ABxx12  ", "CDxx99"]);

    // PAD blank on the second field: "12" extends to "12  ".
    let out = run_primary(recs, &["(", "1-2", "5-8", "PAD", "BLANK", ")", "FIRST"]);
    assert_eq!(out, vec!["ABxx12", "CDxx99"]);
}

#[test]
fn unique_word_range() {
    let recs = &["foo bar", "foo baz", "qux"];
    let (p, s) = run_both(recs, &["WORD", "1", "LAST"]);
    assert_eq!(p, vec!["foo baz", "qux"]);
    assert_eq!(s, vec!["foo bar"]);
}

#[test]
fn unique_compatibility_keywords_then_range() {
    // p.572: two optional keywords then an optional input range.
    let recs = &["aaa1", "aaa2", "bbb1"];
    let out = run_primary(recs, &["FIRST", "1.3"]);
    assert_eq!(out, vec!["aaa1", "bbb1"]);

    let out = run_primary(recs, &["COUNT", "LAST", "1.3"]);
    assert_eq!(out, vec!["         2aaa2", "         1bbb1"]);
}

#[test]
fn unique_range_then_two_keywords() {
    let recs = &["aaa1", "aaa2", "bbb1"];
    let out = run_primary(recs, &["1.3", "COUNT", "LAST"]);
    assert_eq!(out, vec!["         2aaa2", "         1bbb1"]);
}

#[test]
fn unique_null_records() {
    let recs = &["", "", "x", "", ""];
    let (p, s) = run_both(recs, &[]);
    assert_eq!(p, vec!["", "x", ""]);
    assert_eq!(s, vec!["", ""]);

    let (p, s) = run_both(recs, &["SINGLES"]);
    assert_eq!(p, vec!["x"]);
    assert_eq!(s, vec!["", "", "", ""]);

    let (p, s) = run_both(&["", "x"], &["PAIRWISE"]);
    assert_eq!(p, vec!["", "x"]);
    assert!(s.is_empty());
}

#[test]
fn unique_empty_input_writes_nothing() {
    let out = run_primary(&[], &[]);
    assert!(out.is_empty());
}

#[test]
fn unique_secondary_discarded_when_not_connected() {
    let out = run_primary_only(DUP_INPUT, &["1.3", "FIRST"]);
    assert_eq!(out, vec!["aaa1", "bbb1", "ccc1"]);
}

#[test]
fn unique_end_to_end_parser() {
    let out = run_spec_collect("literal a a b a | split | unique first");
    assert_eq!(out, vec!["a", "b", "a"]);

    let parsed =
        pipe_parser::parse_pipeline("literal x | unique count pad blank anycase 1.8 first")
            .expect("parse");
    assert_eq!(parsed.pipelines[0][1].verb.to_ascii_uppercase(), "UNIQUE");
    let args: Vec<&str> = parsed.pipelines[0][1]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(
        args,
        vec!["count", "pad", "blank", "anycase", "1.8", "first"]
    );

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
    let spec =
        "(ENDCHAR ?) literal aa1|literal aa2|literal bb| U: unique 1.2 first | >>P ? U: | >>S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    // literal cascade writes reverse: bb, aa2, aa1. FIRST of each 1.2 run → bb, aa2.
    assert_eq!(
        *collected_p.borrow(),
        vec!["bb".to_string(), "aa2".to_string()]
    );
    assert_eq!(*collected_s.borrow(), vec!["aa1".to_string()]);
}

#[test]
fn unique_abbreviations() {
    let recs = &["a", "a", "b", "b"];
    // SINGLEs min 6; MULTiple / PAIRwise min 4 (p.572).
    assert_eq!(run_primary(recs, &["SINGLE"]), Vec::<String>::new());
    assert_eq!(run_primary(recs, &["SINGLES"]), Vec::<String>::new());
    assert_eq!(
        run_primary(recs, &["MULT"]),
        recs.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()
    );
    assert_eq!(run_primary(recs, &["PAIR"]), Vec::<String>::new());
}

#[test]
fn unique_operand_errors() {
    let err = run_spec("literal a | unique pairwise count | hole").unwrap_err();
    assert!(
        err.to_string().contains("PAIRWISE") && err.to_string().contains("COUNT"),
        "{err}"
    );

    let err = run_spec("literal a | unique count pairwise | hole").unwrap_err();
    assert!(
        err.to_string().contains("PAIRWISE") && err.to_string().contains("COUNT"),
        "{err}"
    );

    let err = run_spec("literal a | unique first last | hole").unwrap_err();
    assert!(
        err.to_string().contains("diagram order") || err.to_string().contains("LAST"),
        "{err}"
    );

    let err = run_spec("literal a | unique pad | hole").unwrap_err();
    assert!(err.to_string().contains("PAD"), "{err}");

    let err = run_spec("literal a | unique caseless | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | unique anyof /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | unique before | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | unique first anycase | hole").unwrap_err();
    assert!(
        err.to_string().contains("diagram order") || err.to_string().contains("ANYCASE"),
        "{err}"
    );

    let err = run_spec("literal a | unique ( | hole").unwrap_err();
    assert!(
        err.to_string().contains("parenthes") || err.to_string().contains("closing"),
        "{err}"
    );

    // SINGLEs minimum abbreviation is 6 (p.572); SINGL is too short.
    let err = run_spec("literal a | unique singl | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");
}

#[test]
fn unique_hex_pad_xorc() {
    let recs = vec![b"ab".to_vec(), b"ab\0\0".to_vec(), b"cd".to_vec()];
    let (p, s) = run_both_bytes(recs, &["PAD", "00", "1-4"]);
    assert_eq!(p, vec![b"ab\0\0".to_vec(), b"cd".to_vec()]);
    assert_eq!(s, vec![b"ab".to_vec()]);
}

#[test]
fn unique_premature_termination_without_output() {
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
    let stage = b.add_stage("UNIQUE", reg.create("UNIQUE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "UNIQUE should stop reading when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn unique_secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage("UNIQUE", reg.create("UNIQUE", vec![]).unwrap());
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
fn unique_secondary_only_connected() {
    let reg = StageRegistry::default();
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "a", "b"]));
    let stage = b.add_stage(
        "UNIQUE",
        reg.create("UNIQUE", vec!["FIRST".into()]).unwrap(),
    );
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*secondary.borrow(), vec!["a".to_string()]);
}
