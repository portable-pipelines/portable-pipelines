//! Stage-verb abbreviations from Author’s Edition 1.1.11 syntax diagrams.
//!
//! Only verbs whose diagram shows a lowercase suffix are abbreviable.
//! Operand abbreviations (ANYCASE, STRING, …) are tested with their stages.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
use pipe_stages::{run_spec, StageRegistry};

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

#[test]
fn unique_min_4() {
    // UNIQue p.572: uniq works; uni is shorter than the uppercase prefix.
    assert!(StageRegistry::default().contains("UNIQ"));
    assert!(StageRegistry::default().contains("UNIQU"));
    assert!(!StageRegistry::default().contains("UNI"));
    assert_eq!(
        run_spec_collect("literal a a a b b b a a a c c c | split | uniq"),
        vec!["a", "b", "a", "c"]
    );
    let err = run_spec("literal a | uni | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("UNI"),
        "{err}"
    );
}

#[test]
fn truncate_min_5() {
    // TRUNCate p.289: trunc works; trun is rejected.
    let reg = StageRegistry::default();
    assert!(reg.contains("TRUNC"));
    assert!(reg.contains("TRUNCA"));
    assert!(reg.contains("TRUNCAT"));
    assert!(reg.contains("TRUNCATE"));
    assert!(!reg.contains("TRUN"));
    assert_eq!(run_spec_collect("literal abcdef | trunc 3"), vec!["abc"]);
    let err = run_spec("literal abcdef | trun 3 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("TRUN"),
        "{err}"
    );
}

#[test]
fn console_and_terminal_min_4() {
    // CONSole / TERMinal p.303.
    let reg = StageRegistry::default();
    assert!(reg.contains("CONS"));
    assert!(reg.contains("CONSO"));
    assert!(reg.contains("CONSOL"));
    assert!(reg.contains("CONSOLE"));
    assert!(!reg.contains("CON"));
    assert!(reg.contains("TERM"));
    assert!(reg.contains("TERMINAL"));
    assert!(!reg.contains("TER"));
    assert!(reg.create("CONS", vec![]).is_some());
    assert!(reg.create("TERM", vec![]).is_some());
    assert!(reg.create("CON", vec![]).is_none());
}

#[test]
fn specs_min_4() {
    // SPECs p.496. SPEC is also the Appendix A verb (exact).
    let reg = StageRegistry::default();
    assert!(reg.contains("SPEC"));
    assert!(reg.contains("SPECS"));
    assert!(!reg.contains("SPE"));
}

#[test]
fn duplicate_min_3() {
    // DUPlicate p.334: dup works; du is rejected.
    assert!(StageRegistry::default().contains("DUP"));
    assert!(!StageRegistry::default().contains("DU"));
    assert_eq!(run_spec_collect("literal a|dup"), vec!["a", "a"]);
    let err = run_spec("literal a | du | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("DU"),
        "{err}"
    );
}

#[test]
fn all_caps_verbs_are_not_invented_abbreviations() {
    // CHOP, COPY, JOIN, SPLIT, PICK, ZONE, PREFACE, APPEND, LOOKUP, … diagrams are all caps.
    let reg = StageRegistry::default();
    assert!(reg.contains("CHOP"));
    assert!(!reg.contains("CHO"));
    assert!(reg.contains("COPY"));
    assert!(!reg.contains("COP"));
    assert!(reg.contains("JOIN"));
    assert!(!reg.contains("JOI"));
    assert!(reg.contains("SPLIT"));
    assert!(!reg.contains("SPLI"));
    assert!(reg.contains("PICK"));
    assert!(!reg.contains("PIC"));
    assert!(reg.contains("ZONE"));
    assert!(!reg.contains("ZON"));
    assert!(reg.contains("PREFACE"));
    assert!(!reg.contains("PREFAC"));
    assert!(reg.contains("APPEND"));
    assert!(!reg.contains("APPEN"));
    assert!(reg.contains("LOOKUP"));
    assert!(!reg.contains("LOOKU"));
    assert!(reg.contains("ELASTIC"));
    assert!(!reg.contains("ELASTI"));
    assert!(reg.contains("GATE"));
    assert!(!reg.contains("GAT"));
    assert!(reg.contains("NOEOFBACK"));
    assert!(!reg.contains("NOEOFBAC"));
    assert!(reg.contains("EOFBACK"));
    assert!(!reg.contains("EOFBAC"));
    assert!(!reg.contains("EOF"));
    assert!(reg.contains("AGGRC"));
    assert!(!reg.contains("AGGR"));
    assert!(!reg.contains("AGG"));
    assert!(reg.contains("NOTEOFBACK"));
    assert!(!reg.contains("NOTEOFBAC"));
    assert!(reg.contains("IF"));
    assert!(!reg.contains("I"));
    assert!(reg.contains("DAM"));
    assert!(!reg.contains("DA"));
    assert!(reg.contains("JUXTAPOSE"));
    assert!(!reg.contains("JUXTAPOS"));
    assert!(reg.contains("COMMAND"));
    assert!(!reg.contains("COMM"));
    // SYNChronise min 4 (p.536); SYNCHRONIZE is the documented synonym.
    assert!(reg.contains("SYNC"));
    assert!(reg.contains("SYNCHRONISE"));
    assert!(reg.contains("SYNCHRONIZE"));
    assert!(!reg.contains("SYN"));
    let err = run_spec("literal a | cho 2 | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn parser_resolves_abbreviated_verb() {
    run_spec("literal a a | split | uniq | hole").unwrap();
    run_spec("literal abcdef | trunc 3 | hole").unwrap();
    run_spec("literal a | dup | hole").unwrap();
    run_spec("literal a | sync | hole").unwrap();
}

#[test]
fn predselect_min_7() {
    // PREDSELect p.459: predsel works; predse is rejected.
    let reg = StageRegistry::default();
    assert!(reg.contains("PREDSEL"));
    assert!(reg.contains("PREDSELECT"));
    assert!(!reg.contains("PREDSE"));
    let err = run_spec("literal a | predse | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("PREDSE"),
        "{err}"
    );
}

#[test]
fn register_without_abbrev_stays_exact() {
    let mut reg = StageRegistry::new();
    let hit = Rc::new(RefCell::new(false));
    let flag = Rc::clone(&hit);
    reg.register("UNIQUE", move |_| {
        let flag = Rc::clone(&flag);
        Box::new(move |ctx: &mut StageCtx| {
            *flag.borrow_mut() = true;
            while ctx.read().ok().flatten().is_some() {}
        })
    });
    assert!(reg.create("UNIQUE", vec![]).is_some());
    assert!(reg.create("UNIQ", vec![]).is_none());

    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from("x"));
        }),
    );
    let stage = b.add_stage("UNIQUE", reg.create("UNIQUE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(*hit.borrow());
}
