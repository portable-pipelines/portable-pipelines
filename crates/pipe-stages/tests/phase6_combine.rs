//! COMBINE — Author's Edition 1.1.11 pp.298–300.
//!
//! Portable ASCII: bitwise functions operate on raw bytes. The book's XOR
//! inequality map (p.299) uses X'00' as equal; that byte is the same on
//! ASCII. Hex examples such as `xf1f2f3` in other stages are EBCDIC and
//! are not used here.

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
    run_primary_bytes(
        records.iter().map(|s| s.as_bytes().to_vec()).collect(),
        args,
    )
    .into_iter()
    .map(|b| String::from_utf8_lossy(&b).into_owned())
    .collect()
}

fn run_primary_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "COMBINE",
        reg.create("COMBINE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing COMBINE"),
    );
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn run_pair(primary: &[&str], secondary: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(primary));
    let src_s = b.add_stage("src_s", emit(secondary));
    let stage = b.add_stage(
        "COMBINE",
        reg.create("COMBINE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing COMBINE"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn default_pairs_or() {
    // Default number 1: combine two records (p.298).
    // 'A'| 'B' = 'C' (0x41|0x42 = 0x43).
    let out = run_primary(&["AAA", "BBB", "AAA", "BBB"], &["OR"]);
    assert_eq!(out, vec!["CCC", "CCC"]);
}

#[test]
fn leftover_odd_record_at_eof() {
    let out = run_primary(&["AAA", "BBB", "ZZ"], &["OR"]);
    assert_eq!(out, vec!["CCC", "ZZ"]);
}

#[test]
fn number_zero_copies_each_record() {
    // Number is how many to combine *with the first* (p.298): 0 → one-record sets.
    let out = run_primary(&["a", "b", "c"], &["0", "OR"]);
    assert_eq!(out, vec!["a", "b", "c"]);
}

#[test]
fn number_two_combines_three() {
    let out = run_primary(&["A", "B", "C"], &["2", "OR"]);
    assert_eq!(out, vec!["C"]); // A|B|C = C
}

#[test]
fn star_combines_all() {
    let out = run_primary(&["A", "B", "A"], &["*", "OR"]);
    assert_eq!(out, vec!["C"]); // A|B|A = C
}

#[test]
fn empty_input_writes_nothing() {
    let out = run_primary(&[], &["OR"]);
    assert!(out.is_empty());
    let out = run_primary(&[], &["*", "LAST"]);
    assert!(out.is_empty());
}

#[test]
fn first_and_last_use_column_presence_not_nonzero() {
    // p.298: FIRST/LAST take the first/last record that *contains the column*.
    // A zero byte still contains that column; a shorter record does not.
    let out = run_primary_bytes(vec![vec![0x00, 0x00], b"AA".to_vec()], &["FIRST"]);
    assert_eq!(out, vec![vec![0x00, 0x00]]);
    let out = run_primary_bytes(vec![vec![0x00, 0x00], b"AA".to_vec()], &["LAST"]);
    assert_eq!(out, vec![b"AA".to_vec()]);
}

#[test]
fn different_lengths_are_not_padded() {
    // Note 2 p.300: the tail of the longer record is copied unmodified.
    let out = run_primary(&["AB", "WXYZ"], &["FIRST"]);
    assert_eq!(out, vec!["ABYZ"]);
    let out = run_primary(&["ABCD", "XY"], &["LAST"]);
    assert_eq!(out, vec!["XYCD"]);
    let out = run_primary(&["ABCD", "XY"], &["FIRST"]);
    assert_eq!(out, vec!["ABCD"]);
}

#[test]
fn and_or_xor_bitwise() {
    let out = run_primary(&["AB", "CD"], &["AND"]);
    // 0x41&0x43='A', 0x42&0x44='@'
    assert_eq!(out, vec!["A@"]);
    let out = run_primary_bytes(vec![b"AA".to_vec(), b"BB".to_vec()], &["X"]);
    assert_eq!(out, vec![vec![0x03, 0x03]]);
    let out = run_primary(&["AB", "CD"], &["O"]);
    assert_eq!(out, vec!["CF"]);
}

#[test]
fn book_xor_inequality_map_via_xlate() {
    // p.299: XOR then map 00 → "0" and 01-FF → "1". Portable ASCII (X'00'
    // is the same byte); DEBLOCK 1 is omitted (not in this port).
    assert_eq!(
        run_spec_collect("literal AAA| append literal BBB | combine x | xlate *-* 01-ff 1 00 0"),
        vec!["111"]
    );
    assert_eq!(
        run_spec_collect("literal AAA| append literal AAA | combine x | xlate *-* 01-ff 1 00 0"),
        vec!["000"]
    );
}

#[test]
fn keylength_leaves_key_and_combines_beyond() {
    // p.299: key unchanged; only positions beyond the key are combined.
    let out = run_primary(&["AA111", "AA222", "BB333"], &["KEYLENGTH", "2", "OR"]);
    assert_eq!(out, vec!["AA333", "BB333"]);
}

#[test]
fn keylength_singleton_copied_unchanged() {
    let out = run_primary(&["AA111", "BB222"], &["KEYLEN", "2", "OR"]);
    assert_eq!(out, vec!["AA111", "BB222"]);
}

#[test]
fn keylength_short_record_uses_whole_record_as_key() {
    // Same reading as JOIN p.399: a record shorter than the key uses the
    // whole record. "AB" vs "ABC" with KEYLENGTH 3 are different keys.
    let out = run_primary(&["AB", "ABC"], &["KEYLENGTH", "3", "OR"]);
    assert_eq!(out, vec!["AB", "ABC"]);
}

#[test]
fn null_records_are_empty_contributors() {
    let out = run_primary(&["", "AB"], &["FIRST"]);
    assert_eq!(out, vec!["AB"]);
    let out = run_primary(&["AB", ""], &["LAST"]);
    assert_eq!(out, vec!["AB"]);
    let out = run_primary(&[""], &["OR"]);
    assert_eq!(out, vec![""]);
}

#[test]
fn pair_mode_combines_primary_then_secondary() {
    let out = run_pair(&["AAA"], &["BBB"], &["OR"]);
    assert_eq!(out, vec!["CCC"]);
    let out = run_pair(&["ABCD"], &["XY"], &["FIRST"]);
    assert_eq!(out, vec!["ABCD"]);
    let out = run_pair(&["ABCD"], &["XY"], &["LAST"]);
    assert_eq!(out, vec!["XYCD"]);
}

#[test]
fn pair_stop_alleof_copies_leftover() {
    // Default STOP ALLEOF: continue while at least one input is connected.
    let out = run_pair(&["A", "B", "C"], &["X"], &["FIRST"]);
    assert_eq!(out, vec!["A", "B", "C"]);
    let out = run_pair(&["A"], &["X", "Y", "Z"], &["LAST"]);
    assert_eq!(out, vec!["X", "Y", "Z"]);
}

#[test]
fn pair_stop_anyeof_stops_at_first_eof() {
    let out = run_pair(&["A", "B", "C"], &["X"], &["STOP", "ANYEOF", "FIRST"]);
    assert_eq!(out, vec!["A"]);
    let out = run_pair(&["A", "B", "C"], &["X"], &["STOP", "1", "FIRST"]);
    assert_eq!(out, vec!["A"]);
}

#[test]
fn pair_stop_2_is_alleof() {
    let out = run_pair(&["A", "B", "C"], &["X"], &["STOP", "2", "FIRST"]);
    assert_eq!(out, vec!["A", "B", "C"]);
}

#[test]
fn pair_rejects_number_and_keylength() {
    let e = err_of("(ENDCHAR ?) literal A | C: combine 2 or | hole ? literal B | C:");
    assert!(
        e.contains("number or KEYLENGTH") || e.contains("two input"),
        "{e}"
    );
    let e = err_of("(ENDCHAR ?) literal A | C: combine keylength 2 or | hole ? literal B | C:");
    assert!(
        e.contains("number or KEYLENGTH") || e.contains("two input"),
        "{e}"
    );
}

#[test]
fn stop_rejected_on_one_stream() {
    let e = err_of("literal A | combine stop anyeof or | hole");
    assert!(e.contains("STOP"), "{e}");
}

#[test]
fn function_required_and_invented_rejected() {
    let e = err_of("literal A | combine | hole");
    assert!(e.contains("function is required"), "{e}");
    let e = err_of("literal A | combine xor | hole");
    assert!(e.contains("unexpected operand"), "{e}");
    let e = err_of("literal A | combine a | hole");
    assert!(e.contains("unexpected operand"), "{e}");
    let e = err_of("literal A | combine an | hole");
    assert!(e.contains("unexpected operand"), "{e}");
    let e = err_of("literal A | combine first 2 | hole");
    assert!(
        e.contains("diagram order") || e.contains("unexpected"),
        "{e}"
    );
    let e = err_of("literal A | combine pad or | hole");
    assert!(e.contains("unexpected operand"), "{e}");
}

#[test]
fn abbreviations_end_to_end() {
    assert_eq!(
        run_spec_collect("literal AA BB | split | combine o"),
        vec!["CC"]
    );
    assert_eq!(
        run_spec_collect("literal AA BB | split | combine n"),
        vec!["@@"] // 0x41 & 0x42 = 0x40
    );
    assert_eq!(
        run_spec_collect("literal AA BB | split | combine and"),
        vec!["@@"]
    );
    assert_eq!(
        run_spec_collect("literal AAA| append literal BBB | combine exc | xlate *-* 01-ff 1 00 0"),
        vec!["111"]
    );
}

#[test]
fn verb_is_exact() {
    let e = err_of("literal A | comb or | hole");
    assert!(e.contains("unknown stage"), "{e}");
    assert!(!StageRegistry::default().contains("COMB"));
    assert!(StageRegistry::default().contains("COMBINE"));
}

#[test]
fn secondary_output_unused_not_rejected() {
    run_spec("(ENDCHAR ?) literal AA BB | split | C: combine or | hole ? C: | hole").unwrap();
}

#[test]
fn tertiary_input_rejected() {
    let e = err_of(
        "(ENDCHAR ?) literal A | C: combine or | hole ? literal B | C: | hole ? literal D | C:",
    );
    assert!(
        e.contains("no input stream other than the primary and secondary") || e.contains("COMBINE"),
        "{e}"
    );
}

#[test]
fn no_primary_output_terminates() {
    // Premature termination (p.299): primary output not connected.
    run_spec("literal A B | split | combine or | hole").unwrap();
    run_spec("literal A | combine or").unwrap();
}
