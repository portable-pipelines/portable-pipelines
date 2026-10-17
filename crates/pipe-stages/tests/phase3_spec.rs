//! SPEC Phase 3 increment — Author's Edition 1.1.11.
//!
//! Tutorial Ch.14 pp.153–162, inventory pp.496–502, reference Ch.20
//! pp.615–628, inputRange pp.195–196, walk-through pp.48–51.
//! Printed hex examples that use EBCDIC (X'5C' asterisk, C2X of "First")
//! are translated to portable ASCII and marked as such.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::StageRegistry;

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
        "SPEC",
        reg.create("SPEC", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing SPEC"),
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
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in records {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        "SPEC",
        reg.create("SPEC", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing SPEC"),
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

fn spec_err(args: &[&str]) -> String {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["x"]));
    let stage = b.add_stage(
        "SPEC",
        reg.create("SPEC", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing SPEC"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap_err().to_string()
}

#[test]
fn specs_keyword_synonym() {
    assert!(StageRegistry::default().contains("SPEC"));
    assert!(StageRegistry::default().contains("SPECS"));
    assert_eq!(run_primary(&["abc"], &["1-*", "1"]), vec!["abc"]);
}

#[test]
fn figure_255_copy_record() {
    // p.154 Figure 255
    assert_eq!(
        run_primary(&["First record", "Second record"], &["1-*", "1"]),
        vec!["First record", "Second record"]
    );
}

#[test]
fn figure_256_first_word_at_column_5() {
    // p.154 Figure 256
    assert_eq!(
        run_primary(&["First record", "Second record"], &["word", "1", "5"]),
        vec!["    First", "    Second"]
    );
}

#[test]
fn figure_257_word_range_keeps_interior_blanks() {
    // p.155 Figure 257
    assert_eq!(
        run_primary(
            &["Here is the first record", "Here is the second record"],
            &["word", "2-4", "5"]
        ),
        vec!["    is the first", "    is the second"]
    );
}

#[test]
fn figure_258_nextword_squishes() {
    // p.155 Figure 258
    assert_eq!(
        run_primary(
            &["Here is the first record", "Here is the second record"],
            &["word", "2", "1", "word", "3", "nextword", "word", "4", "nextword"]
        ),
        vec!["is the first", "is the second"]
    );
}

#[test]
fn figure_259_fieldseparator_equals() {
    // p.155 Figure 259 — fields at columns 1, 6, 11, 16.
    let out = run_primary(
        &["a=b", "=a", "==a=b"],
        &[
            "fieldseparator",
            "=",
            "field",
            "1",
            "1",
            "field",
            "2",
            "6",
            "field",
            "3",
            "11",
            "field",
            "4",
            "16",
        ],
    );
    assert_eq!(out[0], "a    b");
    assert_eq!(out[1], "     a");
    assert_eq!(out[2], "          a    b");
}

#[test]
fn figure_260_penultimate_word() {
    // p.156 Figure 260
    assert_eq!(
        run_primary(
            &["First record is long", "Second record"],
            &["word", "-2", "1"]
        ),
        vec!["is", "Second"]
    );
}

#[test]
fn figure_261_semicolon_word_range_null_record() {
    // p.156 Figure 261 — opposite-end range that ends before it begins is null;
    // SPEC still writes the null record.
    assert_eq!(
        run_primary(
            &[
                "First record is long",
                "Second record",
                "Third one even more words"
            ],
            &["word", "2;-2", "1"]
        ),
        vec!["record is", "", "one even more"]
    );
}

#[test]
fn figure_262_word_start_length() {
    // p.156 Figure 262
    assert_eq!(
        run_primary(
            &["Here is the first record", "Here is the second record"],
            &["word", "2.3", "5"]
        ),
        vec!["    is the first", "    is the second"]
    );
}

#[test]
fn figure_263_substring_of_word() {
    // p.157 Figure 263
    assert_eq!(
        run_primary(
            &["This is the first record", "and the second record"],
            &["substring", "2;-2", "of", "word", "3", "1"]
        ),
        vec!["h", "econ"]
    );
}

#[test]
fn figure_264_nested_substring_separators() {
    // p.157 Figure 264
    assert_eq!(
        run_primary(
            &["a = 17", "x.18= 23"],
            &[
                "substring",
                "fieldsep",
                ".",
                "field",
                "1",
                "of",
                "substr",
                "word",
                "1",
                "of",
                "fs",
                "=",
                "f",
                "1",
                "1"
            ]
        ),
        vec!["a", "x"]
    );
}

#[test]
fn figure_265_literal_nextword() {
    // p.157 Figure 265
    assert_eq!(
        run_primary(
            &["First record", "Second record"],
            &["w1", "1", "/banana/", "nextword"]
        ),
        vec!["First banana", "Second banana"]
    );
}

#[test]
fn figure_266_hex_literal_ascii_asterisk() {
    // p.157 Figure 266 prints `x5c` (EBCDIC asterisk). Portable ASCII is `x2a`.
    assert_eq!(
        run_primary(
            &["First record", "Second record"],
            &["w1", "1", "x2a", "next", "w2", "next"]
        ),
        vec!["First*record", "Second*record"]
    );
}

#[test]
fn figure_267_number_and_literal() {
    // p.158 Figure 267
    assert_eq!(
        run_primary(
            &["First record", "Second record"],
            &["number", "1", "word", "-2", "nw", "/banana/", "nw"]
        ),
        vec!["         1 First banana", "         2 Second banana"]
    );
}

#[test]
fn figure_268_chop_first_word() {
    // p.158 Figure 268
    assert_eq!(
        run_primary(
            &["First record", "Second record"],
            &["w1", "1-3", "/*/", "next"]
        ),
        vec!["Fir*", "Sec*"]
    );
}

#[test]
fn figure_269_number_left_in_five() {
    // p.158 Figure 269 — 5-column LEFT field, then NEXT `*`.
    assert_eq!(
        run_primary(
            &["First record", "Second record"],
            &["number", "1.5", "left", "/*/", "next"]
        ),
        vec!["1    *", "2    *"]
    );
}

#[test]
fn figure_270_null_input_field_ignored() {
    // p.159 Figure 270 — missing word 3 does not pad to column 10.
    // The printed figure collapses the pads that word 3 at column 10 inserts
    // on the second record; we keep those blanks.
    assert_eq!(
        run_primary(
            &["First record", "Second record longer"],
            &["word", "1", "1", "word", "3", "10", "/*/", "next"]
        ),
        vec!["First*", "Second   longer*"]
    );
}

#[test]
fn figure_271_pad_asterisk() {
    // p.159 Figure 271
    assert_eq!(
        run_primary(
            &["First record", "Second record"],
            &["word", "1", "5", "pad", "*", "word", "2", "15"]
        ),
        vec!["    First*****record", "    Second****record"]
    );
}

#[test]
fn figure_272_pad_zero_number_right() {
    // p.159 Figure 272
    assert_eq!(
        run_primary(
            &["First record", "Second record"],
            &["pad", "0", "number", "1.5", "right", "w1", "nextword"]
        ),
        vec!["00001 First", "00002 Second"]
    );
}

#[test]
fn figure_273_c2x_portable_ascii() {
    // p.160 Figure 273 prints EBCDIC C2X (`C68999A2…`). ASCII of "First re"
    // is `46697273 74207265`.
    let out = run_primary(
        &["First record", "Second record", "Short"],
        &[
            "1.4", "c2x", "1", "5.4", "c2x", "nextword", "/*/", "19", "1.8", "next.8", "/*/",
            "next",
        ],
    );
    assert_eq!(out[0], "46697273 74207265 *First re*");
    assert_eq!(out[1], "5365636F 6E642072 *Second r*");
    assert_eq!(out[2], "53686F72 74       *Short   *");
}

#[test]
fn figure_94_shift_to_column_11() {
    // p.48 Figure 94
    assert_eq!(
        run_primary(&["abcd"], &["1-*", "11"]),
        vec!["          abcd"]
    );
}

#[test]
fn figure_95_prefix_literal() {
    // p.48 Figure 95
    assert_eq!(
        run_primary(&["abc"], &["/Input: /", "1", "1-*", "next"]),
        vec!["Input: abc"]
    );
}

#[test]
fn figure_97_negative_columns() {
    // p.49 Figure 97
    assert_eq!(
        run_primary(&["abcdefg", "ab", "abc"], &["2;-2", "1"]),
        vec!["bcdef", "", "b"]
    );
}

#[test]
fn figure_99_word_3() {
    // p.51 Figure 99
    assert_eq!(
        run_primary(&["Such a fine day."], &["word", "3", "1"]),
        vec!["fine"]
    );
}

#[test]
fn figure_100_wordseparator() {
    // p.51 Figure 100
    assert_eq!(
        run_primary(&["a***b***c"], &["wordsep", "*", "word", "2", "1"]),
        vec!["b"]
    );
}

#[test]
fn figure_101_field_and_fieldseparator() {
    // p.51 Figure 101 — `a***b***c` with FS `*` is fields a / '' / '' / b / …
    assert_eq!(
        run_primary(&["a***b***c"], &["fieldsep", "*", "field", "4", "1"]),
        vec!["b"]
    );
}

#[test]
fn figure_102_substring_of_word() {
    // p.51 Figure 102
    assert_eq!(
        run_primary(
            &["What a rum fellow."],
            &["substr", "2", "of", "word", "3", "1"]
        ),
        vec!["u"]
    );
}

#[test]
fn figure_274_read_pairs() {
    // p.161 Figure 274
    let out = run_primary(
        &["First record here", "Second line follows", "Odd record"],
        &[
            "word", "1", "1.4", "word", "2", "11.4", "word", "3", "21.4", "read", "/*****/", "26",
            "word", "1", "6.4", "word", "2", "16.4", "word", "3", "26",
        ],
    );
    assert_eq!(out[0], "Firs Seco reco line here follows");
    assert_eq!(out[1], "Odd       reco           *****");
}

#[test]
fn figure_275_readstop() {
    // p.161 Figure 275 — READSTOP drops the rest of the list at EOF.
    let out = run_primary(
        &["First record here", "Second line follows", "Odd record"],
        &[
            "word", "1", "1.4", "word", "2", "11.4", "word", "3", "21.4", "readstop", "/*****/",
            "26", "word", "1", "6.4", "word", "2", "16.4", "word", "3", "26",
        ],
    );
    assert_eq!(out[0], "Firs Seco reco line here follows");
    assert_eq!(out[1], "Odd       reco          ");
}

#[test]
fn figure_277_write_two_records() {
    // p.162 Figure 277
    let out = run_primary(
        &["First record", "Second line", "Short"],
        &[
            "word", "1", "1", "number", "15.5", "right", "/#1/", "12", "write", "word", "2", "1",
            "/#2/", "12", "number", "15.5", "right",
        ],
    );
    assert_eq!(
        out,
        vec![
            "First      #1     1",
            "record     #2     1",
            "Second     #1     2",
            "line       #2     2",
            "Short      #1     3",
            "           #2     3",
        ]
    );
}

#[test]
fn mixed_literal_and_field() {
    assert_eq!(
        run_primary(
            &["red green blue"],
            &["WORD", "2", "1", "/*/", "next", "WORD", "3", "next"]
        ),
        vec!["green*blue"]
    );
}

#[test]
fn note4_single_field_defaults_to_column_1() {
    // p.502 Note 4
    assert_eq!(run_primary(&["abcd"], &["1-*"]), vec!["abcd"]);
    assert_eq!(run_primary(&["one two"], &["word", "1"]), vec!["one"]);
}

#[test]
fn conversions_c2x_x2c_c2b_b2c() {
    assert_eq!(run_primary(&["AB"], &["1-*", "c2x", "1"]), vec!["4142"]);
    assert_eq!(run_primary(&["4142"], &["1-*", "x2c", "1"]), vec!["AB"]);
    assert_eq!(run_primary(&["A"], &["1-*", "c2b", "1"]), vec!["01000001"]);
    assert_eq!(run_primary(&["01000001"], &["1-*", "b2c", "1"]), vec!["A"]);
    assert_eq!(run_primary(&["41"], &["1-*", "x2b", "1"]), vec!["01000001"]);
    assert_eq!(run_primary(&["01000001"], &["1-*", "b2x", "1"]), vec!["41"]);
}

#[test]
fn x2c_allows_internal_blanks() {
    assert_eq!(run_primary(&["41 42"], &["1-*", "x2c", "1"]), vec!["AB"]);
}

#[test]
fn nextfield_inserts_ascii_tab() {
    // p.628 NEXTFIELD: tab (book X'05'; portable ASCII HT X'09') if output is not null.
    let out = run_primary_bytes(vec![b"ab".to_vec()], &["1", "1", "2", "nextfield"]);
    assert_eq!(out, vec![b"a\tb".to_vec()]);
}

#[test]
fn alignment_centre_pads_right_heavier() {
    // p.500 / p.628: extra pad character goes on the right.
    assert_eq!(
        run_primary(&["ab"], &["1-*", "1.5", "centre"]),
        vec![" ab  "]
    );
}

#[test]
fn strip_before_placement() {
    assert_eq!(run_primary(&["  hi  "], &["1-*", "strip", "1"]), vec!["hi"]);
}

#[test]
fn null_literal_is_stored() {
    // p.499: a null literal is stored; a missing input field is not.
    let out = run_primary(&["x"], &[",,", "1", "/y/", "next"]);
    assert_eq!(out, vec!["y"]);
}

#[test]
fn empty_input_writes_null_when_fields_missing() {
    assert_eq!(run_primary(&[""], &["word", "1", "1"]), vec![""]);
}

#[test]
fn recno_synonym_and_from() {
    assert_eq!(
        run_primary(&["a", "b"], &["recno", "from", "0", "strip", "1"]),
        vec!["0", "1"]
    );
}

#[test]
fn abbreviations() {
    assert_eq!(run_primary(&["one two"], &["w", "2", "n"]), vec!["two"]);
    assert_eq!(
        run_primary(&["one two"], &["w1", "1", "w2", "nw"]),
        vec!["one two"]
    );
    assert_eq!(
        run_primary(&["one two"], &["w1", "1", "w2", "nextw"]),
        vec!["one two"]
    );
    assert_eq!(run_primary(&["xy"], &["1-*", "1.4", "l"]), vec!["xy  "]);
    assert_eq!(run_primary(&["xy"], &["1-*", "1.4", "r"]), vec!["  xy"]);
    assert_eq!(
        run_primary(&["xy"], &["1-*", "1.4", "center"]),
        vec![" xy "]
    );
    assert_eq!(
        run_primary(&["a,b"], &["fs", ",", "f", "2", "1"]),
        vec!["b"]
    );
    assert_eq!(
        run_primary(&["a**b"], &["ws", "*", "w", "2", "1"]),
        vec!["b"]
    );
}

#[test]
fn operand_errors() {
    let err = spec_err(&[]);
    assert!(err.contains("one or more items"), "{err}");

    let err = spec_err(&["1-*", "1", "bogus"]);
    assert!(err.contains("unexpected operand"), "{err}");

    let err = spec_err(&["1-*", "1-*"]);
    assert!(err.contains("asterisk"), "{err}");

    let err = spec_err(&["if", "1-*", "1"]);
    assert!(err.contains("not implemented"), "{err}");

    let err = spec_err(&["select", "1", "1-*", "1"]);
    assert!(err.contains("not implemented"), "{err}");

    let err = spec_err(&["1-*", "c2d", "1"]);
    assert!(err.contains("not implemented"), "{err}");

    let err = spec_err(&["word", "1", "word", "2"]);
    assert!(err.contains("output placement"), "{err}");

    let err = spec_err(&["5-3", "1"]);
    assert!(err.contains("before the beginning"), "{err}");

    let err = spec_err(&["*"]);
    assert!(
        err.contains("unexpected operand") || err.contains("range"),
        "{err}"
    );

    let err = spec_err(&["1-*", "0"]);
    assert!(err.contains("positive"), "{err}");
}

#[test]
fn conversion_errors() {
    let err = spec_err(&["1-*", "x2c", "1"]);
    // input "x" is not even hex — stage fails at runtime
    assert!(
        err.contains("X2C") || err.contains("hex") || err.contains("SPEC"),
        "{err}"
    );
}

#[test]
fn end_to_end_through_parser() {
    // No blank before `|` — a space there is a trailing blank in LITERAL (p.412).
    let out = run_spec_collect("literal abc| spec /Input: / 1 1-* next");
    assert_eq!(out, vec!["Input: abc"]);

    let parsed =
        pipe_parser::parse_pipeline("literal a b c | spec word 2 1 /:/ next word 3 nextword")
            .expect("parse");
    assert_eq!(parsed.pipelines[0][1].verb.to_ascii_uppercase(), "SPEC");
    let args: Vec<&str> = parsed.pipelines[0][1]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(
        args,
        vec!["word", "2", "1", "/:/", "next", "word", "3", "nextword"]
    );

    let out = run_spec_collect("literal red green blue | spec w2 1 /*/ next w3 next");
    assert_eq!(out, vec!["green*blue"]);
}

#[test]
fn secondary_output_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let stage = b.add_stage(
        "SPEC",
        reg.create("SPEC", vec!["1-*".into(), "1".into()]).unwrap(),
    );
    let sink_p = b.add_stage("p", sink(Rc::new(RefCell::new(Vec::new()))));
    let sink_s = b.add_stage("s", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    let err = b.run().unwrap_err().to_string();
    assert!(err.contains("primary"), "{err}");
}

#[test]
fn empty_input_file_writes_nothing() {
    assert!(run_primary(&[], &["1-*", "1"]).is_empty());
}
