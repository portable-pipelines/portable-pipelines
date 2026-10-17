//! TAKE (p.542) and DROP (p.333) — Author's Edition 1.1.11.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

fn run_split(verb: &str, records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
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
                let _ = ctx.write(Record::from(r));
            }
        }),
    );
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
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

#[test]
fn take_book_last_selects_the_end() {
    // p.542 example shape: take last keeps the last record.
    let (p, s) = run_split("TAKE", &["h1", "h2", "last"], &["LAST"]);
    assert_eq!(p, vec!["last".to_string()]);
    assert_eq!(s, vec!["h1".to_string(), "h2".to_string()]);
}

#[test]
fn drop_book_heading_line() {
    // p.334: drop 1 removes the heading line.
    let (p, s) = run_split("DROP", &["heading", "row"], &["1"]);
    assert_eq!(p, vec!["row".to_string()]);
    assert_eq!(s, vec!["heading".to_string()]);
}

#[test]
fn defaults_first_one() {
    let (p, _) = run_split("TAKE", &["a", "b"], &[]);
    assert_eq!(p, vec!["a".to_string()]);
    let (p, _) = run_split("DROP", &["a", "b"], &[]);
    assert_eq!(p, vec!["b".to_string()]);
}

#[test]
fn star_and_zero() {
    let (p, s) = run_split("TAKE", &["a", "b"], &["*"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string()]);
    assert!(s.is_empty());
    let (p, s) = run_split("DROP", &["a", "b"], &["0"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string()]);
    assert!(s.is_empty());
}

#[test]
fn bytes_rejected() {
    let err = run_spec("literal a | take bytes 2 | hole").unwrap_err();
    assert!(err.to_string().contains("BYTES"), "{err}");
    let err = run_spec("literal a | drop last bytes | hole").unwrap_err();
    assert!(err.to_string().contains("BYTES"), "{err}");
}

#[test]
fn invented_alias_rejected() {
    let err = run_spec("literal a | takelast 1 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown") || err.to_string().contains("TAKELAST"),
        "{err}"
    );
}
