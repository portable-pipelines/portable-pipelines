//! OVERLAY — Author's Edition 1.1.11 p.447.
//!
//! Portable ASCII: xorc `BLANK`/`SPACE` is X'20' (the book cites EBCDIC X'40');
//! `TABULATE` is ASCII HT X'09' (book cites EBCDIC X'05'). Hex xorc such as
//! `40` is ASCII '@' (book X'40' is EBCDIC blank).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

fn emit_bytes(records: Vec<Vec<u8>>) -> Box<dyn FnOnce(&mut StageCtx)> {
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

fn run_spec_collect(spec: &str) -> Vec<String> {
    run_labelled(&format!("{spec} | >>COLLECT"))
}

fn run_labelled(spec: &str) -> Vec<String> {
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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    run_streams(&[records], args)
}

fn run_pair(primary: &[&str], secondary: &[&str], args: &[&str]) -> Vec<String> {
    run_streams(&[primary, secondary], args)
}

fn run_streams(inputs: &[&[&str]], args: &[&str]) -> Vec<String> {
    let recs: Vec<Vec<Vec<u8>>> = inputs
        .iter()
        .map(|stream| stream.iter().map(|s| s.as_bytes().to_vec()).collect())
        .collect();
    run_streams_bytes(&recs, args)
        .into_iter()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .collect()
}

fn run_streams_bytes(inputs: &[Vec<Vec<u8>>], args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage(
        "OVERLAY",
        reg.create("OVERLAY", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing OVERLAY"),
    );
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&out)));
    b.connect_primary(stage, snk);
    for (i, recs) in inputs.iter().enumerate() {
        let src = b.add_stage(&format!("src{i}"), emit_bytes(recs.clone()));
        b.connect(src, StreamId::PRIMARY, stage, StreamId(i as u32));
    }
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn one_stream_is_identity() {
    let out = run_primary(&["hello", "world"], &[]);
    assert_eq!(out, vec!["hello", "world"]);
    assert_eq!(run_spec_collect("literal hello| overlay"), vec!["hello"]);
}

#[test]
fn default_blank_pad_higher_stream_wins() {
    // Highest-numbered non-pad character wins (p.447).
    let out = run_pair(&["ABC"], &["  X"], &[]);
    assert_eq!(out, vec!["ABX"]);
}

#[test]
fn blank_keyword_is_the_default() {
    let out = run_pair(&["ABC"], &["  X"], &["BLANK"]);
    assert_eq!(out, vec!["ABX"]);
}

#[test]
fn pad_character_does_not_overlay() {
    let out = run_pair(&["XYZ"], &[" Q"], &[]);
    assert_eq!(out, vec!["XQZ"]);
}

#[test]
fn shorter_record_does_not_fill_missing_columns_with_pad() {
    // "present and not equal to the pad" (p.447): a missing column is not pad.
    let out = run_pair(&["ABCD"], &["X"], &[]);
    assert_eq!(out, vec!["XBCD"]);
    let out = run_pair(&["AB"], &["WXYZ"], &[]);
    assert_eq!(out, vec!["WXYZ"]);
}

#[test]
fn different_lengths_take_the_longest() {
    let out = run_pair(&["AB"], &["     "], &[]);
    assert_eq!(out, vec!["AB   "]);
}

#[test]
fn null_record_is_an_empty_contributor() {
    let out = run_pair(&["ABC"], &[""], &[]);
    assert_eq!(out, vec!["ABC"]);
    let out = run_pair(&[""], &[""], &[]);
    assert_eq!(out, vec![""]);
}

#[test]
fn three_streams_highest_non_pad_wins() {
    let out = run_streams(&[&["AAA"], &[" B"], &["  C"]], &[]);
    assert_eq!(out, vec!["ABC"]);
}

#[test]
fn lockstep_pairs_then_eof_on_one_continues() {
    // Ambiguity (p.447 has no EOF note): EOF on one stream drops that stream
    // from later cycles; remaining live streams continue. Stop when all EOF.
    let out = run_pair(&["AA", "BB", "CC"], &[" X"], &[]);
    assert_eq!(out, vec!["AX", "BB", "CC"]);
}

#[test]
fn eof_on_all_stops() {
    let out = run_pair(&["A", "B"], &["X", "Y"], &[]);
    assert_eq!(out, vec!["X", "Y"]);
}

#[test]
fn eof_primary_continues_with_secondary() {
    // Primary connected but empty: overlay the live secondary file.
    let out = run_pair(&[], &["hello", "world"], &[]);
    assert_eq!(out, vec!["hello", "world"]);
}

#[test]
fn xorc_star_pad() {
    let out = run_pair(&["A*C"], &["*B*"], &["*"]);
    assert_eq!(out, vec!["ABC"]);
    let out = run_pair(&["A*C"], &["*B*"], &["2a"]);
    assert_eq!(out, vec!["ABC"]);
}

