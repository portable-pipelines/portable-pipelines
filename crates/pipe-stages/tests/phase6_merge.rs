//! MERGE — Author's Edition 1.1.11 pp.436–437.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId, TraceConfig};
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

fn run_merge(streams: &[&[&str]], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage(
        "MERGE",
        reg.create("MERGE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("MERGE"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(stage, snk);
    for (i, recs) in streams.iter().enumerate() {
        let src = b.add_stage(format!("src{i}"), emit(recs));
        b.connect(src, StreamId::PRIMARY, stage, StreamId(i as u32));
    }
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn run_collect(spec: &str) -> Vec<String> {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let sink_out = Rc::clone(&sink_out);
        move |_| pipe_stages::collect_to(Rc::clone(&sink_out))
    });
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn two_sorted_streams() {
    assert_eq!(
        run_merge(&[&["a", "c", "e"], &["b", "d"]], &[]),
        vec!["a", "b", "c", "d", "e"]
    );
}

#[test]
fn equal_keys_lowest_stream_first() {
    assert_eq!(
        run_merge(&[&["k", "z"], &["k", "m"]], &[]),
        vec!["k", "k", "m", "z"]
    );
}

#[test]
fn descending() {
    assert_eq!(
        run_merge(&[&["e", "c", "a"], &["d", "b"]], &["DESCENDING"]),
        vec!["e", "d", "c", "b", "a"]
    );
    assert_eq!(
        run_merge(&[&["e", "c", "a"], &["d", "b"]], &["d"]),
        vec!["e", "d", "c", "b", "a"]
    );
}

#[test]
fn anycase_and_range() {
    assert_eq!(
        run_merge(&[&["Ax", "Cz"], &["ay", "Bz"]], &["ANYCASE", "1"]),
        vec!["Ax", "ay", "Bz", "Cz"]
    );
}

#[test]
fn pad_and_field_order() {
    assert_eq!(
        run_merge(&[&["ab"], &["ab "]], &["PAD", "BLANK", "1.3"]),
        vec!["ab", "ab "]
    );
}

#[test]
fn per_field_descending_then_ascending() {
    assert_eq!(
        run_merge(
            &[&["b1", "a2"], &["b0", "a9"]],
            &["1", "DESCENDING", "2", "ASCENDING"]
        ),
        vec!["b0", "b1", "a2", "a9"]
    );
}

#[test]
fn caseany_synonym() {
    assert_eq!(
        run_merge(&[&["Ax"], &["ay"]], &["CASEANY"]),
        vec!["Ax", "ay"]
    );
}

#[test]
fn three_streams() {
    assert_eq!(
        run_merge(&[&["a", "g"], &["c", "e"], &["b", "d", "f"]], &[]),
        vec!["a", "b", "c", "d", "e", "f", "g"]
    );
}

#[test]
fn spec_label() {
    assert_eq!(
        run_collect(
            "(ENDCHAR ?) literal e|literal c|literal a| m: merge | >>COLLECT \
             ? literal d|literal b| m:"
        ),
        vec!["a", "b", "c", "d", "e"]
    );
}

#[test]
fn does_not_delay_write_before_consume() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["a", "b"]));
    let stage = b.add_stage("MERGE", reg.create("MERGE", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a", "b"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] MERGE "))
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
        vec!["W", "R", "W", "R"],
        "consume after copy; events={events:?}\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn secondary_output_rejected() {
    let err = err_of("(ENDCHAR ?) literal a | m: merge | hole ? literal b | m: | hole");
    assert!(
        err.to_ascii_uppercase().contains("PRIMARY") || err.contains("output"),
        "{err}"
    );
}

#[test]
fn invented_operands_rejected() {
    let err = err_of("literal a | merge unique | hole");
    assert!(
        err.contains("unrecognized") || err.contains("UNIQUE"),
        "{err}"
    );
    let err = err_of("literal a | merg | hole");
    assert!(err.contains("unknown stage"), "{err}");
}

#[test]
fn no_verb_prefix() {
    assert!(StageRegistry::default().contains("MERGE"));
    assert!(!StageRegistry::default().contains("MERG"));
}
