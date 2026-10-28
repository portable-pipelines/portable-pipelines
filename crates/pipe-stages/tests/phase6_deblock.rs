//! DEBLOCK — Author's Edition 1.1.11 pp.317–320 (LINEND / CRLF subset).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

fn emit_bytes(records: &[&[u8]]) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<Vec<u8>> = records.iter().map(|s| s.to_vec()).collect();
    Box::new(move |ctx| {
        for rec in records {
            if ctx.write(Record::from(rec)).is_err() {
                break;
            }
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

fn run_through(records: &[&[u8]], args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "DEBLOCK",
        reg.create("DEBLOCK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("DEBLOCK"),
    );
    let sink_p = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
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

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

fn recs(got: &[Vec<u8>]) -> Vec<&[u8]> {
    got.iter().map(|v| v.as_slice()).collect()
}

#[test]
fn linend_default_splits_on_ascii_lf() {
    assert_eq!(
        recs(&run_through(&[b"hello\nworld"], &["LINEND"])),
        vec![b"hello".as_slice(), b"world".as_slice()]
    );
}

#[test]
fn linend_missing_final_line_end() {
    assert_eq!(
        recs(&run_through(&[b"a\nb"], &["LINEND"])),
        vec![b"a".as_slice(), b"b".as_slice()]
    );
}

#[test]
fn linend_trailing_null_without_terminate() {
    assert_eq!(
        recs(&run_through(&[b"a\nb\n"], &["LINEND"])),
        vec![b"a".as_slice(), b"b".as_slice(), b"".as_slice()]
    );
}

#[test]
fn linend_terminate_suppresses_trailing_null() {
    assert_eq!(
        recs(&run_through(&[b"a\nb\n"], &["LINEND", "TERMINATE"])),
        vec![b"a".as_slice(), b"b".as_slice()]
    );
    assert_eq!(
        recs(&run_through(&[b"a\nb"], &["LINEND", "TERM"])),
        vec![b"a".as_slice(), b"b".as_slice()]
    );
}

#[test]
fn linend_consecutive_delims_are_null_records() {
    assert_eq!(
        recs(&run_through(&[b"a\n\nb"], &["LINEND"])),
        vec![b"a".as_slice(), b"".as_slice(), b"b".as_slice()]
    );
}

#[test]
fn linend_spans_input_records() {
    assert_eq!(
        recs(&run_through(&[b"hel", b"lo\nwor", b"ld"], &["LINEND"])),
        vec![b"hello".as_slice(), b"world".as_slice()]
    );
}

#[test]
fn linend_discards_null_input_records() {
    assert_eq!(
        recs(&run_through(&[b"hel", b"", b"lo\nworld"], &["LINEND"])),
        vec![b"hello".as_slice(), b"world".as_slice()]
    );
}

#[test]
fn linend_xorc_15_is_literal_byte() {
    assert_eq!(
        recs(&run_through(&[b"ab\x15cd"], &["LINEND", "15"])),
        vec![b"ab".as_slice(), b"cd".as_slice()]
    );
    // Default LINEND does not split on X'15'.
    assert_eq!(
        recs(&run_through(&[b"ab\x15cd"], &["LINEND"])),
        vec![b"ab\x15cd".as_slice()]
    );
}

#[test]
fn linend_eof_default_is_ascii_sub() {
    assert_eq!(
        recs(&run_through(&[b"a\nb\x1Acd\ne"], &["LINEND", "EOF"])),
        vec![b"a".as_slice(), b"b".as_slice()]
    );
}

#[test]
fn linend_eof_xorc_and_consume_that_block() {
    assert_eq!(
        recs(&run_through(
            &[b"one\n", b"two\x3Frest\nthree"],
            &["LINEND", "EOF", "3F"]
        )),
        vec![b"one".as_slice(), b"two".as_slice()]
    );
}

#[test]
fn crlf_splits_and_spans_the_sequence() {
    assert_eq!(
        recs(&run_through(&[b"hello\r\nworld"], &["CRLF"])),
        vec![b"hello".as_slice(), b"world".as_slice()]
    );
    assert_eq!(
        recs(&run_through(&[b"hello\r", b"\nworld"], &["CRLF"])),
        vec![b"hello".as_slice(), b"world".as_slice()]
    );
}

#[test]
fn crlf_lone_cr_or_lf_is_data() {
    assert_eq!(
        recs(&run_through(&[b"a\rb\nc"], &["CRLF"])),
        vec![b"a\rb\nc".as_slice()]
    );
}

#[test]
fn crlf_eof_stops_the_stream() {
    assert_eq!(
        recs(&run_through(&[b"a\r\nb\x1Acd\r\ne"], &["CRLF", "EOF"])),
        vec![b"a".as_slice(), b"b".as_slice()]
    );
}

#[test]
fn crlf_consecutive_and_terminate() {
    assert_eq!(
        recs(&run_through(&[b"a\r\n\r\nb\r\n"], &["CRLF"])),
        vec![
            b"a".as_slice(),
            b"".as_slice(),
            b"b".as_slice(),
            b"".as_slice()
        ]
    );
    assert_eq!(
        recs(&run_through(&[b"a\r\n\r\nb\r\n"], &["CRLF", "TERMINATE"])),
        vec![b"a".as_slice(), b"".as_slice(), b"b".as_slice()]
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert!(run_through(&[], &["LINEND"]).is_empty());
    assert!(run_through(&[b""], &["LINEND"]).is_empty());
}

#[test]
fn spec_form_change_to_lf() {
    assert_eq!(
        run_spec_collect(r#"literal aNLbNL| change /NL/ x0a | deblock linend terminate"#),
        vec!["a", "b"]
    );
}

#[test]
fn unimplemented_formats_rejected() {
    for spec in [
        "literal a | deblock c | hole",
        "literal a | deblock string /x/ | hole",
        "literal a | deblock cms | hole",
        "literal a | deblock v | hole",
        "literal a | deblock vb | hole",
        "literal a | deblock vs | hole",
        "literal a | deblock vbs | hole",
        "literal a | deblock sf | hole",
        "literal a | deblock awstape | hole",
        "literal a | deblock netdata | hole",
        "literal a | deblock textfile | hole",
        "literal a | deblock ckd | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("not implemented") && (err.contains("LINEND") || err.contains("FIXED")),
            "{spec}: {err}"
        );
    }
}

#[test]
fn fixed_splits_into_character_groups() {
    assert_eq!(
        recs(&run_through(&[b"654321"], &["2"])),
        vec![b"65".as_slice(), b"43".as_slice(), b"21".as_slice()]
    );
    assert_eq!(
        recs(&run_through(&[b"65"], &["1"])),
        vec![b"6".as_slice(), b"5".as_slice()]
    );
    assert_eq!(
        recs(&run_through(&[b"abcde"], &["FIXED", "2"])),
        vec![b"ab".as_slice(), b"cd".as_slice(), b"e".as_slice()]
    );
    assert_eq!(
        run_spec_collect("literal 654321| deblock 2"),
        vec!["65", "43", "21"]
    );
    assert_eq!(
        run_spec_collect("literal 654321| reverse | deblock 2"),
        vec!["12", "34", "56"]
    );
    // Bare DEBLOCK is FIXED 80 (book default). A short record is one leftover.
    assert_eq!(run_spec_collect("literal abc| deblock"), vec!["abc"]);
    // FIXED is a byte stream: input records are concatenated, then split.
    assert_eq!(
        recs(&run_through(&[b"3", b"1", b"78"], &["2"])),
        vec![b"31".as_slice(), b"78".as_slice()]
    );
    // A flush space sent through FIXED joins the leftover of the previous
    // record (`7` + blank), so the space must be appended after DEBLOCK.
    assert_eq!(
        recs(&run_through(&[b"317", b" "], &["2"])),
        vec![b"31".as_slice(), b"7 ".as_slice()]
    );
}

#[test]
fn invented_operands_rejected() {
    let err = err_of("literal a | deblock linend ANYCASE | hole");
    assert!(
        err.contains("unrecognized") || err.contains("unexpected") || err.contains("invalid xorc"),
        "{err}"
    );
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("DEBLOCK"));
    assert!(!reg.contains("DEBLOC"));
    assert!(!reg.contains("DEB"));
    let err = err_of("literal a | debloc linend | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("DEBLOC"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(&[b"a\n"]));
    let extra = b.add_stage("extra", emit_bytes(&[b"b"]));
    let stage = b.add_stage(
        "DEBLOCK",
        reg.create("DEBLOCK", vec!["LINEND".into()]).unwrap(),
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
    assert!(
        err.to_string()
            .contains("no input stream other than the primary"),
        "{err}"
    );
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
            for rec in ["one\n", "two\n", "three\n"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "DEBLOCK",
        reg.create("DEBLOCK", vec!["LINEND".into()]).unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "DEBLOCK should stop when no output is connected, wrote {}",
        written.get()
    );
}
