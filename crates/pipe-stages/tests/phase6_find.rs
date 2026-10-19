//! FIND / NFIND — Author's Edition 1.1.11 pp.349–350, 438–439.
//!
//! Portable ASCII: XEDIT blank is X'20'; underscore is X'5F'. The book
//! describes the same rules on EBCDIC (blank X'40', underscore X'6D').

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
    // Do not put a blank before `|`: FIND keeps that blank as part of the
    // XEDIT string (p.349). Specs that already end with `|` stay as-is.
    let full = if spec.trim_end().ends_with('|') {
        format!("{spec} >>COLLECT")
    } else {
        format!("{spec}| >>COLLECT")
    };
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn run_split(verb: &str, records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("{verb}")),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = primary.borrow().clone();
    let secondary = secondary.borrow().clone();
    (primary, secondary)
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn book_find_dont_care_blank_four_byte_literals() {
    // p.350 first example: `literal axc | literal abc | find a c|`
    // Trailing blanks on the literals make four-byte records; FIND string is
    // three bytes (a, don't-care, c). Both selected. Literal cascade is reverse
    // order (p.413).
    assert_eq!(
        run_spec_collect("literal axc | literal abc | find a c|"),
        vec!["abc ", "axc "]
    );
}

#[test]
fn book_find_trailing_blank_in_string_drops_short_record() {
    // p.350 second example: `literal axc | literal abc| find a c |`
    // FIND string is four bytes (`a c `); the three-byte `abc` is not selected.
    assert_eq!(
        run_spec_collect("literal axc | literal abc| find a c |"),
        vec!["axc "]
    );
}

#[test]
fn book_find_two_blanks_discards_null_records() {
    // p.350: two blank characters after FIND (`find` + separator + a blank
    // in the string) require at least one character (don't-care).
    let (p, s) = run_split("FIND", &["", "a", "ab"], &[" "]);
    assert_eq!(p, vec!["a", "ab"]);
    assert_eq!(s, vec![""]);
    assert_eq!(run_spec_collect("literal a|literal | find  |"), vec!["a"]);
}

#[test]
fn book_nfind_example() {
    // p.439: `literal abc axc axy xyc | split | nfind a c|` → axy / xyc
    assert_eq!(
        run_spec_collect("literal abc axc axy xyc | split | nfind a c|"),
        vec!["axy", "xyc"]
    );
}

#[test]
fn null_string_matches_any_record() {
    // p.350: omitted string matches any record, including nulls.
    let (p, s) = run_split("FIND", &["", "abc"], &[]);
    assert_eq!(p, vec!["", "abc"]);
    assert!(s.is_empty());
    assert_eq!(
        run_spec_collect("literal abc|literal | find"),
        vec!["", "abc"]
    );
}

#[test]
fn nfind_null_string_selects_nothing() {
    // Converse: a null string matches every record, so NFIND writes none
    // to primary (all go to secondary).
    let (p, s) = run_split("NFIND", &["", "abc"], &[]);
    assert!(p.is_empty());
    assert_eq!(s, vec!["", "abc"]);
}

#[test]
fn underscore_is_required_blank() {
    let (p, s) = run_split("FIND", &["a b", "axb", "ab"], &["a_b"]);
    assert_eq!(p, vec!["a b"]);
    assert_eq!(s, vec!["axb", "ab"]);
}

