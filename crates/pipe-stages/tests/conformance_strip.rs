//! STRIP — Author's Edition 1.1.11 pp. 532–533.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
use pipe_stages::{run_spec, StageRegistry};

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
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
        "STRIP",
        reg.create("STRIP", args.iter().map(|a| (*a).to_string()).collect())
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
fn book_trailing_blanks() {
    // p.533: strip trailing, then a marker shows the end.
    assert_eq!(run_primary(&["abc  "], &["TRAILING"]), vec!["abc"]);
}

#[test]
fn default_both_blank() {
    assert_eq!(run_primary(&["  x  "], &[]), vec!["x"]);
}

#[test]
fn string_and_xrange() {
    assert_eq!(run_primary(&["ababxabab"], &["STRING", "/ab/"]), vec!["x"]);
    assert_eq!(run_primary(&["12ab34"], &["0-9"]), vec!["ab"]);
}

#[test]
fn anycase_min_four_anyof_min_three() {
    // p.533 note 1: ANYCASE min 4 because ANYOF wins at 3.
    assert_eq!(
        run_primary(&["AaA"], &["ANYC", "ANYOF", "/a/"]),
        vec!["".to_string()]
    );
    assert_eq!(run_primary(&["xxaxx"], &["ANY", "/x/"]), vec!["a"]);
    let err = run_spec("literal a | strip cas /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized") || err.to_string().contains("STRIP"),
        "{err}"
    );
}

#[test]
fn not_and_count() {
    assert_eq!(run_primary(&["xxa  "], &["NOT", "1"]), vec!["xa  "]);
    assert_eq!(
        run_primary(&["  aa  "], &["BOTH", "1"]),
        vec![" aa ".to_string()]
    );
}
