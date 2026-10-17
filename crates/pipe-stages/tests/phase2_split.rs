use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
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

fn run_primary(records: &[&str], verb: &str, args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("missing {verb}")),
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
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "SPLIT",
        reg.create("SPLIT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing SPLIT"),
    );
    let sink_p = b.add_stage("sink_p", sink_bytes(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

#[test]
fn split_default_blanks() {
    let out = run_primary(&["one two three"], "SPLIT", &[]);
    assert_eq!(out, vec!["one", "two", "three"]);

    // Adjacent blanks would generate null records; Note 1 drops them.
    let out = run_primary(&["a  b   c"], "SPLIT", &[]);
    assert_eq!(out, vec!["a", "b", "c"]);

    let out = run_primary(&[" leading"], "SPLIT", &[]);
    assert_eq!(out, vec!["leading"]);

    let out = run_primary(&["trailing "], "SPLIT", &[]);
    assert_eq!(out, vec!["trailing"]);

    // Null input is copied (Note 1).
    let out = run_primary(&[""], "SPLIT", &[]);
    assert_eq!(out, vec![""]);
}

#[test]
fn split_book_example_literal_a_b_c() {
    // 1.1.11 p.505: pipe literal a b c | split | console
    let out = run_primary(&["a b c"], "SPLIT", &[]);
    assert_eq!(out, vec!["a", "b", "c"]);
}

#[test]
fn split_at_xrange_single_char() {
    let out = run_primary(&["a,b,c"], "SPLIT", &[","]);
    assert_eq!(out, vec!["a", "b", "c"]);

    let out = run_primary(&["x:y:z"], "SPLIT", &[":"]);
    assert_eq!(out, vec!["x", "y", "z"]);
}

#[test]
fn split_at_hex_xrange() {
    // `11` is xorc X'11', not a count of 11.
    let input = format!("field1{}field2", '\x11');
    let expected = format!("{}field2", '\x11');
    let out = run_primary(&[&input], "SPLIT", &["BEFORE", "11"]);
    assert_eq!(out, vec!["field1".to_string(), expected]);

    let input = format!("a{}b", '\x11');
    let out = run_primary(&[&input], "SPLIT", &["11"]);
    assert_eq!(out, vec!["a", "b"]);
}

#[test]
fn split_book_example_minimum_3_before_11() {
    // 1.1.11 p.505: split minimum 3 before 11
    let input = format!("ab{}cd{}ef", '\x11', '\x11');
    let out = run_primary(&[&input], "SPLIT", &["MINIMUM", "3", "BEFORE", "11"]);
    let expected = format!("ab{}cd", '\x11');
    let expected2 = format!("{}ef", '\x11');
    assert_eq!(out, vec![expected, expected2]);
}

#[test]
fn split_at_string_keyword() {
    let out = run_primary(&["one::two::three"], "SPLIT", &["STRING", "/::/"]);
    assert_eq!(out, vec!["one", "two", "three"]);

    let out = run_primary(&["a--b--c"], "SPLIT", &["STR", "/--/"]);
    assert_eq!(out, vec!["a", "b", "c"]);
}

#[test]
fn split_anyof_set() {
    let out = run_primary(&["a,b;c:d"], "SPLIT", &["ANYOF", "/,:;/"]);
    assert_eq!(out, vec!["a", "b", "c", "d"]);

    let out = run_primary(&["one two|three"], "SPLIT", &["ANY", "/ |/"]);
    assert_eq!(out, vec!["one", "two", "three"]);
}

#[test]
fn split_before_placement() {
    let out = run_primary(&["a,b,c"], "SPLIT", &["BEFORE", ","]);
    assert_eq!(out, vec!["a", ",b", ",c"]);

    let out = run_primary(&["x y z"], "SPLIT", &["BEFORE"]);
    assert_eq!(out, vec!["x", " y", " z"]);
}

#[test]
fn split_after_placement() {
    let out = run_primary(&["a,b,c"], "SPLIT", &["AFTER", ","]);
    assert_eq!(out, vec!["a,", "b,", "c"]);

    let out = run_primary(&["x y z"], "SPLIT", &["AFTER"]);
    assert_eq!(out, vec!["x ", "y ", "z"]);
}

#[test]
fn split_with_count() {
    // Count is only valid after a target; default blanks need explicit BLANK.
    let out = run_primary(&["a b c d e"], "SPLIT", &["BLANK", "2"]);
    assert_eq!(out, vec!["a", "b", "c d e"]);

    let out = run_primary(&["a,b,c,d"], "SPLIT", &[",", "1"]);
    assert_eq!(out, vec!["a", "b,c,d"]);

    let out = run_primary(&["one two three four"], "SPLIT", &["BLANK", "3"]);
    assert_eq!(out, vec!["one", "two", "three", "four"]);
}

#[test]
fn split_anycase() {
    let out = run_primary(&["AaBbCc"], "SPLIT", &["ANYCASE", "ANYOF", "/b/"]);
    assert_eq!(out, vec!["Aa", "Cc"]);

    let out = run_primary(&["oneTWOthree"], "SPLIT", &["ANYC", "STRING", "/TWO/"]);
    assert_eq!(out, vec!["one", "three"]);

    let out = run_primary(&["XxYyZz"], "SPLIT", &["CASEANY", "/x/"]);
    assert_eq!(out, vec!["YyZz"]);
}

#[test]
fn split_no_empty_pieces_generated() {
    // Records consisting entirely of the target are discarded.
    let out = run_primary(&[",,"], "SPLIT", &[","]);
    assert_eq!(out, Vec::<String>::new());

    let out = run_primary(&[",a,"], "SPLIT", &[","]);
    assert_eq!(out, vec!["a"]);

    let out = run_primary(&["a,,b"], "SPLIT", &[","]);
    assert_eq!(out, vec!["a", "b"]);
}

#[test]
fn split_with_snumber_offsets() {
    // 1 AFTER , splits one byte past the comma: "a,b,c" → "a,b" / ",c"
    let out = run_primary(&["a,b,c"], "SPLIT", &["1", "AFTER", ","]);
    assert_eq!(out, vec!["a,b", ",c"]);

    // Book: n AFTER ≡ m BEFORE with m = -n - length(target); m = -1 - 1 = -2.
    let eq = run_primary(&["a,b,c"], "SPLIT", &["-2", "BEFORE", ","]);
    assert_eq!(eq, out);
}

#[test]
fn split_not_keyword() {
    let out = run_primary(&["abc123def"], "SPLIT", &["NOT", "ANYOF", "/0123456789/"]);
    assert_eq!(out, vec!["123"]);
}

#[test]
fn split_xrange_character_ranges() {
    let out = run_primary(&["word123another"], "SPLIT", &["a-z"]);
    assert_eq!(out, vec!["123"]);

    let out = run_primary(&["abc123def"], "SPLIT", &["0-9"]);
    assert_eq!(out, vec!["abc", "def"]);
}

#[test]
fn split_xrange_from_count_and_wrap() {
    // a.3 is 'a','b','c'
    let out = run_primary(&["XaYbZcW"], "SPLIT", &["a.3"]);
    assert_eq!(out, vec!["X", "Y", "Z", "W"]);

    // ff.2 wraps X'FF' then X'00'
    let rec = vec![b'A', 0xff, 0x00, b'B'];
    let out = run_primary_bytes(vec![rec], &["ff.2"]);
    assert_eq!(out, vec![b"A".to_vec(), b"B".to_vec()]);
}

#[test]
fn split_blank_keyword() {
    let out = run_primary(&["one two three"], "SPLIT", &["BLANK"]);
    assert_eq!(out, vec!["one", "two", "three"]);

    let out = run_primary(&["one two three"], "SPLIT", &["SPACE"]);
    assert_eq!(out, vec!["one", "two", "three"]);
}

#[test]
fn split_minimum_skips_early_match() {
    let out = run_primary(&["abcXdefXghi"], "SPLIT", &["MINIMUM", "5", "/X/"]);
    assert_eq!(out, vec!["abcXdef", "ghi"]);
}

#[test]
fn split_operand_order_errors() {
    // Count without a target: a lone decimal that is not a 2-digit xorc and is
    // not in front of BEFORE/AFTER cannot be a count.
    let err = run_spec("literal a | split 123 | hole").unwrap_err();
    assert!(
        err.to_string()
            .contains("count is only valid after a target"),
        "{err}"
    );

    // BEFORE after the target is out of diagram order.
    let err = run_spec("literal a | split , before | hole").unwrap_err();
    assert!(
        err.to_string().contains("BEFORE") && err.to_string().contains("before the target"),
        "{err}"
    );

    let err = run_spec("literal a | split , 0 | hole").unwrap_err();
    assert!(
        err.to_string().contains("count must be at least 1"),
        "{err}"
    );
}

#[test]
fn split_two_is_xorc_not_count() {
    // One-character xorc '2', not "split at blanks twice".
    let out = run_primary(&["a2b2c"], "SPLIT", &["2"]);
    assert_eq!(out, vec!["a", "b", "c"]);

    let out = run_primary(&["a b c d"], "SPLIT", &["2"]);
    assert_eq!(out, vec!["a b c d"]);
}

#[test]
fn split_one_char_dstring_without_keyword() {
    let out = run_primary(&["a,b"], "SPLIT", &["/,/"]);
    assert_eq!(out, vec!["a", "b"]);
}

#[test]
fn split_multi_char_requires_keyword() {
    let err = run_spec("literal a | split /abc/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("multi-character") && err.to_string().contains("keyword"),
        "{err}"
    );
}

#[test]
fn split_end_to_end_through_parser() {
    run_spec("literal one,two,three | split , | hole").unwrap();
    run_spec("literal a b c | split | hole").unwrap();
    run_spec("literal x y z | split before | hole").unwrap();
    run_spec("literal test | split before 11 | hole").unwrap();
}

#[test]
fn split_abbreviations() {
    // ANYCASE min 4; ANYOF wins at 3.
    let out = run_primary(&["AaBb"], "SPLIT", &["ANYC", "/b/"]);
    assert_eq!(out, vec!["Aa"]);

    let out = run_primary(&["a,b;c"], "SPLIT", &["ANY", "/,;/"]);
    assert_eq!(out, vec!["a", "b", "c"]);

    let out = run_primary(&["abcXdefX"], "SPLIT", &["MIN", "5", "/X/"]);
    assert_eq!(out, vec!["abcXdef"]);

    let out = run_primary(&["a--b"], "SPLIT", &["STR", "/--/"]);
    assert_eq!(out, vec!["a", "b"]);
}

#[test]
fn split_case_synonyms() {
    let out = run_primary(&["AaBb"], "SPLIT", &["IGNORECASE", "/b/"]);
    assert_eq!(out, vec!["Aa"]);

    let out = run_primary(&["AaBb"], "SPLIT", &["CASEIGNORE", "/b/"]);
    assert_eq!(out, vec!["Aa"]);
}
