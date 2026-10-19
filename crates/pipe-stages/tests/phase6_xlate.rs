//! XLATE — Author's Edition 1.1.11 pp.604–607 (tutorial pp.44–46).
//!
//! Portable ASCII: case tables affect A–Z / a–z only. Book examples that use
//! EBCDIC code points (X'40' blank, X'F1' digit 1, A–Z holes) are translated
//! to ASCII in the tests that say so.

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
        "XLATE",
        reg.create("XLATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("XLATE"),
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
        "XLATE",
        reg.create("XLATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("XLATE"),
    );
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn identity_table() -> Vec<u8> {
    (0u8..=255).collect()
}

fn run_with_secondary_table(records: &[&str], table: Vec<u8>, args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let tab = b.add_stage("tab", emit_bytes(vec![table]));
    let stage = b.add_stage(
        "XLATE",
        reg.create("XLATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("XLATE"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect(tab, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

#[test]
fn book_hello_world_upper() {
    // Tutorial Figure 82 p.44. ASCII letters match the EBCDIC example.
    assert_eq!(
        run_spec_collect("literal Hello, world|xlate upper"),
        vec!["HELLO, WORLD"]
    );
}

#[test]
fn no_arguments_defaults_to_upper() {
    // p.604: uppercase table when there are no arguments.
    assert_eq!(run_primary(&["AbC"], &[]), vec!["ABC"]);
}

#[test]
fn only_input_range_defaults_to_upper() {
    assert_eq!(run_primary(&["abcdEF"], &["1-3"]), vec!["ABCdEF"]);
}

#[test]
fn book_lower() {
    // Tutorial Figure 85 p.45.
    assert_eq!(
        run_spec_collect("literal Hello, WORLD|xlate lower"),
        vec!["hello, world"]
    );
}

#[test]
fn book_upper_then_pair_is_direct_mapping() {
    // p.606: `xlate upper a z` yields zBCABC, not ABCABC.
    // Modifications replace entries in the default table; they are not applied
    // after it (Note 3 p.607). ASCII letters, same as the book’s example.
    assert_eq!(
        run_spec_collect("literal abcABC|xlate upper a z"),
        vec!["zBCABC"]
    );
}

#[test]
fn book_odd_number_of_translate_pairs() {
    // Tutorial Figure 88 p.46: 01-80 is taken as a column range, 40 is leftover.
    let err = run_spec("literal abc|xlate 01-80 40 | hole").unwrap_err();
    assert!(
        err.to_string().contains("Odd number of translate pairs"),
        "{err}"
    );
}

#[test]
fn placeholder_star_star_for_neutral_pairs() {
    // Note 2 p.607: *-* when the first from-range would also be a valid range.
    // ASCII: map X'21'–X'2F' (punctuation) to blank. Book 40-7f / X'40' is EBCDIC.
    assert_eq!(
        run_spec_collect("literal Hello, world!|xlate *-* 21-2f blank"),
        vec!["Hello  world "]
    );
}

#[test]
fn reverse_case_with_upper_and_letter_ranges() {
    // Tutorial Figure 84 p.45 “To reverse the case”. On ASCII, A-Z / a-z are
    // contiguous (EBCDIC A-Z has holes, so the book’s printed file may differ).
    assert_eq!(
        run_primary(&["This"], &["UPPER", "A-Z", "a-z"]),
        vec!["tHIS"]
    );
}

#[test]
fn double_translation_applies_ranges_in_order() {
    // Tutorial Figure 86 p.45: (1-* 1.10) reverse-case twice on the first 10
    // columns. ASCII constructed record, not the book’s SMALL EXEC file.
    let rec = "abcdefghijklmnop";
    let out = run_primary(&[rec], &["(", "1-*", "1.10", ")", "UPPER", "A-Z", "a-z"]);
    // Whole record reverse-cased, then first 10 columns reverse-cased again
    // (back to original case); columns 11–16 stay reversed.
    assert_eq!(out, vec!["abcdefghijKLMNOP"]);
}

#[test]
fn book_caesar_cipher_operands() {
    // Tutorial Figure 89 p.46. Extra `i j r s` fill EBCDIC holes; on ASCII
    // they are already covered by a-y → b-z. Book form, ASCII letters.
    assert_eq!(
        run_spec_collect("literal hello|xlate a-y b-z i j r s z a"),
        vec!["ifmmp"]
    );
}

#[test]
fn ascii_contiguous_caesar_without_hole_pairs() {
    // Figure 89 says a-y b-z z a is unsafe on EBCDIC; on ASCII it is the
    // same Caesar cipher.
    assert_eq!(
        run_spec_collect("literal hello|xlate a-y b-z z a"),
        vec!["ifmmp"]
    );
}

#[test]
fn parenthesized_column_ranges() {
    // Tutorial Figure 83 p.45: `(10.5 *-5) upper`. `*-5` is columns 1–5
    // (`*` = start). ASCII constructed line, not the book’s SMALL EXEC file.
    let rec = "abcdefghijKLMNopqrst";
    let out = run_primary(&[rec], &["(", "10.5", "*-5", ")", "UPPER"]);
    // cols 1–5 abcde→ABCDE; cols 10–14 jKLMN→JKLMN.
    assert_eq!(out, vec!["ABCDEfghiJKLMNopqrst"]);
}

#[test]
fn word_range_positions_use_original_record() {
    // Note 5 p.607: translating blanks to a nonblank does not move words.
    // Word 2 of "a b c" is "b"; mapping blank→X must not change that.
    let out = run_primary(&["a b c"], &["WORD", "2", "blank", "X"]);
    assert_eq!(out, vec!["a b c"]);
    let out = run_primary(&["a b c"], &["1-*", "blank", "X"]);
    assert_eq!(out, vec!["aXbXc"]);
}

#[test]
fn sticky_last_to_character() {
    // p.606: 00-02 0-1 → 00→'0', 01 and 02→'1'. Book cites EBCDIC X'F1' for
    // '1'; portable ASCII uses X'31'.
    let rec = vec![0x00, 0x01, 0x02, 0x03];
    let out = run_primary_bytes(vec![rec], &["*-*", "00-02", "0-1"]);
    assert_eq!(out, vec![vec![b'0', b'1', b'1', 0x03]]);
}

#[test]
fn hex_character_and_blank_xorc_pairs() {
    // Note 2 p.607: a two-digit hex from-range is also a valid column number.
    assert_eq!(run_primary(&["abc"], &["*-*", "61", "41"]), vec!["Abc"]);
    assert_eq!(run_primary(&["a b"], &["blank", "2e"]), vec!["a.b"]);
}

#[test]
fn wraparound_from_range() {
    let rec = vec![0xfe, 0xff, 0x00, 0x01, 0x02];
    let out = run_primary_bytes(vec![rec], &["*-*", "fe-01", "41"]);
    assert_eq!(out, vec![vec![b'A', b'A', b'A', b'A', 0x02]]);
}

#[test]
fn input_and_output_are_identity() {
    // p.604: INPUT/OUTPUT default to the neutral table when no CMS SET is in
    // effect.
    assert_eq!(run_primary(&["AbC"], &["INPUT"]), vec!["AbC"]);
    assert_eq!(run_primary(&["AbC"], &["OUTPUT"]), vec!["AbC"]);
    assert_eq!(run_primary(&["AbC"], &["IN"]), vec!["AbC"]);
    assert_eq!(run_primary(&["AbC"], &["OUT"]), vec!["AbC"]);
}

#[test]
fn table_keyword_abbreviations() {
    assert_eq!(run_primary(&["ab"], &["UP"]), vec!["AB"]);
    assert_eq!(run_primary(&["AB"], &["LOW"]), vec!["ab"]);
    let err = run_spec("literal ab|xlate lo | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("Odd number"),
        "{err}"
    );
}

#[test]
fn translate_synonym_min_5() {
    // TRANSlate p.604.
    let reg = StageRegistry::default();
    assert!(reg.contains("TRANS"));
    assert!(reg.contains("TRANSLATE"));
    assert!(reg.contains("XLATE"));
    assert!(!reg.contains("TRAN"));
    assert!(!reg.contains("XLAT"));
    assert_eq!(run_spec_collect("literal ab|trans upper"), vec!["AB"]);
    let err = run_spec("literal ab | tran upper | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("TRAN"),
        "{err}"
    );
    let err = run_spec("literal ab | xlat upper | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("XLAT"),
        "{err}"
    );
}

#[test]
fn code_page_tables_are_rejected() {
    for spec in [
        "literal a|xlate a2e | hole",
        "literal a|xlate e2a | hole",
        "literal a|xlate to 819 | hole",
        "literal a|xlate from 37 | hole",
        "literal a|xlate to codepage 500 | hole",
        "literal a|xlate from codepage 1047 | hole",
        "literal a|xlate to 37-2 | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        assert!(
            err.to_string().contains("not implemented"),
            "{spec} → {err}"
        );
    }
}

#[test]
fn to_from_require_a_number() {
    let err = run_spec("literal a|xlate to | hole").unwrap_err();
    assert!(err.to_string().contains("code page number"), "{err}");
    let err = run_spec("literal a|xlate from codepage | hole").unwrap_err();
    assert!(err.to_string().contains("code page number"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    let err = run_spec("literal a|xlate anycase | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("Odd number"),
        "{err}"
    );
    let err = run_spec("literal a|xlate first | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("Odd number"),
        "{err}"
    );
    let err = run_spec("literal a|xlate strict | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("Odd number"),
        "{err}"
    );
}

#[test]
fn table_keyword_after_pairs_is_rejected() {
    // Fixed order: ranges, tables, then pairs. UPPER after a pair is leftover.
    let err = run_spec("literal abc|xlate a z upper | hole").unwrap_err();
    assert!(
        err.to_string().contains("Odd number of translate pairs")
            || err.to_string().contains("unrecognized"),
        "{err}"
    );
}

#[test]
fn secondary_table_is_initial_then_pairs() {
    // p.606: first 256 bytes of the secondary record are the initial table.
    let mut table = identity_table();
    table[b'a' as usize] = b'z';
    table[b'b' as usize] = b'y';
    assert_eq!(run_with_secondary_table(&["abc"], table, &[]), vec!["zyc"]);
}

#[test]
fn secondary_table_not_default_upper() {
    let table = identity_table();
    assert_eq!(run_with_secondary_table(&["abc"], table, &[]), vec!["abc"]);
}

#[test]
fn secondary_table_then_pair_patches_source_byte() {
    let mut table = identity_table();
    table[b'a' as usize] = b'Q';
    assert_eq!(
        run_with_secondary_table(&["abc"], table, &["a", "z"]),
        vec!["zbc"]
    );
}

#[test]
fn secondary_output_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let stage = b.add_stage("XLATE", reg.create("XLATE", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    let extra = b.add_stage(
        "extra",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect_primary(src, stage);
    b.connect_primary(stage, hole);
    b.connect(stage, StreamId::SECONDARY, extra, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("secondary output"), "{err}");
}

#[test]
fn null_records_stay_null() {
    assert_eq!(run_primary(&[""], &["UPPER"]), vec![""]);
}

#[test]
fn empty_input_writes_nothing() {
    assert!(run_primary(&[], &["UPPER"]).is_empty());
}

#[test]
fn compose_upper_then_lower() {
    assert_eq!(run_primary(&["AbC"], &["UPPER", "LOWER"]), vec!["abc"]);
}

#[test]
fn xrange_count_form_in_pairs() {
    // a.3 → a,b,c mapped onto X,Y,Z.
    assert_eq!(run_primary(&["abcd"], &["a.3", "X-Z"]), vec!["XYZd"]);
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
    let stage = b.add_stage("XLATE", reg.create("XLATE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "XLATE should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn substring_range() {
    let rec = "What a rum fellow.";
    // substr 2 of word 3 is 'u' → 'U'; rest unchanged.
    assert_eq!(
        run_primary(&[rec], &["SUBSTRING", "2", "OF", "WORD", "3", "UPPER"]),
        vec!["What a rUm fellow."]
    );
}
