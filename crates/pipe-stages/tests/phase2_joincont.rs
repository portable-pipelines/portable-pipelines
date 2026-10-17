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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "JOINCONT",
        reg.create("JOINCONT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing JOINCONT"),
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
fn joincont_book_figure_127() {
    // 1.1.11 p.60 Figure 127 / p.401: C newline splice with a trailing `\`.
    let out = run_spec_collect("literal one two\\ three four | split | joincont /\\/");
    assert_eq!(out, vec!["one", "twothree", "four"]);
}

#[test]
fn joincont_book_rfc822_leading_space_keep() {
    // 1.1.11 p.403: joincont leading / / keep
    let out = run_primary(
        &["From: a", " b", " c", "Subject: x"],
        &["LEADING", "/ /", "KEEP"],
    );
    assert_eq!(out, vec!["From: a b c", "Subject: x"]);
}

#[test]
fn joincont_book_note3_not_range_1() {
    // 1.1.11 p.403 Note 3: joincont not range 1 /x/ joins until (and including)
    // the next record whose column 1 contains "x".
    let out = run_primary(
        &["a1", "a2", "x3", "b1", "x2"],
        &["NOT", "RANGE", "1", "/x/"],
    );
    assert_eq!(out, vec!["a1a2x3", "b1x2"]);
}

#[test]
fn joincont_default_trailing_deletes_marker() {
    // The record without the marker is the last of the set (included).
    let out = run_primary(&["This,", "is,", "the,", "first,", "sentence."], &["/,/"]);
    assert_eq!(out, vec!["Thisisthefirstsentence."]);
}

#[test]
fn joincont_trailing_insert_string() {
    let out = run_primary(
        &["This,", "is,", "the,", "first,", "sentence."],
        &["/,/", "/ /"],
    );
    assert_eq!(out, vec!["This is the first sentence."]);
}

#[test]
fn joincont_not_keeps_marker() {
    // NOT: join while the string is absent; marker is retained (p.402).
    let out = run_primary(
        &["This,", "is,", "the,", "first,", "sentence."],
        &["NOT", "/./"],
    );
    assert_eq!(out, vec!["This,is,the,first,sentence."]);
}

#[test]
fn joincont_keep_retains_trailing_marker() {
    let out = run_primary(&["ab,", "cd.", "ef"], &["/,/", "KEEP"]);
    assert_eq!(out, vec!["ab,cd.", "ef"]);
}

#[test]
fn joincont_range_implies_keep() {
    // RANGE implies KEEP: column 1 "x" is not deleted.
    let out = run_primary(&["xhello", "world", "yno"], &["RANGE", "1", "/x/"]);
    assert_eq!(out, vec!["xhelloworld", "yno"]);
}

#[test]
fn joincont_anycase_leading() {
    let out = run_primary(
        &["key1 data 1", "KEY1 data 2", "key1 data 3", "key2 data 4"],
        &["ANYCASE", "LEADING", "/key1/"],
    );
    assert_eq!(out, vec!["key1 data 1 data 2 data 3", "key2 data 4"]);
}

#[test]
fn joincont_anyof_trailing() {
    let out = run_primary(&["a,", "b;", "c."], &["ANYOF", "/,;/"]);
    assert_eq!(out, vec!["abc."]);
}

#[test]
fn joincont_anyof_anycase() {
    let out = run_primary(&["oneX", "twoY", "three"], &["ANYCASE", "ANYOF", "/xy/"]);
    assert_eq!(out, vec!["onetwothree"]);
}

#[test]
fn joincont_leading_strips_marker_without_keep() {
    let out = run_primary(&["From: a", " b", " c"], &["LEADING", "/ /"]);
    assert_eq!(out, vec!["From: abc"]);
}

#[test]
fn joincont_delay_same_records_as_trailing() {
    let without = run_primary(&["a,", "b.", "c"], &["TRAILING", "/,/"]);
    let with = run_primary(&["a,", "b.", "c"], &["TRAILING", "DELAY", "/,/"]);
    assert_eq!(without, with);
    assert_eq!(with, vec!["ab.", "c"]);
}

#[test]
fn joincont_delay_ignored_with_leading() {
    let out = run_primary(&["a", " b", "c"], &["LEADING", "DELAY", "/ /", "KEEP"]);
    assert_eq!(out, vec!["a b", "c"]);
}

#[test]
fn joincont_hex_and_binary_dstrings() {
    // ASCII comma X'2C'; space X'20'.
    let out = run_primary(&["a,", "b.", "c"], &["x2c", "x20"]);
    assert_eq!(out, vec!["a b.", "c"]);
    let out = run_primary(&["a,", "b"], &["b00101100"]);
    assert_eq!(out, vec!["ab"]);
}

#[test]
fn joincont_abbreviations() {
    // ANYCASE min 4; ANYOF wins at 3 (Note 1).
    let out = run_primary(&["aX", "b"], &["ANYC", "/x/"]);
    assert_eq!(out, vec!["ab"]);

    let out = run_primary(&["a,", "b;", "c"], &["ANY", "/,;/"]);
    assert_eq!(out, vec!["abc"]);

    let out = run_primary(&["aX", "b"], &["ANYO", "/X/"]);
    assert_eq!(out, vec!["ab"]);
}

