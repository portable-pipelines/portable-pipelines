//! BETWEEN — Author's Edition 1.1.11 pp. 272–273.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

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
                let _ = ctx.write(Record::from(r));
            }
        }),
    );
    let stage = b.add_stage(
        "BETWEEN",
        reg.create("BETWEEN", args.iter().map(|a| (*a).to_string()).collect())
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
fn book_script_tags_are_prefixes() {
    // p.273: tags at the beginning of the record; match is "begins with".
    let (p, s) = run_split(
        &[":xmp.one", "body", ":exmp.", "after"],
        &["/:xmp./", "/:exmp./"],
    );
    assert_eq!(
        p,
        vec![
            ":xmp.one".to_string(),
            "body".to_string(),
            ":exmp.".to_string()
        ]
    );
    assert_eq!(s, vec!["after".to_string()]);
}

#[test]
fn match_is_prefix_not_contains() {
    let (p, s) = run_split(&["xxstart", "start here", "end"], &["/start/", "/end/"]);
    assert_eq!(p, vec!["start here".to_string(), "end".to_string()]);
    assert_eq!(s, vec!["xxstart".to_string()]);
}

#[test]
fn anycase_synonyms() {
    for kw in ["ANY", "ANYCASE", "CASEANY", "IGNORECASE", "CASEIGNORE"] {
        let (p, _) = run_split(&["START", "END"], &[kw, "/start/", "/end/"]);
        assert_eq!(p, vec!["START".to_string(), "END".to_string()], "{kw}");
    }
    let err = run_spec("literal a | between an /start/ /end/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("BETWEEN"),
        "{err}"
    );
}

#[test]
fn count_must_be_at_least_two() {
    let err = run_spec("literal a | between /x/ 1 | hole").unwrap_err();
    assert!(err.to_string().contains("at least 2"), "{err}");
}

#[test]
fn column_ranges_are_not_documented() {
    // pp.272–273 railroad is `[ANYcase] dstring (number | dstring)` — no
    // column range or mask. Reject as an unrecognized operand, not a subset.
    let err = run_spec("literal a | between 1-4 /x/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized operand"),
        "{err}"
    );
}

#[test]
fn hex_and_binary_dstrings() {
    // p.194 / p.272: a delimitedString may be hex or binary.
    let (p, _) = run_split(&["start here", "mid", "end"], &["x7374617274", "x656e64"]);
    assert_eq!(
        p,
        vec![
            "start here".to_string(),
            "mid".to_string(),
            "end".to_string()
        ]
    );
    let (p, _) = run_split(&["abc", "zzz"], &["x6162", "2"]);
    assert_eq!(p, vec!["abc".to_string(), "zzz".to_string()]);
}
