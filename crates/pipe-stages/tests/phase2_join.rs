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
        "JOIN",
        reg.create("JOIN", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing JOIN"),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn words_in_a_sentence() -> Vec<&'static str> {
    vec!["a", "few", "words", "in", "a", "sentence"]
}

#[test]
fn join_book_figure_126_default_pairs() {
    // 1.1.11 p.60 / Figure 126: join (default append 1) with no insert string.
    let out = run_primary(&words_in_a_sentence(), &[]);
    assert_eq!(out, vec!["afew", "wordsin", "asentence"]);
}

#[test]
fn join_book_figure_126_append_two() {
    let out = run_primary(&words_in_a_sentence(), &["2"]);
    assert_eq!(out, vec!["afewwords", "inasentence"]);
}

#[test]
fn join_book_figure_126_append_two_with_string() {
    let out = run_primary(&words_in_a_sentence(), &["2", "/**/"]);
    assert_eq!(out, vec!["a**few**words", "in**a**sentence"]);
}

#[test]
fn join_book_figure_126_star_blank_max_20() {
    let out = run_primary(&words_in_a_sentence(), &["*", "/ /", "20"]);
    assert_eq!(out, vec!["a few words in a", "sentence"]);
}

#[test]
fn join_book_example_asterisk_pairs() {
    // 1.1.11 p.401: pipe literal a b c d e | split | join /*/ | console
    let out = run_primary(&["a", "b", "c", "d", "e"], &["/*/"]);
    assert_eq!(out, vec!["a*b", "c*d", "e"]);
}

#[test]
fn join_book_example_flow_width_10() {
    // 1.1.11 p.401: join * / / 10
    let out = run_primary(
        &["This", "is", "a", "short", "sentence."],
        &["*", "/ /", "10"],
    );
    assert_eq!(out, vec!["This is a", "short", "sentence."]);
}

#[test]
fn join_star_all_records() {
    let out = run_primary(&["a", "b", "c"], &["*"]);
    assert_eq!(out, vec!["abc"]);

    let out = run_primary(&["a", "b", "c"], &["*", "/-/"]);
    assert_eq!(out, vec!["a-b-c"]);
}

#[test]
fn join_zero_is_one_record_sets() {
    // Number 0: append nothing; each record is a set by itself (p.400).
    let out = run_primary(&["a", "b", "c"], &["0"]);
    assert_eq!(out, vec!["a", "b", "c"]);

    let out = run_primary(&["a", "b"], &["0", "/*/", "TERMINATE"]);
    assert_eq!(out, vec!["a*", "b*"]);
}

#[test]
fn join_leftover_partial_set() {
    let out = run_primary(&["a", "b", "c"], &[]);
    assert_eq!(out, vec!["ab", "c"]);
}

#[test]
fn join_empty_input_writes_nothing() {
    let out = run_primary(&[], &[]);
    assert!(out.is_empty());

    let out = run_primary(&[], &["*"]);
    assert!(out.is_empty());
}

#[test]
fn join_null_records_are_contributors() {
    // Null records are ordinary (empty) contributions, not set terminators.
    let out = run_primary(&["a", "", "b"], &[]);
    assert_eq!(out, vec!["a", "b"]);

    let out = run_primary(&["a", "", "b"], &["/*/"]);
    assert_eq!(out, vec!["a*", "b"]);

    let out = run_primary(&[""], &[]);
    assert_eq!(out, vec![""]);

    let out = run_primary(&["", ""], &["*"]);
    assert_eq!(out, vec![""]);

    let out = run_primary(&["", ""], &["*", "/x/"]);
    assert_eq!(out, vec!["x"]);
}

#[test]
fn join_count_prefix() {
    let out = run_primary(&["a", "b", "c"], &["COUNT"]);
    assert_eq!(out, vec![format!("{:>10}ab", 2), format!("{:>10}c", 1),]);

    let out = run_primary(&["a", "b", "c"], &["COUNT", "*"]);
    assert_eq!(out, vec![format!("{:>10}abc", 3)]);
}

#[test]
fn join_terminate_appends_after_last() {
    let out = run_primary(&["a", "b", "c", "d", "e"], &["/*/", "TERMINATE"]);
    assert_eq!(out, vec!["a*b*", "c*d*", "e*"]);

    let out = run_primary(&["a", "b", "c"], &["2", "/**/", "TERM"]);
    assert_eq!(out, vec!["a**b**c**"]);
}

#[test]
fn join_keylength_same_leading_key() {
    let out = run_primary(&["abc123", "abc456", "def789"], &["KEYLENGTH", "3"]);
    assert_eq!(out, vec!["abc123456", "def789"]);

    let out = run_primary(&["abc123", "abc456", "def789"], &["KEYLEN", "3", "/-/"]);
    assert_eq!(out, vec!["abc123-456", "def789"]);
}

#[test]
fn join_keylength_and_max_length() {
    let out = run_primary(&["aa1111", "aa2222", "aa3333"], &["KEYLENGTH", "2", "8"]);
    // "aa1111" (6) + "2222" would be 10 > 8, so flush then start the next key group.
    assert_eq!(out, vec!["aa1111", "aa2222", "aa3333"]);
}

#[test]
fn join_max_length_second_number() {
    // A lone first number is the append count, not the maximum (p.400).
    let out = run_primary(&["aa", "bb", "cc"], &["2"]);
    assert_eq!(out, vec!["aabbcc"]);

    // After a delimiter, the number is the maximum length.
    let out = run_primary(&["aa", "bb", "cc"], &["/ /", "5"]);
    assert_eq!(out, vec!["aa bb", "cc"]);
}

