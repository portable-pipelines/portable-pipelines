//! PICK — Author's Edition 1.1.11 pp. 454–456 (tutorial pp. 53–54, 68).

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
        "PICK",
        reg.create("PICK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing PICK"),
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
        "PICK",
        reg.create("PICK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing PICK"),
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
        "PICK",
        reg.create("PICK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing PICK"),
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

// --- Book examples (1.1.11 pp. 53–54, 456). Snippets have no Ready; listing;
// the asserted output is the selection the surrounding prose describes.

#[test]
fn pick_book_timestamp_before_8am() {
    // p.456: `pick 1.4 << "0800"` — four-character timestamp, before 8 am.
    let recs = &["0759 rest", "0800 rest", "0801 rest"];
    let (p, s) = run_both(recs, &["1.4", "<<", "\"0800\""]);
    assert_eq!(p, vec!["0759 rest"]);
    assert_eq!(s, vec!["0800 rest".to_string(), "0801 rest".into()]);
}

#[test]
fn pick_book_timestamp_interval_cascade() {
    // p.456: cascade `pick 1.4 >>= "0800" | pick 1.4 <<= "1559"`.
    let out = run_spec_collect(
        r#"literal 0759 0800 1200 1559 1600 | split | pick 1.4 >>= "0800" | pick 1.4 <<= "1559""#,
    );
    assert_eq!(out, vec!["0800", "1200", "1559"]);
}

#[test]
fn pick_book_two_timestamp_fields() {
    // p.456: `pick 23.14 >> 57.14` — ISO timestamps YYYYMMDDHHMMSS.
    // Build 70-column records: cols 23–36 and 57–70.
    fn rec(a: &str, b: &str) -> String {
        assert_eq!(a.len(), 14);
        assert_eq!(b.len(), 14);
        format!("{:22}{}{:20}{}", "", a, "", b)
    }
    let later = rec("20240102120000", "20240101120000");
    let earlier = rec("20240101120000", "20240102120000");
    let equal = rec("20240101120000", "20240101120000");
    let (p, s) = run_both(&[&later, &earlier, &equal], &["23.14", ">>", "57.14"]);
    assert_eq!(p, vec![later]);
    assert_eq!(s, vec![earlier, equal]);
}

#[test]
fn pick_book_word2_eq_word4() {
    // p.456: `pick word 2 == word 4` also selects records with one word only
    // (tutorial p.54: two null words are equal).
    let recs = &["a b c b", "a b c d", "only", "", "x y"];
    let (p, s) = run_both(recs, &["word", "2", "==", "word", "4"]);
    assert_eq!(p, vec!["a b c b", "only", ""]);
    assert_eq!(s, vec!["a b c d".to_string(), "x y".into()]);
}

#[test]
fn pick_book_at_least_two_words() {
    // p.456: `pick word 2 ¬== //`
    let recs = &["one", "one two", "", "a b c"];
    let (p, s) = run_both(recs, &["word", "2", "\u{00AC}==", "//"]);
    assert_eq!(p, vec!["one two", "a b c"]);
    assert_eq!(s, vec!["one".to_string(), "".into()]);
}

#[test]
fn pick_book_numeric_word_vs_columns() {
    // p.456: `pick word 2 > 37.5` — 37.5 is columns 37–41, not the number 37.5.
    fn rec(word2: &str, col: &str) -> String {
        assert_eq!(col.len(), 5);
        // word1, word2, then pad so column 37 is the start of `col`.
        let prefix = format!("w1 {word2}");
        let pad = 36usize.saturating_sub(prefix.len());
        format!("{}{}{}", prefix, " ".repeat(pad), col)
    }
    let hi = rec("10", "00005");
    let lo = rec("10", "00020");
    let (p, s) = run_both(&[&hi, &lo], &["word", "2", ">", "37.5"]);
    assert_eq!(p, vec![hi]);
    assert_eq!(s, vec![lo]);
}

#[test]
fn pick_book_numeric_word_vs_constant() {
    // p.456: `pick word 2 > /37.5/`
    let recs = &["n 40", "n 37.5", "n 10", "n 37.50"];
    let (p, s) = run_both(recs, &["word", "2", ">", "/37.5/"]);
    assert_eq!(p, vec!["n 40"]);
    assert_eq!(
        s,
        vec!["n 37.5".to_string(), "n 10".into(), "n 37.50".into()]
    );
}

