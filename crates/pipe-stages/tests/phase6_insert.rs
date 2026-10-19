//! INSERT — Author's Edition 1.1.11 p.392 (REXX tutorial pp.100–101, 106).
//!
//! Portable ASCII: hex/binary dstrings are raw bytes. Book examples that use
//! EBCDIC code points are translated to ASCII in the tests that say so.

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
        "INSERT",
        reg.create("INSERT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("INSERT"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn run_primary_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "INSERT",
        reg.create("INSERT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("INSERT"),
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

#[test]
fn book_example_append_star() {
    // p.392: `pipe literal abc def | split | insert /*/ after | console`
    assert_eq!(
        run_spec_collect("literal abc def | split | insert /*/ after"),
        vec!["abc*", "def*"]
    );
}

#[test]
fn default_before_prefixes_entire_record() {
    // p.392: omitted BEFORE; default range is the entire record.
    assert_eq!(
        run_spec_collect("literal abc def | split | insert /*/"),
        vec!["*abc", "*def"]
    );
    assert_eq!(
        run_spec_collect("literal abc def | split | insert /*/ before"),
        vec!["*abc", "*def"]
    );
}

#[test]
fn column_before_and_after() {
    assert_eq!(run_primary(&["abcdef"], &["/X/", "3"]), vec!["abXcdef"]);
    assert_eq!(
        run_primary(&["abcdef"], &["/X/", "before", "3"]),
        vec!["abXcdef"]
    );
    assert_eq!(
        run_primary(&["abcdef"], &["/X/", "after", "3"]),
        vec!["abcXdef"]
    );
    assert_eq!(
        run_primary(&["abcdef"], &["/X/", "before", "2-4"]),
        vec!["aXbcdef"]
    );
    assert_eq!(
        run_primary(&["abcdef"], &["/X/", "after", "2-4"]),
        vec!["abcdXef"]
    );
}

#[test]
fn entire_record_explicit_range() {
    assert_eq!(
        run_primary(&["abc"], &["/*/", "after", "1-*"]),
        vec!["abc*"]
    );
    assert_eq!(
        run_primary(&["abc"], &["/*/", "before", "*-*"]),
        vec!["*abc"]
    );
}

#[test]
fn negative_columns() {
    assert_eq!(run_primary(&["abc"], &["/X/", "-1"]), vec!["abXc"]);
    assert_eq!(run_primary(&["abc"], &["/X/", "after", "-1"]), vec!["abcX"]);
    assert_eq!(
        run_primary(&["abcdef"], &["/X/", "before", "-2;-1"]),
        vec!["abcdXef"]
    );
}

#[test]
fn length_form() {
    // 3.2 is columns 3–4 (`c`,`d`); AFTER inserts past that span.
    assert_eq!(
        run_primary(&["abcdef"], &["/X/", "after", "3.2"]),
        vec!["abcdXef"]
    );
}

#[test]
fn word_range() {
    assert_eq!(
        run_primary(&["one two three"], &["/-/", "after", "word", "1"]),
        vec!["one- two three"]
    );
    assert_eq!(
        run_primary(&["one two three"], &["/-/", "before", "word", "2"]),
        vec!["one -two three"]
    );
    assert_eq!(
        run_primary(&["one two three"], &["/-/", "after", "w1"]),
        vec!["one- two three"]
    );
}

#[test]
fn field_range() {
    let rec = "a\tb\tc";
    assert_eq!(
        run_primary(&[rec], &["/X/", "after", "field", "2"]),
        vec!["a\tbX\tc"]
    );
    assert_eq!(
        run_primary(&[rec], &["/X/", "before", "f2"]),
        vec!["a\tXb\tc"]
    );
}

#[test]
fn substring_of_word() {
    let rec = "What a rum fellow.";
    assert_eq!(
        run_primary(
            &[rec],
            &["/X/", "before", "substring", "2", "of", "word", "3"]
        ),
        vec!["What a rXum fellow."]
    );
    assert_eq!(
        run_primary(&[rec], &["/X/", "after", "substr", "2", "of", "word", "3"]),
        vec!["What a ruXm fellow."]
    );
}

#[test]
fn missing_range_from_beginning_uses_first_column() {
    // p.392 Operation: missing range, first part relative to the beginning
    // → first column of the record.
    assert_eq!(run_primary(&["abc"], &["/X/", "10"]), vec!["Xabc"]);
    assert_eq!(run_primary(&["abc"], &["/X/", "after", "10"]), vec!["aXbc"]);
    assert_eq!(
        run_primary(&["one two"], &["/X/", "word", "9"]),
        vec!["Xone two"]
    );
}

#[test]
fn missing_range_from_end_uses_last_column() {
    assert_eq!(run_primary(&["abc"], &["/X/", "-10"]), vec!["abXc"]);
    assert_eq!(
        run_primary(&["abc"], &["/X/", "after", "-10"]),
        vec!["abcX"]
    );
    assert_eq!(
        run_primary(&["one two"], &["/X/", "word", "-9"]),
        vec!["one twXo"]
    );
}

#[test]
fn empty_record_default_range() {
    assert_eq!(run_primary(&[""], &["/* /"]), vec!["* "]);
    assert_eq!(run_primary(&[""], &["/*/", "after"]), vec!["*"]);
}

#[test]
fn empty_record_missing_range() {
    assert_eq!(run_primary(&[""], &["/X/", "10"]), vec!["X"]);
    assert_eq!(run_primary(&[""], &["/X/", "-10"]), vec!["X"]);
}

#[test]
fn null_dstring_is_a_no_op() {
    assert_eq!(run_primary(&["abc"], &["//"]), vec!["abc"]);
    assert_eq!(run_primary(&["abc"], &[",,", "after"]), vec!["abc"]);
}

#[test]
fn hex_and_binary_dstrings_are_ascii_bytes() {
    // p.194 dstring; book's xf1f2f3 example is EBCDIC digits. ASCII '*' is x2a.
    assert_eq!(
        run_spec_collect("literal abc|insert x2a after"),
        vec!["abc*"]
    );
    assert_eq!(
        run_spec_collect("literal abc|insert b00101010 after"),
        vec!["abc*"]
    );
}

#[test]
fn blank_insert() {
    assert_eq!(
        run_spec_collect("literal abc|insert / / after"),
        vec!["abc "]
    );
}

#[test]
fn if_book_prefix_example() {
    // p.390 uses insert /-> / and insert / / ; three-space shift of rejects
    // is the portable stand-in for the book's “three columns to the right”.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a abc b | split | if1: if locate /abc/ | insert /-> / | if1: | insert /   / | if1:"
        ),
        vec!["   a", "-> abc", "   b"]
    );
}