#[test]
fn join_oversized_record_not_truncated() {
    let out = run_primary(&["toolong", "a", "b"], &["*", "/ /", "3"]);
    assert_eq!(out, vec!["toolong", "a b"]);
}

#[test]
fn join_end_to_end_through_parser() {
    run_spec("literal a few words in a sentence | split | join | hole").unwrap();
    run_spec("literal a b c d e | split | join /*/ | hole").unwrap();
    run_spec("literal This is a short sentence. | split | join * / / 10 | hole").unwrap();
    run_spec("literal a b c | split | join 2 /**/ | hole").unwrap();
    run_spec("literal a b c | split | join count * | hole").unwrap();
    run_spec("literal abc123 | join keylen 3 | hole").unwrap();
}

#[test]
fn join_parser_book_examples_produce_records() {
    // End-to-end: LITERAL + SPLIT + JOIN, collected via a builder sink so we
    // can assert the book's own console output.
    let out = run_spec_collect("literal a few words in a sentence | split | join");
    assert_eq!(out, vec!["afew", "wordsin", "asentence"]);

    let out = run_spec_collect("literal a b c d e | split | join /*/");
    assert_eq!(out, vec!["a*b", "c*d", "e"]);

    let out = run_spec_collect("literal This is a short sentence. | split | join * / / 10");
    assert_eq!(out, vec!["This is a", "short", "sentence."]);
}

#[test]
fn join_book_p401_examples_as_printed() {
    // Author's Edition 1.1.11 p.401, exact operand spelling.
    let out = run_spec_collect("literal a b c d e | split | join /*/");
    assert_eq!(out, vec!["a*b", "c*d", "e"]);

    let out = run_spec_collect("literal This is a short sentence. | split | join * / / 10");
    assert_eq!(out, vec!["This is a", "short", "sentence."]);
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
fn join_operand_errors() {
    let err = run_spec("literal a | join hello | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");

    let err = run_spec("literal a | join keylength | hole").unwrap_err();
    assert!(
        err.to_string().contains("KEYLENGTH requires a number"),
        "{err}"
    );

    let err = run_spec("literal a | join 2 keylength 3 | hole").unwrap_err();
    assert!(err.to_string().contains("diagram order"), "{err}");

    let err = run_spec("literal a | join terminate | hole").unwrap_err();
    assert!(err.to_string().contains("diagram order"), "{err}");

    let err = run_spec("literal a | join anycase /*/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");

    let err = run_spec("literal a | join count 2 extra | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");
}

#[test]
fn join_abbreviations() {
    let out = run_primary(&["abX", "abY"], &["KEYLEN", "2"]);
    assert_eq!(out, vec!["abXY"]);

    let out = run_primary(&["abX", "abY"], &["KEYLENGT", "2"]);
    assert_eq!(out, vec!["abXY"]);

    let out = run_primary(&["a", "b"], &["/-/", "TERM"]);
    assert_eq!(out, vec!["a-b-"]);
}

#[test]
fn join_hex_and_binary_dstrings() {
    // p.194: JOIN insert may be a hex or binary delimitedString.
    let out = run_primary(&["a", "b", "c"], &["x2a"]);
    assert_eq!(out, vec!["a*b", "c"]);
    let out = run_primary(&["a", "b"], &["b00101101"]);
    assert_eq!(out, vec!["a-b"]);
}

#[test]
fn join_rejects_secondary_output() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b"]));
    let stage = b.add_stage("JOIN", reg.create("JOIN", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(p));
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .contains("no output stream other than the primary"),
        "{err}"
    );
}

#[test]
fn join_secondary_input_flushes_buffer() {
    // Secondary input is defined, so the default set size is unlimited (p.400).
    // A record on the secondary stream flushes the buffer and is discarded.
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c", "d"]));
    let flush = b.add_stage("flush", emit(&["X", "Y"]));
    let stage = b.add_stage("JOIN", reg.create("JOIN", vec!["/-/".into()]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect(flush, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let out = primary.borrow().clone();
    // Drain-bias plus primary-first read_any: all four primary records are
    // consumed before the secondary flushes (or EOF flushes the remainder).
    // With two secondary records the exact grouping can vary with scheduling,
    // but every primary byte appears once, separators are only between
    // primaries of a set, and secondary records are never written.
    let joined = out.join("|");
    assert!(
        !joined.contains('X') && !joined.contains('Y'),
        "secondary records must be discarded: {out:?}"
    );
    let letters: String = out
        .iter()
        .flat_map(|s| s.chars().filter(|c| *c != '-'))
        .collect();
    assert_eq!(letters, "abcd");
}

#[test]
fn join_secondary_input_default_is_unlimited() {
    // No secondary flush records: secondary is connected but stays quiet until
    // EOF, so default infinity joins everything.
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c"]));
    let flush = b.add_stage("flush", emit(&[]));
    let stage = b.add_stage("JOIN", reg.create("JOIN", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect(flush, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["abc".to_string()]);
}

#[test]
fn join_premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(std::cell::Cell::new(0u32));
    let count = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b", "c", "d"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("JOIN", reg.create("JOIN", vec![]).unwrap());
    b.connect_primary(src, stage);
    // No output connected: JOIN terminates without draining the producer.
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "JOIN should stop reading when output is not connected, wrote {}",
        written.get()
    );
}