#[test]
fn pick_book_figure_107_dates() {
    // p.53 Figure 107: `pick word 1 >>= /19940701/` — July 1994 and later.
    // p.54: also discards null and blank lines (missing word is empty).
    let recs = &["19940630 a", "19940701 b", "19941231 c", "", "   "];
    let (p, s) = run_both(recs, &["word", "1", ">>=", "/19940701/"]);
    assert_eq!(p, vec!["19940701 b", "19941231 c"]);
    assert_eq!(s, vec!["19940630 a".to_string(), "".into(), "   ".into()]);
}

#[test]
fn pick_book_figure_108_word_difference() {
    // p.54 Figure 108: `pick word 1 ¬== word 2`
    let recs = &["a a", "a b", "only", ""];
    let (p, s) = run_both(recs, &["word", "1", "/==", "word", "2"]);
    assert_eq!(p, vec!["a b", "only"]);
    assert_eq!(s, vec!["a a".to_string(), "".into()]);
}

#[test]
fn pick_book_figure_109_reject_exact_string() {
    // p.54 Figure 109: `pick 1-* ¬== /Reject these only./`
    let recs = &["Reject these only.", "Reject these only. more", "keep me"];
    let (p, s) = run_both(recs, &["1-*", "\\==", "/Reject these only./"]);
    assert_eq!(p, vec!["Reject these only. more", "keep me"]);
    assert_eq!(s, vec!["Reject these only."]);
}

#[test]
fn pick_book_figure_144_opcode() {
    // p.68 Figure 144: `pick word 2 == /PIPERM/`
    let recs = &["LAB PIPERM 12", "LAB OTHER 12", "PIPERM"];
    let (p, s) = run_both(recs, &["word", "2", "==", "/PIPERM/"]);
    assert_eq!(p, vec!["LAB PIPERM 12"]);
    assert_eq!(s, vec!["LAB OTHER 12".to_string(), "PIPERM".into()]);
}

// --- End-to-end parser (quoted dstring + slash dstring)

