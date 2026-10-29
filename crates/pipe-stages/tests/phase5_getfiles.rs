//! GETFILES — host files named on input records.

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

fn sink(out: Rc<RefCell<Vec<String>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            out.borrow_mut().push(r.to_string_lossy());
        }
    })
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "cms-pipelines-getfiles-{}-{}",
        std::process::id(),
        name
    ))
}

fn run_getfiles(paths: &[&str]) -> Vec<String> {
    let (p, _) = run_split(paths, false);
    p
}

fn run_split(paths: &[&str], secondary: bool) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary_out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(paths));
    let stage = b.add_stage(
        "GETFILES",
        reg.create("GETFILES", vec![]).expect("GETFILES"),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    if secondary {
        let sink_s = b.add_stage("s", sink(Rc::clone(&secondary_out)));
        b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    }
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary_out.borrow().clone();
    (p, s)
}

#[test]
fn reads_files_named_on_input() {
    let a = tmp("a.txt");
    let b = tmp("b.txt");
    std::fs::write(&a, "one\r\ntwo\n").unwrap();
    std::fs::write(&b, "three\n").unwrap();
    let out = run_getfiles(&[a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(out, vec!["one", "two", "three"]);
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
}

#[test]
fn skips_blank_records() {
    let a = tmp("blank.txt");
    std::fs::write(&a, "ok\n").unwrap();
    let out = run_getfiles(&["", "  ", a.to_str().unwrap()]);
    assert_eq!(out, vec!["ok"]);
    let _ = std::fs::remove_file(&a);
}

#[test]
fn empty_file_writes_nothing() {
    let a = tmp("empty.txt");
    std::fs::write(&a, "").unwrap();
    let out = run_getfiles(&[a.to_str().unwrap()]);
    assert!(out.is_empty(), "{out:?}");
    let _ = std::fs::remove_file(&a);
}

#[test]
fn missing_file_is_an_error() {
    let path = tmp("no-such.txt");
    let _ = std::fs::remove_file(&path);
    let err = run_spec(&format!("literal {} | getfiles | hole", path.display()))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("GETFILES") || err.contains("file read") || err.contains("No such"),
        "{err}"
    );
}

#[test]
fn missing_file_goes_to_secondary() {
    let missing = tmp("missing-sec.txt");
    let good = tmp("good-sec.txt");
    let _ = std::fs::remove_file(&missing);
    std::fs::write(&good, "here\n").unwrap();
    let (p, s) = run_split(&[missing.to_str().unwrap(), good.to_str().unwrap()], true);
    assert_eq!(p, vec!["here"]);
    assert_eq!(s, vec![missing.to_str().unwrap().to_string()]);
    let _ = std::fs::remove_file(&good);
}

#[test]
fn must_not_be_first_and_rejects_operands() {
    let err = run_spec("getfiles | hole").unwrap_err().to_string();
    assert!(err.to_ascii_lowercase().contains("first"), "{err}");
    let err = run_spec("literal x | getfiles extra | hole")
        .unwrap_err()
        .to_string();
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn eof_after_last_file() {
    let a = tmp("eof.txt");
    std::fs::write(&a, "only\n").unwrap();
    assert_eq!(run_getfiles(&[a.to_str().unwrap()]), vec!["only"]);
    let _ = std::fs::remove_file(&a);
}
