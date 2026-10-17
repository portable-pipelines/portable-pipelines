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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    run_both(records, args).0
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
        "CHOP",
        reg.create("CHOP", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing CHOP"),
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
        "CHOP",
        reg.create("CHOP", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing CHOP"),
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

fn run_primary_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "CHOP",
        reg.create("CHOP", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing CHOP"),
    );
    let sink_p = b.add_stage("sink_p", sink_bytes(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

#[test]
fn chop_book_figure_121_string() {
    // 1.1.11 p.58 Figure 121: chop str /ccab/
    let out = run_primary(&["abcaabccabcaa"], &["STR", "/ccab/"]);
    assert_eq!(out, vec!["abcaab"]);
}

#[test]
fn chop_book_figure_122_anyof() {
    // 1.1.11 p.59 Figure 122: chop any /c /
    let out = run_primary(&["abcaabccabcaa"], &["ANY", "/c /"]);
    assert_eq!(out, vec!["ab"]);
}

#[test]
fn chop_book_example_blank() {
    // 1.1.11 p.289: chop blank — truncate before the first blank.
    let out = run_primary(&["one two three"], &["BLANK"]);
    assert_eq!(out, vec!["one"]);
}

#[test]
fn chop_book_figure_5_column_8() {
    // 1.1.11 p.6 / p.12: chop 8 keeps the first eight columns.
    let out = run_primary(&["VANILLA X"], &["8"]);
    assert_eq!(out, vec!["VANILLA "]);
}

#[test]
fn chop_book_figure_120_default_80() {
    // 1.1.11 p.58 Figure 120: default chops after column 80.
    // spec ,abcdefghij, 75 | chop keeps columns 1–80 of a record that has
    // "abcdefghij" at column 75 (74 blanks + 10 letters = 84 bytes).
    let rec = format!("{}abcdefghij", " ".repeat(74));
    assert_eq!(rec.len(), 84);
    let out = run_primary_only(&[&rec], &[]);
    assert_eq!(out[0].len(), 80);
    assert_eq!(&out[0][74..], "abcdef");
}

#[test]
fn chop_book_note1_not_z() {
    // 1.1.11 p.290 Note 1: chop not z truncates before the first non-z.
    let (p, s) = run_both(&["zzzzabc", "abc", "zzzz"], &["NOT", "z"]);
    assert_eq!(p, vec!["zzzz", "", "zzzz"]);
    assert_eq!(s, vec!["abc", "abc", ""]);
}

#[test]
fn chop_column_zero_is_null() {
    let (p, s) = run_both(&["abcd"], &["0"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec!["abcd"]);
}

#[test]
fn chop_does_not_pad() {
    let out = run_primary_only(&["hi"], &["80"]);
    assert_eq!(out, vec!["hi"]);
    let out = run_primary_only(&["hi"], &[]);
    assert_eq!(out, vec!["hi"]);
}

#[test]
fn chop_after_keeps_the_match() {
    let (p, s) = run_both(&["one two"], &["AFTER", "BLANK"]);
    assert_eq!(p, vec!["one "]);
    assert_eq!(s, vec!["two"]);
}

#[test]
fn chop_before_explicit() {
    let (p, s) = run_both(&["one two"], &["BEFORE", "BLANK"]);
    assert_eq!(p, vec!["one"]);
    assert_eq!(s, vec![" two"]);
}

#[test]
fn chop_snumber_after_equiv_before() {
    // p.59: n AFTER ≡ m BEFORE with m = -n - length(target).
    let a = run_primary(&["abcdef"], &["1", "AFTER", "STRING", "/cd/"]);
    let b = run_primary(&["abcdef"], &["-3", "BEFORE", "STRING", "/cd/"]);
    assert_eq!(a, b);
    assert_eq!(a, vec!["abcde"]);
}

#[test]
fn chop_negative_snumber_before() {
    // Negative BEFORE moves through the target (p.59). -2 BEFORE /cd/ ≡ AFTER /cd/.
    let out = run_primary(&["abcdef"], &["-2", "BEFORE", "STRING", "/cd/"]);
    assert_eq!(out, vec!["abcd"]);
}

#[test]
fn chop_not_string_match_length_one() {
    // p.59: NOT with a string skips occurrences; match length is 1.
    let (p, s) = run_both(&["ababX"], &["NOT", "STRING", "/ab/"]);
    assert_eq!(p, vec!["abab"]);
    assert_eq!(s, vec!["X"]);
}

#[test]
fn chop_anycase_string() {
    let out = run_primary(&["xxABCyy"], &["ANYCASE", "STRING", "/abc/"]);
    assert_eq!(out, vec!["xx"]);
    let out = run_primary(&["xxABCyy"], &["CASEANY", "STRING", "/abc/"]);
    assert_eq!(out, vec!["xx"]);
    let out = run_primary(&["xxABCyy"], &["IGNORECASE", "STRING", "/abc/"]);
    assert_eq!(out, vec!["xx"]);
}

#[test]
fn chop_xrange_and_one_char() {
    let out = run_primary(&["abc123"], &["0-9"]);
    assert_eq!(out, vec!["abc"]);
    let out = run_primary(&["a,b,c"], &[","]);
    assert_eq!(out, vec!["a"]);
    let out = run_primary(&["hello"], &["/,/"]);
    assert_eq!(out, vec!["hello"]);
}

#[test]
fn chop_xrange_from_count() {
    let out = run_primary(&["XaYb"], &["a.3"]);
    assert_eq!(out, vec!["X"]);
}

#[test]
fn chop_hex_and_binary_dstrings() {
    let out = run_primary(&["x::y"], &["STRING", "x3a3a"]);
    assert_eq!(out, vec!["x"]);
    let out = run_primary(&["a,b"], &["x2c"]);
    assert_eq!(out, vec!["a"]);
    let out = run_primary(&["a,b;c"], &["ANYOF", "x2c3b"]);
    assert_eq!(out, vec!["a"]);
    let out = run_primary(&["a-b"], &["b00101101"]);
    assert_eq!(out, vec!["a"]);
}

#[test]
fn chop_lone_number_is_column_not_xorc() {
    // Unlike SPLIT, `chop 11` truncates after column 11.
    let rec = "abcdefghijklmnop";
    let out = run_primary_only(&[rec], &["11"]);
    assert_eq!(out, vec!["abcdefghijk"]);
}

#[test]
fn chop_before_two_hex_is_xorc() {
    // `chop before 80` is BEFORE + xorc X'80', not column 80.
    let rec = vec![b'A', b'B', 0x80, b'C'];
    let out = run_primary_bytes(vec![rec], &["BEFORE", "80"]);
    assert_eq!(out, vec![b"AB".to_vec()]);
}

#[test]
fn chop_no_match_whole_record_null_remainder() {
    let (p, s) = run_both(&["abcdef"], &["STRING", "/zz/"]);
    assert_eq!(p, vec!["abcdef"]);
    assert_eq!(s, vec![""]);
}

#[test]
fn chop_null_records() {
    let (p, s) = run_both(&[""], &["80"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec![""]);

    let (p, s) = run_both(&[""], &["BLANK"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec![""]);

    let (p, s) = run_both(&[""], &["0"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec![""]);
}

#[test]
fn chop_secondary_output_remainder() {
    let (p, s) = run_both(&["abcdef"], &["3"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["def"]);
}

#[test]
fn chop_truncate_synonym() {
    let reg = StageRegistry::default();
    assert!(reg.contains("TRUNCATE"));
    assert!(reg.contains("TRUNC"));
    let out = run_spec_collect("literal abcdef | truncate 3");
    assert_eq!(out, vec!["abc"]);
    let out = run_spec_collect("literal abcdef | trunc 2");
    assert_eq!(out, vec!["ab"]);
}

#[test]
fn chop_end_to_end_through_parser() {
    run_spec("literal abcdef | chop 3 | hole").unwrap();
    run_spec("literal one two | chop blank | hole").unwrap();
    run_spec("literal abcaabccabcaa | chop str /ccab/ | hole").unwrap();
    run_spec("literal abcaabccabcaa | chop any /c / | hole").unwrap();
    run_spec("literal hello | chop after string /ll/ | hole").unwrap();
    run_spec("literal abc | chop not z | hole").unwrap();
    run_spec("(ENDCHAR ?) literal abcdef | C: chop 3 | hole ? C: | hole").unwrap();
}

#[test]
fn chop_parser_book_examples_produce_records() {
    let out = run_spec_collect("literal abcaabccabcaa | chop str /ccab/");
    assert_eq!(out, vec!["abcaab"]);
    let out = run_spec_collect("literal abcaabccabcaa | chop any /c /");
    assert_eq!(out, vec!["ab"]);
    let out = run_spec_collect("literal one two three | chop blank");
    assert_eq!(out, vec!["one"]);
}

#[test]
fn chop_multistream_parser() {
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
    let parsed =
        pipe_parser::parse_pipeline("(ENDCHAR ?) literal abcdef| C: chop 3 | >>P ? C: | >>S")
            .expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    assert_eq!(*collected_p.borrow(), vec!["abc".to_string()]);
    assert_eq!(*collected_s.borrow(), vec!["def".to_string()]);
}

#[test]
fn chop_operand_errors() {
    let err = run_spec("literal a | chop before | hole").unwrap_err();
    assert!(err.to_string().contains("target"), "{err}");

    let err = run_spec("literal a | chop after | hole").unwrap_err();
    assert!(err.to_string().contains("target"), "{err}");

    let err = run_spec("literal a | chop -2 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unexpected") || err.to_string().contains("snumber"),
        "{err}"
    );

    let err = run_spec("literal a | chop at blank | hole").unwrap_err();
    assert!(
        err.to_string().contains("AT") || err.to_string().contains("unexpected"),
        "{err}"
    );

    let err = run_spec("literal a | chop minimum 3 blank | hole").unwrap_err();
    assert!(
        err.to_string().contains("MINIMUM") || err.to_string().contains("unexpected"),
        "{err}"
    );

    let err = run_spec("literal a | chop string | hole").unwrap_err();
    assert!(err.to_string().contains("STRING"), "{err}");

    let err = run_spec("literal a | chop /abc/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("multi-character") && err.to_string().contains("keyword"),
        "{err}"
    );

    let err = run_spec("literal a | chop 3 blank | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | chop blank before | hole").unwrap_err();
    assert!(
        err.to_string().contains("diagram order") || err.to_string().contains("BEFORE"),
        "{err}"
    );

    // CASELESS is not an Author's Edition synonym.
    let err = run_spec("literal a | chop caseless /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");
}

#[test]
fn chop_abbreviations() {
    let out = run_primary(&["xxABCyy"], &["ANYC", "STR", "/abc/"]);
    assert_eq!(out, vec!["xx"]);
    let out = run_primary(&["a,b"], &["ANY", "/,/"]);
    assert_eq!(out, vec!["a"]);
}

#[test]
fn chop_space_xorc_synonym() {
    let out = run_primary(&["one two"], &["SPACE"]);
    assert_eq!(out, vec!["one"]);
}

#[test]
fn chop_premature_termination_without_output() {
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
    let stage = b.add_stage("CHOP", reg.create("CHOP", vec!["2".into()]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "CHOP should stop reading when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn chop_secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["ab"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage("CHOP", reg.create("CHOP", vec!["1".into()]).unwrap());
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
