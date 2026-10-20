//! SPILL — Author's Edition 1.1.11 pp.502–504.
//!
//! Portable ASCII: blank is X'20' (book cites EBCDIC X'40').

use std::cell::{Cell, RefCell};
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
    let (p, _) = run_split(records, args);
    p
}

fn run_split(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let recs: Vec<Vec<u8>> = records.iter().map(|s| s.as_bytes().to_vec()).collect();
    run_split_bytes(&recs, args)
}

fn run_split_bytes(records: &[Vec<u8>], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new({
            let records = records.to_vec();
            move |ctx: &mut StageCtx| {
                for rec in records {
                    if ctx.write(Record::from(rec)).is_err() {
                        break;
                    }
                }
            }
        }),
    );
    let stage = b.add_stage(
        "SPILL",
        reg.create("SPILL", args.iter().map(|a| (*a).to_string()).collect())
            .expect("SPILL"),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

#[test]
fn paragraph_example() {
    // p.504: flow a paragraph at width 12.
    assert_eq!(
        run_spec_collect("literal This is a paragraph to be flowed.| spill 12"),
        vec!["This is a", "paragraph to", "be flowed."]
    );
}

#[test]
fn numbered_list_offset() {
    // p.504: OFFSET number is that many blanks on continuation lines.
    let out = run_primary(&["2. A most important item."], &["14", "OFFSET", "4"]);
    assert_eq!(out, vec!["2. A most", "    important", "    item."]);
}

#[test]
fn long_word_split_without_secondary() {
    // p.504: very long words are split, not discarded, when secondary is absent.
    assert_eq!(
        run_spec_collect("literal abcdefghi klmnopq| spill 4"),
        vec!["abcd", "efgh", "i", "klmn", "opq"]
    );
}

#[test]
fn long_word_goes_to_secondary() {
    // p.504: remainder of the record goes to secondary.
    let (p, s) = run_split(&["abc defghi klmn opq"], &["4"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["defghi klmn opq"]);
}

#[test]
fn offset_prefixes_secondary_remainder() {
    let (p, s) = run_split(&["abc defghi klmn"], &["4", "OFFSET", "2"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["  defghi klmn"]);
}

#[test]
fn short_records_pass_unchanged() {
    assert_eq!(run_primary(&["ab", "abcd"], &["4"]), vec!["ab", "abcd"]);
}

#[test]
fn keep_retains_the_separator() {
    let out = run_primary(&["aa:bb:cc"], &["3", "STRING", "/:/", "KEEP"]);
    assert_eq!(out, vec!["aa:", "bb:", "cc"]);
}

#[test]
fn string_separator_without_keyword() {
    // STRING is the default of the separator branch (p.502).
    let out = run_primary(&["aa::bb::cc"], &["5", "/::/"]);
    assert_eq!(out, vec!["aa", "bb", "cc"]);
}

#[test]
fn anyof_separator() {
    let out = run_primary(&["aa,bb,cc"], &["4", "ANYOF", "/,/"]);
    assert_eq!(out, vec!["aa", "bb", "cc"]);
}

#[test]
fn not_anyof_separator() {
    // Complement set: split on any character not in the list.
    let out = run_primary(&["axbyc"], &["2", "NOT", "ANYOF", "/abc/"]);
    assert_eq!(out, vec!["a", "b", "c"]);
}

#[test]
fn notanyof_one_token() {
    // p.502 Note 1: blanks optional between NOT and ANYOF.
    let out = run_primary(&["axbyc"], &["2", "NOTANYOF", "/abc/"]);
    assert_eq!(out, vec!["a", "b", "c"]);
}

#[test]
fn anycase_string_separator() {
    let out = run_primary(&["aaXbbxcc"], &["4", "STRING", "/x/", "ANYCASE"]);
    assert_eq!(out, vec!["aa", "bb", "cc"]);
}

#[test]
fn remaining_keywords_any_order() {
    let a = run_primary(&["aa:bb"], &["4", "/:/", "KEEP", "OFFSET", "1"]);
    let b = run_primary(&["aa:bb"], &["4", "/:/", "OFFSET", "1", "KEEP"]);
    assert_eq!(a, b);
    assert_eq!(a, vec!["aa:", " bb"]);
}

#[test]
fn anycase_min_three_after_separator() {
    // Remaining `ANYcase` (p.502); `any` is ANYCASE once the separator is set.
    let out = run_primary(&["aaXbbxcc"], &["4", "/x/", "ANY"]);
    assert_eq!(out, vec!["aa", "bb", "cc"]);
}

#[test]
fn string_keyword_min_three() {
    let out = run_primary(&["aa:bb:cc"], &["3", "STR", "/:/"]);
    assert_eq!(out, vec!["aa", "bb", "cc"]);
}

#[test]
fn caseany_synonym() {
    // p.504 Note 2.
    let out = run_primary(&["aaXbb"], &["4", "STRING", "/x/", "CASEANY"]);
    assert_eq!(out, vec!["aa", "bb"]);
}

#[test]
fn offset_dstring() {
    let out = run_primary(&["aa bb cc dd"], &["5", "OFFSET", "/>/"]);
    assert_eq!(out[0], "aa bb");
    assert!(out[1].starts_with('>'), "{out:?}");
}

#[test]
fn missing_number_is_rejected() {
    let err = run_spec("literal a | spill | hole").unwrap_err();
    assert!(
        err.to_string().contains("positive number") || err.to_string().contains("required"),
        "{err}"
    );
}

#[test]
fn invented_operands_rejected() {
    let err = run_spec("literal a | spill 4 pad | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");
    let err = run_spec("literal a | spill 0 | hole").unwrap_err();
    assert!(err.to_string().contains("positive"), "{err}");
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("SPILL"));
    assert!(!reg.contains("SPIL"));
    let err = run_spec("literal a | spil 4 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("SPIL"),
        "{err}"
    );
}

#[test]
fn secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc def"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "SPILL",
        reg.create("SPILL", vec!["4".into()]).expect("SPILL"),
    );
    let hole = b.add_stage(
        "hole",
        Box::new(|ctx: &mut StageCtx| while ctx.read().ok().flatten().is_some() {}),
    );
    b.connect_primary(src, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string().to_ascii_uppercase().contains("SPILL"),
        "{err}"
    );
}

#[test]
fn premature_termination_without_output() {
    let written = Rc::new(Cell::new(0usize));
    let w = Rc::clone(&written);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["abc def ghi", "more words here"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                w.set(w.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "SPILL",
        reg.create("SPILL", vec!["4".into()]).expect("SPILL"),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "SPILL should stop when no output is connected, wrote {}",
        written.get()
    );
}
