//! GATHER — Author's Edition 1.1.11 pp.366–367.

use std::cell::{Cell, RefCell};
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

fn run_gather(streams: &[&[&str]], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let stage = b.add_stage(
        "GATHER",
        reg.create("GATHER", args.iter().map(|a| (*a).to_string()).collect())
            .expect("GATHER"),
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
fn round_robin_two_inputs() {
    assert_eq!(
        run_gather(&[&["a", "c"], &["b", "d"]], &[]),
        vec!["a", "b", "c", "d"]
    );
}

#[test]
fn skips_eof_streams() {
    assert_eq!(
        run_gather(&[&["a"], &["b", "d"], &["c"]], &[]),
        vec!["a", "b", "c", "d"]
    );
}

#[test]
fn stop_count_two() {
    let leftover = Rc::new(Cell::new(0usize));
    let flag = Rc::clone(&leftover);
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src0 = b.add_stage("src0", emit(&["a"]));
    let src1 = b.add_stage("src1", emit(&["b"]));
    let src2 = b.add_stage(
        "src2",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["c", "d"] {
                if ctx.write(Record::from(rec)).is_err() {
                    flag.set(flag.get() + 1);
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        "GATHER",
        reg.create("GATHER", vec!["STOP".into(), "2".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src0, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src1, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(src2, StreamId::PRIMARY, stage, StreamId::TERTIARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a", "b", "c"]);
    assert!(
        leftover.get() > 0,
        "STOP 2 must leave later input unconsumed"
    );
}

#[test]
fn stop_anyeof_stops_at_first_eof() {
    // Stream 0 has one record; after it EOFs, ANYEOF stops and leaves
    // remaining secondary records unconsumed.
    let leftover = Rc::new(Cell::new(0usize));
    let flag = Rc::clone(&leftover);
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src0 = b.add_stage("src0", emit(&["a"]));
    let src1 = b.add_stage(
        "src1",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["b", "c"] {
                if ctx.write(Record::from(rec)).is_err() {
                    flag.set(flag.get() + 1);
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        "GATHER",
        reg.create("GATHER", vec!["STOP".into(), "ANYEOF".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src0, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src1, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a", "b"]);
    assert!(
        leftover.get() > 0,
        "STOP ANYEOF must leave later input unconsumed"
    );
}

#[test]
fn streamid_zero_passes_primary() {
    assert_eq!(
        run_gather(&[&["0keep", "0also"]], &["STREAMID", "1"]),
        vec!["0keep", "0also"]
    );
}

#[test]
fn streamid_selects_other_stream() {
    assert_eq!(
        run_gather(&[&["1x", "0y"], &["from1"]], &["STREAMID", "1"]),
        vec!["from1", "0y"]
    );
}

#[test]
fn streamid_blank_is_primary() {
    assert_eq!(run_gather(&[&[" keep"]], &["STREAMID", "1"]), vec![" keep"]);
}

#[test]
fn streamid_abbrev_min_7() {
    assert_eq!(run_gather(&[&["0a"]], &["STREAMI", "1"]), vec!["0a"]);
    let err = err_of("literal a | gather stream 1 | hole");
    assert!(
        err.contains("unrecognized") || err.contains("unknown"),
        "{err}"
    );
}

#[test]
fn streamid_eof_stops_without_consuming_primary() {
    let leftover = Rc::new(Cell::new(false));
    let flag = Rc::clone(&leftover);
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src0 = b.add_stage(
        "src0",
        Box::new(move |ctx: &mut StageCtx| {
            if ctx.write(Record::from("1need")).is_err() {
                return;
            }
            if ctx.write(Record::from("0later")).is_err() {
                flag.set(true);
            }
        }),
    );
    let src1 = b.add_stage("src1", emit(&[]));
    let stage = b.add_stage(
        "GATHER",
        reg.create("GATHER", vec!["STREAMID".into(), "1".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src0, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src1, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert!(out.borrow().is_empty());
    assert!(
        leftover.get(),
        "STREAMID EOF must leave the primary record unconsumed"
    );
}

#[test]
fn spec_label() {
    assert_eq!(
        run_collect(
            "(ENDCHAR ?) literal c|literal a| g: gather | >>COLLECT \
             ? literal d|literal b| g:"
        ),
        vec!["a", "b", "c", "d"]
    );
}

#[test]
fn does_not_delay_write_before_consume() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["a"]));
    let stage = b.add_stage("GATHER", reg.create("GATHER", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] GATHER "))
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
        vec!["W", "R"],
        "trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn secondary_output_rejected() {
    let err = err_of("(ENDCHAR ?) literal a | g: gather | hole ? literal b | g: | hole");
    assert!(
        err.to_ascii_uppercase().contains("PRIMARY") || err.contains("output"),
        "{err}"
    );
}

#[test]
fn invented_operands_rejected() {
    let err = err_of("literal a | gather key 1 | hole");
    assert!(err.contains("unrecognized"), "{err}");
    let err = err_of("literal a | gathe | hole");
    assert!(err.contains("unknown stage"), "{err}");
}

#[test]
fn no_verb_prefix() {
    assert!(StageRegistry::default().contains("GATHER"));
    assert!(!StageRegistry::default().contains("GATHE"));
}

#[test]
fn named_stream_id_rejected() {
    let err = err_of("literal named | gather streamid 1-5 | hole");
    assert!(
        err.contains("not implemented") || err.contains("stream identifier"),
        "{err}"
    );
}
