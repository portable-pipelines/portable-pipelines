//! CONSOLE / TERMINAL — Author's Edition 1.1.11 pp. 303–305.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
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

#[test]
fn terminal_is_a_documented_synonym() {
    let reg = StageRegistry::default();
    assert!(reg.contains("CONSOLE"));
    assert!(reg.contains("TERMINAL"));
}

#[test]
fn not_first_copies_through() {
    // p.304: when not first, write to the terminal and copy to primary output.
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["hello"]));
    let cons = b.add_stage("CONSOLE", reg.create("CONSOLE", vec![]).unwrap());
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, cons);
    b.connect_primary(cons, sink);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["hello".to_string()]);
}

#[test]
fn not_first_rejects_operands() {
    let err = run_spec("literal a | console eof // | hole").unwrap_err();
    assert!(
        err.to_string().contains("no argument") || err.to_string().contains("CONSOLE"),
        "{err}"
    );
}

#[test]
fn cms_direct_keywords_are_rejected() {
    for spec in [
        "console direct | hole",
        "console dark | hole",
        "console async | hole",
        "console invisible | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        assert!(
            err.to_string().contains("not implemented") || err.to_string().contains("CMS-specific"),
            "{spec}: {err}"
        );
    }
}
