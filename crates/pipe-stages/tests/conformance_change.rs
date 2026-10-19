//! CHANGE — Author's Edition 1.1.11 pp. 286–288.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::StageRegistry;

fn run_split(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let recs: Vec<String> = records.iter().map(|x| (*x).to_string()).collect();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for r in recs {
                if ctx.write(Record::from(r)).is_err() {
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        "CHANGE",
        reg.create("CHANGE", args.iter().map(|a| (*a).to_string()).collect())
            .unwrap(),
    );
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
    let primary = primary.borrow().clone();
    let secondary = secondary.borrow().clone();
    (primary, secondary)
}

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    run_split(records, args).0
}

#[test]
fn book_prefix_null_first_string() {
    // p.287: pipe literal abc|change //msg /|console → msg abc
    assert_eq!(run_primary(&["abc"], &["//msg /"]), vec!["msg abc"]);
}

#[test]
fn book_all_occurrences() {
    // p.288: change /ab/cd/ on abab and xabab
    assert_eq!(
        run_primary(&["abab", "xabab"], &["/ab/cd/"]),
        vec!["cdcd", "xcdcd"]
    );
}

#[test]
fn book_anycase_case_preservation() {
    // p.288 printed session. Railroad `ANYcase` (min 3).
    assert_eq!(
        run_primary(&["pipe"], &["ANY", "/pipe/line/"]),
        vec!["line"]
    );
    assert_eq!(
        run_primary(&["pipe"], &["ANYCASE", "/pipe/line/"]),
        vec!["line"]
    );
    assert_eq!(
        run_primary(&["Pipe"], &["ANYCASE", "/pipe/line/"]),
        vec!["Line"]
    );
    assert_eq!(
        run_primary(&["PiPe"], &["ANYCASE", "/pipe/line/"]),
        vec!["Line"]
    );
    assert_eq!(
        run_primary(&["PIpe"], &["ANYCASE", "/pipe/line/"]),
        vec!["LINE"]
    );
    assert_eq!(
        run_primary(&["PiPe"], &["ANYCASE", "/Pipe/line/"]),
        vec!["line"]
    );
    let err = pipe_stages::run_spec("literal a | change an /a/b/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("CHANGE"),
        "{err}"
    );
}

#[test]
fn two_delimited_strings_and_count() {
    assert_eq!(
        run_primary(&["xxxyxx"], &["/xx/", "/y/", "1"]),
        vec!["yxyxx"]
    );
}

#[test]
fn secondary_gets_unchanged() {
    let (p, s) = run_split(&["ab", "zz"], &["/ab/cd/"]);
    assert_eq!(p, vec!["cd".to_string()]);
    assert_eq!(s, vec!["zz".to_string()]);
}

#[test]
fn invented_first_only_alias_is_rejected() {
    let err = pipe_stages::run_spec("literal a | change first /a/ /b/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unexpected") || err.to_string().contains("CHANGE"),
        "{err}"
    );
}

#[test]
fn hex_and_binary_dstrings() {
    // p.194: two delimitedStrings may be hex or binary literals.
    assert_eq!(run_primary(&["abab"], &["x6162", "x6364"]), vec!["cdcd"]);
    assert_eq!(run_primary(&["xx"], &["x7878", "b01111001"]), vec!["y"]);
}