#[test]
fn missing_dstring_rejected() {
    let err = err_of("literal a | insert | hole");
    assert!(err.contains("delimitedString"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | insert /*/ in | hole",
        "literal a | insert /*/ between 1 2 | hole",
        "literal a | insert /*/ first | hole",
        "literal a | insert /*/ overlay | hole",
        "literal a | insert /*/ anycase | hole",
        "literal a | insert /*/ last | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unexpected") || err.contains("not a delimited"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn wrong_order_rejected() {
    let err = err_of("literal a | insert after /*/ | hole");
    assert!(
        err.contains("AFTER") && err.contains("after the delimited string"),
        "{err}"
    );
    let err = err_of("literal a | insert /*/ 2-3 after | hole");
    assert!(
        err.contains("AFTER") && err.contains("diagram order"),
        "{err}"
    );
    let err = err_of("literal a | insert 3 /*/ | hole");
    assert!(
        err.contains("inputRange") && err.contains("after the delimited string"),
        "{err}"
    );
}

#[test]
fn before_after_are_whole_words() {
    let err = err_of("literal a | insert /*/ bef | hole");
    assert!(err.contains("unexpected"), "{err}");
    let err = err_of("literal a | insert /*/ aft | hole");
    assert!(err.contains("unexpected"), "{err}");
}

#[test]
fn extra_operand_rejected() {
    let err = err_of("literal a | insert /*/ after 1 extra | hole");
    assert!(err.contains("unexpected"), "{err}");
}

#[test]
fn undelimited_string_rejected() {
    let err = err_of("literal a | insert hello | hole");
    assert!(err.contains("not a delimited string"), "{err}");
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("INSERT"));
    assert!(!reg.contains("INSER"));
    assert!(!reg.contains("INS"));
    let err = err_of("literal a | ins /*/ | hole");
    assert!(
        err.contains("unknown stage") && err.contains("INS"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("INSERT", reg.create("INSERT", vec!["/*/".into()]).unwrap());
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
    assert!(err.to_string().contains("primary"), "{err}");
}

#[test]
fn premature_termination_without_output() {
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
    let stage = b.add_stage("INSERT", reg.create("INSERT", vec!["/*/".into()]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "INSERT should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert!(run_primary(&[], &["/*/"]).is_empty());
}

#[test]
fn binary_bytes_round_trip() {
    let out = run_primary_bytes(vec![vec![0x00, 0xff]], &["x7e", "after"]);
    assert_eq!(out, vec![vec![0x00, 0xff, 0x7e]]);
}
