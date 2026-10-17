//! LOCATE / NLOCATE — Author's Edition 1.1.11 pp. 414–415, 439–440.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::StageRegistry;

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
fn book_locate_discards_null_records() {
    // p.415: pipe literal abc | literal | locate | console → abc
    let (p, s) = run_split("LOCATE", &["abc", ""], &[]);
    assert_eq!(p, vec!["abc".to_string()]);
    assert_eq!(s, vec!["".to_string()]);
}

#[test]
fn book_nlocate_selects_null_records() {
    // p.440: nlocate with no operand selects null records.
    let (p, s) = run_split("NLOCATE", &["abc", ""], &[]);
    assert_eq!(p, vec!["".to_string()]);
    assert_eq!(s, vec!["abc".to_string()]);
}

#[test]
fn anycase_and_anyof() {
    let (p, _) = run_split("LOCATE", &["AbC", "zzz"], &["ANYCASE", "/abc/"]);
    assert_eq!(p, vec!["AbC".to_string()]);
    let (p, _) = run_split("LOCATE", &["q", "z"], &["ANYOF", "/xyz/"]);
    assert_eq!(p, vec!["z".to_string()]);
}

#[test]
fn hex_and_binary_dstrings() {
    // p.194: X/H + even hex; B + bits in multiples of 8.
    let (p, _) = run_split("LOCATE", &["abc", "zzz"], &["x616263"]);
    assert_eq!(p, vec!["abc".to_string()]);
    let (p, _) = run_split("LOCATE", &["hi", "no"], &["H6869"]);
    assert_eq!(p, vec!["hi".to_string()]);
    let (p, _) = run_split("LOCATE", &["a", "b"], &["b01100001"]);
    assert_eq!(p, vec!["a".to_string()]);
    let err = pipe_stages::run_spec("literal a | locate x1 | hole").unwrap_err();
    assert!(err.to_string().contains("even"), "{err}");
}

#[test]
fn notlocate_synonym_and_case_synonyms() {
    let (p, _) = run_split("NOTLOCATE", &["ax", "b"], &["/x/"]);
    assert_eq!(p, vec!["b".to_string()]);
    let (p, _) = run_split("LOCATE", &["Ab"], &["CASEANY", "/ab/"]);
    assert_eq!(p, vec!["Ab".to_string()]);
}

#[test]
fn bit_masks_and_ranges_stay_subset() {
    let err = pipe_stages::run_spec("literal a | locate mixed /x/ | hole").unwrap_err();
    assert!(err.to_string().contains("not implemented"), "{err}");
    let err = pipe_stages::run_spec("literal a | locate 1.4 /ab/ | hole").unwrap_err();
    assert!(err.to_string().contains("not implemented"), "{err}");
}
