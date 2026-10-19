//! AGGRC — Author's Edition 1.1.11 pp.258–259
//! (host-command note p.39; PIPE aggregate p.219).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId, TraceConfig};
use pipe_stages::{run_spec, run_spec_with_return_code, StageRegistry};

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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "AGGRC",
        reg.create("AGGRC", args.iter().map(|a| (*a).to_string()).collect())
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn book_nonnegative_largest() {
    // p.258: `literal 0 99 3 6 | split | aggrc | console` → 99.
    assert_eq!(
        run_spec_collect("literal 0 99 3 6 | split | aggrc"),
        vec!["99"]
    );
}

#[test]
fn book_any_negative_is_most_negative() {
    // p.258: `literal 0 99 -3 6 | split | aggrc | console` → -3.
    assert_eq!(
        run_spec_collect("literal 0 99 -3 6 | split | aggrc"),
        vec!["-3"]
    );
}

#[test]
fn most_negative_among_several_negatives() {
    assert_eq!(
        run_spec_collect("literal -1 -10 5 | split | aggrc"),
        vec!["-10"]
    );
    assert_eq!(
        run_spec_collect("literal -3 -1 | split | aggrc"),
        vec!["-3"]
    );
}

#[test]
fn largest_among_zeros_and_positives() {
    assert_eq!(run_spec_collect("literal 0 1 0 | split | aggrc"), vec!["1"]);
    assert_eq!(run_spec_collect("literal 0 | aggrc"), vec!["0"]);
    assert_eq!(run_spec_collect("literal 3 3 3 | split | aggrc"), vec!["3"]);
}

#[test]
fn single_number_is_the_aggregate() {
    assert_eq!(run_spec_collect("literal 6 | aggrc"), vec!["6"]);
}

#[test]
fn interval_bounds_in_records() {
    assert_eq!(
        run_spec_collect("literal -2147483648|literal 2147483647 | aggrc"),
        vec!["-2147483648"]
    );
    assert_eq!(
        run_spec_collect("literal 2147483647|literal 0 | aggrc"),
        vec!["2147483647"]
    );
}

#[test]
fn leading_plus_is_ignored() {
    // p.258: a leading plus sign is ignored.
    assert_eq!(run_spec_collect("literal +6 +0 | split | aggrc"), vec!["6"]);
    assert_eq!(run_primary(&["+2147483647"], &[]), vec!["2147483647"]);
}

#[test]
fn surrounding_blanks_are_allowed() {
    assert_eq!(run_primary(&["  99  ", "\t6"], &[]), vec!["99"]);
}

#[test]
fn nonnegative_output_is_unsigned() {
    // p.258: a nonnegative number is unsigned (no plus, no pad).
    let got = run_spec_collect("literal +6 | aggrc");
    assert_eq!(got, vec!["6"]);
    assert!(!got[0].starts_with('+'));
}

#[test]
fn no_output_if_no_input() {
    // Note 2 p.259.
    assert!(run_spec_collect("literal a | drop | aggrc").is_empty());
}

#[test]
fn last_stage_sets_return_code() {
    // p.258–259: when last, the stage return code is the aggregate.
    // `(listerr) literal 6 | aggrc` → Ready(00006). LISTERR is not implemented;
    // the RC is still 6.
    assert_eq!(run_spec_with_return_code("literal 6 | aggrc").unwrap(), 6);
    assert_eq!(
        run_spec_with_return_code("literal 0 99 -3 6 | split | aggrc").unwrap(),
        -3
    );
    assert_eq!(
        run_spec_with_return_code("literal 0 99 3 6 | split | aggrc").unwrap(),
        99
    );
}

#[test]
fn last_stage_empty_input_keeps_rc_zero() {
    assert_eq!(
        run_spec_with_return_code("literal a | drop | aggrc").unwrap(),
        0
    );
}

#[test]
fn not_last_does_not_set_return_code() {
    // Output can be written, so the aggregate is a record, not the PIPE RC.
    assert_eq!(
        run_spec_with_return_code("literal 6 | aggrc | hole").unwrap(),
        0
    );
    assert_eq!(
        run_spec_with_return_code("literal 0 99 -3 6 | split | aggrc | hole").unwrap(),
        0
    );
}

#[test]
fn last_stage_still_reads_every_record() {
    // Premature termination is not listed: last-stage still drains input
    // and aggregates (p.258 delays all records until EOF).
    assert_eq!(
        run_spec_with_return_code("literal 1 2 3 4 | split | aggrc").unwrap(),
        4
    );
}

#[test]
fn delays_all_records_until_eof() {
    // p.258: delays all records until end-of-file — every READ, then one WRITE.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["0", "99", "3", "6"]));
    let stage = b.add_stage("AGGRC", reg.create("AGGRC", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["99"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] AGGRC "))
        .filter_map(|l| {
            if l.contains(" WRITE ") && l.contains(" len=") {
                Some("W")
            } else if l.contains(" READ ") && l.contains(" len=") {
                Some("R")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        events,
        vec!["R", "R", "R", "R", "W"],
        "AGGRC must read to EOF before writing; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn last_stage_sets_rc_without_writing() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["6"]));
    let stage = b.add_stage(
        "AGGRC",
        StageRegistry::default().create("AGGRC", vec![]).unwrap(),
    );
    b.connect_primary(src, stage);
    let rc = b.run_with_return_code().unwrap();
    assert_eq!(rc, 6);
    let wrote = trace
        .borrow()
        .iter()
        .any(|l| l.contains("] AGGRC ") && l.contains(" WRITE "));
    let set_rc = trace
        .borrow()
        .iter()
        .any(|l| l.contains("] AGGRC ") && l.contains(" RC 6"));
    assert!(
        !wrote,
        "last-stage AGGRC must not write; trace:\n{}",
        trace.borrow().join("\n")
    );
    assert!(
        set_rc,
        "last-stage AGGRC must set RC; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn secondary_output_is_unused() {
    // Streams Used is omitted (p.258). Primary in / primary out; a connected
    // secondary output is unused (not rejected), matching DUPLICATE/NOEOFBACK.
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["3", "9", "1"]));
    let stage = b.add_stage("AGGRC", reg.create("AGGRC", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["9"]);
    assert!(s.borrow().is_empty(), "secondary {s:?}", s = s.borrow());
}

#[test]
fn rejects_invented_operands() {
    for spec in [
        "literal 6 | aggrc first | hole",
        "literal 6 | aggrc strict | hole",
        "literal 6 | aggrc 1 | hole",
        "literal 6 | aggrc /6/ | hole",
        "literal 6 | aggrc max | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        assert!(
            err.to_string().contains("unrecognized operand"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn invalid_number_is_a_stage_error() {
    for spec in [
        "literal abc | aggrc | hole",
        "literal 0 99 | aggrc | hole",
        "literal 3.0 | aggrc | hole",
        "literal 2147483648 | aggrc | hole",
        "literal -2147483649 | aggrc | hole",
        "literal +-3 | aggrc | hole",
        "literal + | aggrc | hole",
        "literal| aggrc | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("AGGRC") && (msg.contains("not a number") || msg.contains("interval")),
            "{spec}: {err}"
        );
    }
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("AGGRC"));
    assert!(reg.contains("aggrc"));
    assert!(!reg.contains("AGGR"));
    assert!(!reg.contains("AGG"));
    let err = run_spec("literal 6 | aggr | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("AGGR"),
        "{err}"
    );
}

#[test]
fn run_spec_without_output_is_ok() {
    // Last-stage nonzero RC is not a pipeline error.
    run_spec("literal 6 | aggrc").unwrap();
}