#[test]
fn space_xorc_synonym() {
    // Shared xorc helper (p.197): SPACE ≡ BLANK. Portable ASCII X'20'.
    let out = run_pair(&["ABC"], &["  X"], &["SPACE"]);
    assert_eq!(out, vec!["ABX"]);
}

#[test]
fn two_hex_xorc_is_raw_byte() {
    // `overlay 40` is xorc X'40' (ASCII '@'), not a count.
    let out = run_pair(&["A@C"], &["@B@"], &["40"]);
    assert_eq!(out, vec!["ABC"]);
}

#[test]
fn book_example_spec_column_four() {
    // p.447: chop at '/', spec 2-* 4 into overlay. Portable ASCII (no EBCDIC).
    assert_eq!(
        run_labelled(
            "(ENDCHAR ?) literal the left/the right| c: chop before / | o: overlay | >>COLLECT ? c: | spec 2-* 4 | o:"
        ),
        vec!["thethefright"]
    );
}

#[test]
fn book_example_left_right_flush() {
    // p.447: spec 2-* 1.50 right. "the right" is 9 characters, so 41 pad
    // bytes then the word, overlaid onto "the left".
    let out = run_labelled(
        "(ENDCHAR ?) literal the left/the right| c: chop before / | o: overlay | >>COLLECT ? c: | spec 2-* 1.50 right | o:",
    );
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].len(), 50, "len {} rec {:?}", out[0].len(), out[0]);
    assert!(out[0].starts_with("the left"), "{:?}", out[0]);
    assert!(out[0].ends_with("the right"), "{:?}", out[0]);
    assert_eq!(&out[0][8..41], &" ".repeat(33));
}

#[test]
fn chop_into_overlay_does_not_stall() {
    // p.447: overlay can delay the primary record, so CHOP can write the
    // remainder to the secondary path.
    assert_eq!(
        run_labelled(
            "(ENDCHAR ?) literal ab/cd| c: chop before / | o: overlay | >>COLLECT ? c: | o:"
        ),
        vec!["/cd"]
    );
}

#[test]
fn spec_two_stream_label() {
    assert_eq!(
        run_labelled("(ENDCHAR ?) literal AAA| O: overlay | >>COLLECT ? literal  B| O:"),
        vec!["ABA"]
    );
}

#[test]
fn verb_abbrev_min_4() {
    assert!(StageRegistry::default().contains("OVER"));
    assert!(StageRegistry::default().contains("OVERL"));
    assert!(StageRegistry::default().contains("OVERLA"));
    assert!(StageRegistry::default().contains("OVERLAY"));
    assert!(!StageRegistry::default().contains("OVE"));
    assert_eq!(
        run_labelled("(ENDCHAR ?) literal ABC| O: over | >>COLLECT ? literal   X| O:"),
        vec!["ABX"]
    );
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | overlay pad | hole",
        "literal a | overlay anycase | hole",
        "literal a | overlay stop | hole",
        "literal a | overlay first | hole",
        "literal a | overlay count | hole",
        "literal a | overlay xor | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("invalid xorc") || err.contains("unexpected"),
            "{spec}: {err}"
        );
    }
    let err = err_of("literal a | overlay blank * | hole");
    assert!(err.contains("unexpected operand"), "{err}");
}

#[test]
fn secondary_output_rejected() {
    let err = err_of("(ENDCHAR ?) literal a | o: overlay | hole ? o: | hole");
    assert!(
        err.contains("only connected output") || err.contains("primary output"),
        "{err}"
    );
}

#[test]
fn no_output_shorts_input() {
    // Premature termination when the primary output is not connected (p.447).
    let produced = Rc::new(RefCell::new(0usize));
    let count = Rc::clone(&produced);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b", "c"] {
                if ctx.write(Record::from(rec)).is_ok() {
                    *count.borrow_mut() += 1;
                } else {
                    break;
                }
            }
        }),
    );
    let reg = StageRegistry::default();
    let stage = b.add_stage("OVERLAY", reg.create("OVERLAY", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        *produced.borrow() <= 1,
        "unconnected overlay must short so the producer is not parked; wrote {}",
        produced.borrow()
    );
}

#[test]
fn extra_input_streams_are_supported() {
    // Streams Used: all defined and connected inputs, beginning with stream 0.
    let out = run_streams(
        &[&["...."], &["A   "], &[" B  "], &["  C "], &["   D"]],
        &[],
    );
    assert_eq!(out, vec!["ABCD"]);
}

#[test]
fn multiple_record_pairs() {
    let out = run_pair(&["AA", "BB"], &[" X", " Y"], &[]);
    assert_eq!(out, vec!["AX", "BY"]);
}

#[test]
fn tabulate_pad() {
    let out = run_streams_bytes(&[vec![b"A\tC".to_vec()], vec![b"\tB\t".to_vec()]], &["TAB"]);
    assert_eq!(out, vec![b"ABC".to_vec()]);
}