#[test]
fn case_sensitive_mixed_respect() {
    // XEDIT CASE MIXED RESPECT: not ANYCASE (that is STRFIND, p.349/534).
    let (p, s) = run_split("FIND", &["abc", "Abc", "ABC"], &["abc"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["Abc", "ABC"]);
}

#[test]
fn prefix_not_substring() {
    // Unlike LOCATE, FIND matches the beginning of the record.
    let (p, s) = run_split("FIND", &["abc", "xabc", "ab"], &["abc"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["xabc", "ab"]);
}

#[test]
fn slashes_are_literal_not_a_dstring() {
    // FIND must not import LOCATE delimitedString scanning.
    let (p, s) = run_split("FIND", &["/abc/", "abc"], &["/abc/"]);
    assert_eq!(p, vec!["/abc/"]);
    assert_eq!(s, vec!["abc"]);
    assert_eq!(
        run_spec_collect("literal /abc/|literal abc| find /abc/"),
        vec!["/abc/"]
    );
}

#[test]
fn anycase_is_the_search_string_not_a_keyword() {
    // ANYCASE is STRFIND-only (p.349 railroad). FIND takes the remainder as
    // the XEDIT string, so `find ANYCASE abc` looks for that prefix.
    let (p, s) = run_split(
        "FIND",
        &["ANYCASE abc", "abc", "ANYCASE ABC"],
        &["ANYCASE abc"],
    );
    assert_eq!(p, vec!["ANYCASE abc"]);
    assert_eq!(s, vec!["abc", "ANYCASE ABC"]);
    assert_eq!(
        run_spec_collect("literal ANYCASE abc|literal abc| find ANYCASE abc"),
        vec!["ANYCASE abc"]
    );
}

#[test]
fn locate_anyof_is_not_a_find_operand() {
    let (p, s) = run_split("FIND", &["ANYOF /x/", "x"], &["ANYOF /x/"]);
    assert_eq!(p, vec!["ANYOF /x/"]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn hex_text_is_not_decoded() {
    // STRFIND `x02` is byte X'02' (p.534). FIND `x02` is the characters x, 0, 2.
    let (p, s) = run_split("FIND", &["x02", "\u{0002}rest"], &["x02"]);
    assert_eq!(p, vec!["x02"]);
    assert_eq!(s, vec!["\u{0002}rest"]);
}

#[test]
fn notfind_synonym() {
    // p.439 Note 1.
    assert_eq!(
        run_spec_collect("literal abc axc axy xyc | split | notfind a c|"),
        vec!["axy", "xyc"]
    );
}

#[test]
fn nfind_routes_matches_to_secondary() {
    let (p, s) = run_split("NFIND", &["abc", "axy", "xyc"], &["a c"]);
    assert_eq!(p, vec!["axy", "xyc"]);
    assert_eq!(s, vec!["abc"]);
}

#[test]
fn find_is_not_nlocate() {
    // NLOCATE `/a/` is substring; NFIND `a` is prefix.
    let (p, _) = run_split("NFIND", &["xa"], &["a"]);
    assert_eq!(p, vec!["xa"]);
    let (p, _) = run_split("NLOCATE", &["xa"], &["/a/"]);
    assert!(p.is_empty());
}

#[test]
fn remainder_blanks_are_kept() {
    // Leading blank in the string is don't-care in column 1.
    assert_eq!(
        run_spec_collect("literal xbc|literal abc| find  bc"),
        vec!["abc", "xbc"]
    );
}

#[test]
fn strfind_is_not_find() {
    let err = err_of("literal a | strfind x02 | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("STRFIND"),
        "{err}"
    );
    let err = err_of("literal a | strnfind x02 | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("STRNFIND"),
        "{err}"
    );
}

#[test]
fn verb_is_exact() {
    let err = err_of("literal a | fin abc | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("FIN"),
        "{err}"
    );
    let err = err_of("literal a | nfin abc | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("NFIN"),
        "{err}"
    );
}

#[test]
fn secondary_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("FIND", reg.create("FIND", vec!["a".into()]).unwrap());
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

#[test]
fn premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(Cell::new(0u32));
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
    let stage = b.add_stage("FIND", reg.create("FIND", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "FIND should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn discards_when_only_the_other_stream_is_connected() {
    // Matches discarded if primary output is not connected (p.350).
    let reg = StageRegistry::default();
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc", "xyz"]));
    let stage = b.add_stage("FIND", reg.create("FIND", vec!["a".into()]).unwrap());
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*secondary.borrow(), vec!["xyz".to_string()]);
}

#[test]
fn nested_word_split_args_are_joined_with_a_blank() {
    // IF/ZONE word-split the remainder; FIND joins those tokens with a blank.
    let (p, s) = run_split("FIND", &["abc", "axc", "axy"], &["a", "c"]);
    assert_eq!(p, vec!["abc", "axc"]);
    assert_eq!(s, vec!["axy"]);
    run_spec("(ENDCHAR ?) literal abc axc axy | split | if1: if find a c| copy | if1: | hole")
        .unwrap();
}