#[test]
fn joincont_case_synonyms() {
    for syn in ["CASEANY", "IGNORECASE", "CASEIGNORE"] {
        let out = run_primary(&["abX", "cd"], &[syn, "/x/"]);
        assert_eq!(out, vec!["abcd"], "synonym {syn}");
    }
}

#[test]
fn joincont_empty_continuation_joins_all() {
    // Empty needle is a suffix of every record, including nulls.
    let out = run_primary(&["a", "b", "c"], &["//"]);
    assert_eq!(out, vec!["abc"]);

    let out = run_primary(&["a", "b"], &[",,"]);
    assert_eq!(out, vec!["ab"]);
}

#[test]
fn joincont_null_records() {
    // Null does not have a non-empty trailing marker → passed unmodified.
    let out = run_primary(&["", "a,", "b"], &["/,/"]);
    assert_eq!(out, vec!["", "ab"]);

    // Lone continued record at EOF writes the stripped remainder (null).
    let out = run_primary(&[","], &["/,/"]);
    assert_eq!(out, vec![""]);

    // KEEP retains the marker-only record.
    let out = run_primary(&[","], &["/,/", "KEEP"]);
    assert_eq!(out, vec![","]);

    // Null next record does not have a leading marker.
    let out = run_primary(&["a", ""], &["LEADING", "/ /"]);
    assert_eq!(out, vec!["a", ""]);

    let out = run_primary(&[], &["/,/"]);
    assert!(out.is_empty());
}

#[test]
fn joincont_eof_flushes_continued_run() {
    let out = run_primary(&["a,", "b,"], &["/,/"]);
    assert_eq!(out, vec!["ab"]);
}

#[test]
fn joincont_range_word_and_substring() {
    let out = run_primary(
        &["keep x rest", "next", "stop"],
        &["RANGE", "WORD", "2", "/x/"],
    );
    assert_eq!(out, vec!["keep x restnext", "stop"]);

    let out = run_primary(
        &["What a rum fellow.", "more"],
        &["RANGE", "SUBSTR", "2", "OF", "WORD", "3", "/u/"],
    );
    assert_eq!(out, vec!["What a rum fellow.more"]);
}

#[test]
fn joincont_end_to_end_through_parser() {
    run_spec("literal one two\\ three four | split | joincont /\\/ | hole").unwrap();
    run_spec("literal a, b | split | joincont /,/ / / | hole").unwrap();
    run_spec("literal a b | split | joincont leading / / keep | hole").unwrap();
    run_spec("literal xa ya | split | joincont not range 1 /x/ | hole").unwrap();
    run_spec("literal a,b | split | joincont anyof /,/ | hole").unwrap();
    run_spec("literal a, b | split | joincont trailing delay x2c | hole").unwrap();
}

#[test]
fn joincont_parser_book_example_produces_records() {
    let out = run_spec_collect("literal one two\\ three four | split | joincont /\\/");
    assert_eq!(out, vec!["one", "twothree", "four"]);

    let out = run_spec_collect("literal This, is, done. | split | joincont /,/ / /");
    assert_eq!(out, vec!["This is done."]);
}

#[test]
fn joincont_operand_errors() {
    let err = run_spec("literal a | joincont | hole").unwrap_err();
    assert!(
        err.to_string()
            .contains("requires a delimited continuation string"),
        "{err}"
    );

    let err = run_spec("literal a | joincont hello | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");

    let err = run_spec("literal a | joincont /x/ trailing | hole").unwrap_err();
    assert!(err.to_string().contains("diagram order"), "{err}");

    let err = run_spec("literal a | joincont delay trailing /x/ | hole").unwrap_err();
    assert!(err.to_string().contains("diagram order"), "{err}");

    let err = run_spec("literal a | joincont keep /x/ | hole").unwrap_err();
    assert!(err.to_string().contains("diagram order"), "{err}");

    // TRAILING is a whole word (diagram shows no abbreviation).
    let err = run_spec("literal a | joincont trail /x/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");

    // CASELESS is not an Author's Edition synonym.
    let err = run_spec("literal a | joincont caseless /x/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");

    let err = run_spec("literal a | joincont range | hole").unwrap_err();
    assert!(
        err.to_string().contains("inputRange") || err.to_string().contains("range"),
        "{err}"
    );

    let err = run_spec("literal a | joincont /x/ extra | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");
}

#[test]
fn joincont_premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(std::cell::Cell::new(0u32));
    let count = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a,", "b,", "c,", "d"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "JOINCONT",
        reg.create("JOINCONT", vec!["/,/".into()]).unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "JOINCONT should stop reading when output is not connected, wrote {}",
        written.get()
    );
}

#[test]
fn joincont_secondary_output_is_unused() {
    // Unlike JOIN, JOINCONT does not forbid a secondary output; it only writes
    // the primary (p.401 streams: primary in / primary out).
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a,", "b"]));
    let stage = b.add_stage(
        "JOINCONT",
        reg.create("JOINCONT", vec!["/,/".into()]).unwrap(),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["ab".to_string()]);
    assert!(secondary.borrow().is_empty());
}
