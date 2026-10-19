//! COUNT (p.306), BUFFER (p.279), SORT (p.493) — Author's Edition 1.1.11.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::StageRegistry;

fn run_primary(verb: &str, records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let recs: Vec<String> = records.iter().map(|x| (*x).to_string()).collect();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for r in recs {
                let _ = ctx.write(Record::from(r));
            }
        }),
    );
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap(),
    );
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn count_book_words_example_shape() {
    // p.307: count words writes one number.
    assert_eq!(run_primary("COUNT", &["a b c", "d"], &["WORDS"]), vec!["4"]);
}

#[test]
fn count_lines_chars_order() {
    // Output order is characters, words, lines regardless of keyword order (p.306).
    assert_eq!(
        run_primary("COUNT", &["ab", "c"], &["LINES", "CHARACTERS"]),
        vec!["3 2"]
    );
}

#[test]
fn count_empty_min_is_infinity() {
    // p.306: no input → shortest is 2G-1, longest is 0.
    let out = run_primary("COUNT", &[], &["MINLINE", "MAXLINE"]);
    assert_eq!(out, vec![format!("{} 0", i32::MAX as u64)]);
}

#[test]
fn count_records_synonym() {
    assert_eq!(run_primary("COUNT", &["a", "b"], &["RECORDS"]), vec!["2"]);
}

#[test]
fn count_characters_min_abbrev_is_eight() {
    // CHARACTErs (p.306): CHARS is the documented short synonym; CHAR is not.
    assert_eq!(run_primary("COUNT", &["ab"], &["CHARS"]), vec!["2"]);
    let err = pipe_stages::run_spec("literal ab|count char|hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized"), "{err}");
}

#[test]
fn bare_count_is_rejected() {
    // p.306: specify what to count; at least one keyword is required.
    let err = pipe_stages::run_spec("literal ab|count|hole").unwrap_err();
    assert!(err.to_string().contains("at least one keyword"), "{err}");
}

#[test]
fn buffer_no_args_delays_until_eof() {
    assert_eq!(
        run_primary("BUFFER", &["a", "", "b"], &[]),
        vec!["a", "", "b"]
    );
}

#[test]
fn buffer_copies_of_subfile() {
    // p.279: BUFFER n [dstring] copies subfiles separated by null records.
    assert_eq!(
        run_primary("BUFFER", &["a", "b", "", "c"], &["2", "/**/"]),
        vec!["a", "b", "**", "a", "b", "", "c", "**", "c"]
    );
}

#[test]
fn sort_book_numeric_is_binary() {
    // p.494: literal 11 5 2 1 | split | sort → 1, 11, 2, 5
    assert_eq!(
        run_primary("SORT", &["11", "5", "2", "1"], &[]),
        vec!["1", "11", "2", "5"]
    );
}

#[test]
fn sort_unique_and_descending() {
    assert_eq!(
        run_primary("SORT", &["b", "a", "b"], &["UNIQUE"]),
        vec!["a", "b"]
    );
    assert_eq!(
        run_primary("SORT", &["a", "c", "b"], &["DESCENDING"]),
        vec!["c", "b", "a"]
    );
}

#[test]
fn sort_anycase_min_three() {
    // Railroad `ANYcase` (p.493): min 3, no ANYOF.
    assert_eq!(
        run_primary("SORT", &["b", "A", "a"], &["ANY"]),
        vec!["A", "a", "b"]
    );
    let err = pipe_stages::run_spec("literal a | sort an | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("unexpected"),
        "{err}"
    );
}

#[test]
fn sort_count_prefixes_ten_chars() {
    let out = run_primary("SORT", &["a", "b", "a"], &["COUNT"]);
    assert_eq!(out[0].len(), 11);
    assert!(out[0].ends_with('a'), "{}", out[0]);
    assert!(out[1].ends_with('b'), "{}", out[1]);
}

#[test]
fn sort_pad_rejected() {
    let err = pipe_stages::run_spec("literal a | sort pad 00 | hole").unwrap_err();
    assert!(err.to_string().contains("not implemented"), "{err}");
}

#[test]
fn count_secondary_copies_then_writes_counts() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from("x"));
            let _ = ctx.write(Record::from("y"));
        }),
    );
    let stage = b.add_stage("COUNT", reg.create("COUNT", vec!["LINES".into()]).unwrap());
    let sink_p = b.add_stage(
        "p",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                p.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    let sink_s = b.add_stage(
        "s",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                s.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["x".to_string(), "y".to_string()]);
    assert_eq!(*secondary.borrow(), vec!["2".to_string()]);
}
