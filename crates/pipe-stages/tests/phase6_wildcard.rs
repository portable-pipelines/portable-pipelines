//! WILDCARD — Author's Edition 1.1.11 pp.597–598.
//!
//! LISTFILE-style pattern. Portable ASCII: blank is X'20' (book cites
//! EBCDIC X'40'). ANYCASE folds ASCII letters; metacharacters stay
//! case-sensitive.

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

fn run_split(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "WILDCARD",
        reg.create("WILDCARD", args.iter().map(|a| (*a).to_string()).collect())
            .expect("WILDCARD"),
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
fn star_matches_any_string_in_a_word() {
    let recs = ["abc", "a", "axxxc", "ab c", "xbc"];
    let (p, s) = run_split(&recs, &["/a*c/"]);
    assert_eq!(p, vec!["abc", "axxxc"]);
    assert_eq!(s, vec!["a", "ab c", "xbc"]);
}

#[test]
fn percent_matches_one_character() {
    let recs = ["abc", "ac", "abxc", "aXc"];
    let (p, s) = run_split(&recs, &["/a%c/"]);
    assert_eq!(p, vec!["abc", "aXc"]);
    assert_eq!(s, vec!["ac", "abxc"]);
}

#[test]
fn star_only_word_is_one_character() {
    let recs = ["a", "ab", "", "x"];
    let (p, s) = run_split(&recs, &["/*/"]);
    assert_eq!(p, vec!["a", "x"]);
    assert_eq!(s, vec!["ab", ""]);
}

#[test]
fn two_words_and_collapsed_blanks() {
    let recs = ["a b", "a  b", "ab", "a b c", "a b  "];
    let (p, s) = run_split(&recs, &["/a b/"]);
    assert_eq!(p, vec!["a b", "a  b", "a b  "]);
    assert_eq!(s, vec!["ab", "a b c"]);
}

#[test]
fn left_margin_rigorous_right_trailing_allowed() {
    let recs = ["a", " a", "a ", "  a"];
    let (p, s) = run_split(&recs, &["/a/"]);
    assert_eq!(p, vec!["a", "a "]);
    assert_eq!(s, vec![" a", "  a"]);
}

#[test]
fn pattern_trailing_blank_requires_trailing_delim() {
    let recs = ["a", "a ", "a  "];
    let (p, s) = run_split(&recs, &["/a /"]);
    assert_eq!(p, vec!["a ", "a  "]);
    assert_eq!(s, vec!["a"]);
}

#[test]
fn anycase_folds_literals() {
    let recs = ["ABC", "abc", "AbC", "xyz"];
    let (p, s) = run_split(&recs, &["ANYCASE", "/a*c/"]);
    assert_eq!(p, vec!["ABC", "abc", "AbC"]);
    assert_eq!(s, vec!["xyz"]);
}

#[test]
fn anycase_synonyms_min_four() {
    for kw in ["CASEANY", "IGNORECASE", "CASEIGNORE", "anycase", "ANYC"] {
        let (p, s) = run_split(&["A", "x"], &[kw, "/a/"]);
        assert_eq!(p, vec!["A"], "{kw}");
        assert_eq!(s, vec!["x"], "{kw}");
    }
    // Railroad is ANYCase (min 4); ANY is not ANYCASE (ANYCHARACTER is min 5).
    let err = err_of("literal a | wildcard any /a/ | hole");
    assert!(
        err.contains("unrecognized") || err.contains("delimited"),
        "{err}"
    );
}

#[test]
fn book_swap_metacharacters() {
    // p.598: `wildcard anychar off anystring % anychar * /*bcd%/`
    let recs = ["Abcd", "Xbcdrest", "bcd", "ABcd", "abc"];
    let (p, s) = run_split(
        &recs,
        &[
            "anychar",
            "off",
            "anystring",
            "%",
            "anychar",
            "*",
            "/*bcd%/",
        ],
    );
    assert_eq!(p, vec!["Abcd", "Xbcdrest"]);
    assert_eq!(s, vec!["bcd", "ABcd", "abc"]);
}

#[test]
fn input_range() {
    let recs = ["xxabcyy", "xxabxyy", "ab"];
    let (p, s) = run_split(&recs, &["3-5", "/abc/"]);
    assert_eq!(p, vec!["xxabcyy"]);
    // `ab` is a missing/short range → empty, does not match `/abc/`.
    assert_eq!(s, vec!["xxabxyy", "ab"]);
}

#[test]
fn word_range() {
    let recs = ["id abc", "id xyz"];
    let (p, s) = run_split(&recs, &["word", "2", "/a*/"]);
    assert_eq!(p, vec!["id abc"]);
    assert_eq!(s, vec!["id xyz"]);
}

#[test]
fn blank_off_spaces_are_literals() {
    let recs = ["a b", "a  b", "ab"];
    let (p, s) = run_split(&recs, &["BLANK", "OFF", "/a b/"]);
    assert_eq!(p, vec!["a b"]);
    assert_eq!(s, vec!["a  b", "ab"]);
}

#[test]
fn colliding_metacharacters_rejected() {
    let err = err_of("literal a | wildcard anystring % /x/ | hole");
    assert!(err.contains("different"), "{err}");
}

#[test]
fn missing_pattern_rejected() {
    let err = err_of("literal a | wildcard | hole");
    assert!(err.to_ascii_uppercase().contains("DELIMITED"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | wildcard string /a*/ | hole",
        "literal a | wildcard /a*/ extra | hole",
        "literal a | wildcard first /a*/ | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected") || err.contains("delimited"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn anychar_and_anystring_abbreviations() {
    let (p, _) = run_split(&["ab"], &["ANYCH", "?", "/a?/"]);
    assert_eq!(p, vec!["ab"]);
    let (p, _) = run_split(&["abc"], &["ANYSTR", "#", "/a#/"]);
    assert_eq!(p, vec!["abc"]);
}

#[test]
fn spec_form() {
    assert_eq!(
        run_spec_collect("literal abc|literal xyz| wildcard /a*/"),
        vec!["abc"]
    );
}

#[test]
fn not_wildcard_is_the_converse() {
    assert_eq!(
        run_spec_collect("literal abc|literal xyz| not wildcard /a*/"),
        vec!["xyz"]
    );
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("WILDCARD"));
    assert!(!reg.contains("WILDCA"));
    assert!(!reg.contains("WILD"));
    let err = err_of("literal a | wildcar /a/ | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("WILDCAR"),
        "{err}"
    );
}

#[test]
fn secondary_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage(
        "WILDCARD",
        reg.create("WILDCARD", vec!["/a/".into()]).unwrap(),
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
    let stage = b.add_stage(
        "WILDCARD",
        reg.create("WILDCARD", vec!["/*/".into()]).unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "WILDCARD should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn secondary_gets_rejected_records() {
    let (p, s) = run_split(&["abc", "xyz"], &["/a*/"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["xyz"]);
}