#[test]
fn pick_parser_quoted_and_slash_dstring() {
    let out = run_spec_collect(r#"literal 0759|literal 0800|literal 1600 | pick 1.4 << "0800""#);
    assert_eq!(out, vec!["0759"]);
    let out = run_spec_collect("literal 0759|literal 0800 | pick 1.4 << /0800/");
    assert_eq!(out, vec!["0759"]);
}

#[test]
fn pick_hex_dstring_is_raw_ascii_bytes() {
    // Portable ASCII: "0800" is x30383030. The book's xf1f2f3 → 123 example is EBCDIC.
    let out = run_spec_collect("literal 0759|literal 0800 | pick 1.4 << x30383030");
    assert_eq!(out, vec!["0759"]);
    let recs = vec![b"AB".to_vec(), b"\x00\x41".to_vec()];
    let (p, s) = run_both_bytes(recs, &["1-*", "==", "x4142"]);
    assert_eq!(p, vec![b"AB".to_vec()]);
    assert_eq!(s, vec![b"\x00\x41".to_vec()]);
}

// --- Operators, PAD, ANYCASE, null records, streams

#[test]
fn pick_strict_operators_nopad() {
    let recs = &["ab", "abc", "aa", "ac"];
    let (p, _) = run_both(recs, &["1-*", "<<", "/abc/"]);
    assert_eq!(p, vec!["ab", "aa"]);
    let (p, _) = run_both(recs, &["1-*", "<<=", "/ab/"]);
    assert_eq!(p, vec!["ab", "aa"]);
    let (p, _) = run_both(recs, &["1-*", ">>", "/ab/"]);
    assert_eq!(p, vec!["abc", "ac"]);
    let (p, _) = run_both(&["ab", "abc"], &["1-*", ">>=", "/abc/"]);
    assert_eq!(p, vec!["abc"]);
    let (p, _) = run_both(&["ab", "abc"], &["1-*", "==", "/ab/"]);
    assert_eq!(p, vec!["ab"]);
}

#[test]
fn pick_nopad_null_byte_still_compares_high() {
    // Shorter is logically extended with a value that compares low against X'00' (p.455).
    let recs = vec![b"ab".to_vec(), b"ab\0".to_vec()];
    let (p, s) = run_both_bytes(recs, &["1-*", "<<", "x616200"]);
    assert_eq!(p, vec![b"ab".to_vec()]);
    assert_eq!(s, vec![b"ab\0".to_vec()]);
}

#[test]
fn pick_pad_extends_shorter_on_the_right() {
    let recs = vec![b"ab".to_vec(), b"ab\0".to_vec(), b"ac".to_vec()];
    let (p, s) = run_both_bytes(recs, &["PAD", "00", "1-*", "==", "x616200"]);
    assert_eq!(p, vec![b"ab".to_vec(), b"ab\0".to_vec()]);
    assert_eq!(s, vec![b"ac".to_vec()]);
}

#[test]
fn pick_anycase_string_compare() {
    let recs = &["Abc", "abc", "x"];
    let (p, s) = run_both(recs, &["ANYCASE", "1-*", "==", "/ABC/"]);
    assert_eq!(p, vec!["Abc", "abc"]);
    assert_eq!(s, vec!["x"]);
    let out = run_primary(&["Foo"], &["ANY", "1-*", "==", "/foo/"]);
    assert_eq!(out, vec!["Foo"]);
    let out = run_primary(&["Foo"], &["CASEANY", "1-*", "==", "/foo/"]);
    assert_eq!(out, vec!["Foo"]);
    let out = run_primary(&["Foo"], &["IGNORECASE", "1-*", "==", "/foo/"]);
    assert_eq!(out, vec!["Foo"]);
    let out = run_primary(&["Foo"], &["CASEIGNORE", "1-*", "==", "/foo/"]);
    assert_eq!(out, vec!["Foo"]);
}

#[test]
fn pick_numeric_operators_and_signs() {
    let recs = &["-1", "0", "-0", "1", "1.0", "2"];
    let (p, s) = run_both(recs, &["1-*", "=", "/1/"]);
    assert_eq!(p, vec!["1", "1.0"]);
    assert_eq!(s, vec!["-1", "0", "-0", "2"]);
    let (p, _) = run_both(recs, &["1-*", "<", "/0/"]);
    assert_eq!(p, vec!["-1"]);
    let (p, _) = run_both(&["0", "-0"], &["1-*", "<=", "/0/"]);
    assert_eq!(p, vec!["0", "-0"]);
    let (p, _) = run_both(&["0", "-0"], &["1-*", "=", "/0/"]);
    assert_eq!(p, vec!["0"]);
    let (p, _) = run_both(&["5", "5.0", "4"], &["1-*", ">=", "/5/"]);
    assert_eq!(p, vec!["5", "5.0"]);
    let (p, _) = run_both(&["5", "6"], &["1-*", "^=", "/5/"]);
    assert_eq!(p, vec!["6"]);
}

#[test]
fn pick_two_constants_select_all_or_none() {
    // Note 2 p.456.
    let recs = &["a", "b"];
    let (p, s) = run_both(recs, &["/a/", "==", "/a/"]);
    assert_eq!(p, vec!["a", "b"]);
    assert!(s.is_empty());
    let (p, s) = run_both(recs, &["/a/", "==", "/b/"]);
    assert!(p.is_empty());
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn pick_null_record_whole_range() {
    let recs = &["", "x", ""];
    let (p, s) = run_both(recs, &["1-*", "==", "//"]);
    assert_eq!(p, vec!["", ""]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn pick_secondary_output_routing() {
    let recs = &["keep", "drop", "keep2"];
    let (p, s) = run_both(recs, &["1-*", "==", "/keep/"]);
    assert_eq!(p, vec!["keep"]);
    assert_eq!(s, vec!["drop".to_string(), "keep2".into()]);
}

#[test]
fn pick_primary_only_discards_rejects() {
    let recs = &["keep", "drop"];
    let out = run_primary_only(recs, &["1-*", "==", "/keep/"]);
    assert_eq!(out, vec!["keep"]);
}

#[test]
fn pick_secondary_only_connected() {
    let reg = StageRegistry::default();
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["keep", "drop"]));
    let stage = b.add_stage(
        "PICK",
        reg.create("PICK", vec!["1-*".into(), "==".into(), "/keep/".into()])
            .unwrap(),
    );
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*secondary.borrow(), vec!["drop".to_string()]);
}

#[test]
fn pick_field_range() {
    let recs = &["a\tb\tc", "a\tx\tc"];
    let (p, s) = run_both(recs, &["field", "2", "==", "/b/"]);
    assert_eq!(p, vec!["a\tb\tc"]);
    assert_eq!(s, vec!["a\tx\tc"]);
}

#[test]
fn pick_not_sign_and_caret_synonyms() {
    let recs = &["aa", "ab"];
    for op in ["\u{00AC}==", "\\==", "/==", "^=="] {
        let (p, _) = run_both(recs, &["1-*", op, "/aa/"]);
        assert_eq!(p, vec!["ab"], "op {op}");
    }
    let recs = &["1", "2"];
    for op in ["\u{00AC}=", "\\=", "/=", "^="] {
        let (p, _) = run_both(recs, &["1-*", op, "/1/"]);
        assert_eq!(p, vec!["2"], "op {op}");
    }
}

#[test]
fn pick_explicit_nopad() {
    let recs = &["ab", "abc"];
    let (p, s) = run_both(recs, &["NOPAD", "1-*", "==", "/ab/"]);
    assert_eq!(p, vec!["ab"]);
    assert_eq!(s, vec!["abc"]);
}

#[test]
fn pick_space_pad_xorc() {
    let recs = &["ab", "ab ", "abc"];
    let (p, _) = run_both(recs, &["PAD", "BLANK", "1-*", "==", "/ab /"]);
    assert_eq!(p, vec!["ab", "ab "]);
}

// --- Operand errors

#[test]
fn pick_operand_errors() {
    let err = run_spec("literal a | pick | hole").unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");

    let err = run_spec("literal a | pick 1-* | hole").unwrap_err();
    assert!(err.to_string().contains("operator"), "{err}");

    let err = run_spec("literal a | pick 1-* == | hole").unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");

    let err = run_spec("literal a | pick 1-* == /a/ extra | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | pick 1-* AND /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("AND") && err.to_string().contains("cascade"),
        "{err}"
    );

    let err = run_spec("literal a | pick 1-* OR /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("OR"), "{err}");

    let err = run_spec("literal a | pick NOT 1-* == /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("NOT"), "{err}");

    let err = run_spec("literal a | pick 1-* eq /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | pick 1-* === /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | pick pad | hole").unwrap_err();
    assert!(err.to_string().contains("PAD"), "{err}");

    let err = run_spec("literal a | pick anycase pad 00 1-* == /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("diagram order") || err.to_string().contains("PAD"),
        "{err}"
    );

    let err = run_spec("literal a | pick 1-* == /a/ anycase | hole").unwrap_err();
    assert!(
        err.to_string().contains("diagram order") || err.to_string().contains("unexpected"),
        "{err}"
    );

    let err = run_spec("literal a | pick caseless 1-* == /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal abc | pick | hole").unwrap_err();
    assert!(err.to_string().contains("missing"), "{err}");
}

#[test]
fn pick_invalid_numeric_field_is_an_error() {
    let err = run_spec("literal abc | pick 1-* > /1/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("floating-point") || err.to_string().contains("not a"),
        "{err}"
    );
}

#[test]
fn pick_verb_is_not_abbreviable() {
    // Diagram is all-caps PICK (p.454); `pic` is not a verb abbreviation.
    assert!(StageRegistry::default().contains("PICK"));
    assert!(!StageRegistry::default().contains("PIC"));
    let err = run_spec("literal a | pic 1-* == /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("PIC"),
        "{err}"
    );
}

#[test]
fn pick_premature_termination_without_output() {
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
        "PICK",
        reg.create("PICK", vec!["1-*".into(), "==".into(), "/one/".into()])
            .unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "PICK should stop reading when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn pick_secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "PICK",
        reg.create("PICK", vec!["1-*".into(), "==".into(), "/a/".into()])
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
