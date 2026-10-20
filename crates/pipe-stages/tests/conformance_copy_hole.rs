//! COPY (p.305) and HOLE (p.382) — Author's Edition 1.1.11.

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

fn run_primary(verb: &str, records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
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

#[test]
fn copy_passes_records_including_null() {
    assert_eq!(
        run_primary("COPY", &["a", "", "b"], &[]),
        vec!["a", "", "b"]
    );
}

#[test]
fn copy_spec_pass_through() {
    assert_eq!(
        run_spec_collect("literal c|literal a|literal b| copy"),
        vec!["b", "a", "c"]
    );
}

#[test]
fn copy_rejects_invented_operands() {
    let err = run_spec("literal a | copy first | hole").unwrap_err();
    assert!(err.to_string().contains("no operands"), "{err}");
}

#[test]
fn copy_verb_is_exact() {
    // COPY p.305: diagram is all-caps; `cop` is unknown.
    let reg = StageRegistry::default();
    assert!(reg.contains("COPY"));
    assert!(!reg.contains("COP"));
    let err = run_spec("literal a | cop | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("COP"),
        "{err}"
    );
}

#[test]
fn copy_reads_before_write_one_record_delay() {
    // p.305: the input record is consumed before the buffer is written.
    // Contrast NOEOFBACK (p.440) which writes then consumes.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["x", "y"]));
    let stage = b.add_stage("COPY", reg.create("COPY", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["x", "y"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] COPY "))
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
        vec!["R", "W", "R", "W"],
        "COPY must consume before write; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn copy_one_record_delay_decouples_producer() {
    // p.305: delaying one record lets the producer write the next record
    // while COPY is waiting on its consumer. A strictly-no-delay stage
    // (peek, write, consume) would stall here.
    let produced = Rc::new(Cell::new(0u32));
    let pflag = Rc::clone(&produced);
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b", "c"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                pflag.set(pflag.get() + 1);
            }
        }),
    );
    let copy = b.add_stage("COPY", reg.create("COPY", vec![]).unwrap());
    let produced2 = Rc::clone(&produced);
    let o = Rc::clone(&out);
    let snk = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while produced2.get() < 2 {
                if ctx.yield_others().is_err() {
                    return;
                }
            }
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, copy);
    b.connect_primary(copy, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a", "b", "c"]);
}

#[test]
fn copy_premature_termination_without_output() {
    // p.305: terminates when the output stream is not connected.
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b", "c"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let copy = b.add_stage("COPY", reg.create("COPY", vec![]).unwrap());
    b.connect_primary(src, copy);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "COPY should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn copy_secondary_output_is_unused() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b"]));
    let stage = b.add_stage("COPY", reg.create("COPY", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a", "b"]);
    assert!(s.borrow().is_empty());
}

#[test]
fn copy_rejects_secondary_input() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src1 = b.add_stage("src1", emit(&["a"]));
    let src2 = b.add_stage("src2", emit(&["b"]));
    let copy = b.add_stage("COPY", reg.create("COPY", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect(src1, StreamId::PRIMARY, copy, StreamId::PRIMARY);
    b.connect(src2, StreamId::PRIMARY, copy, StreamId::SECONDARY);
    b.connect_primary(copy, hole);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .contains("no input stream other than the primary"),
        "{err}"
    );
}

#[test]
fn hole_discards_primary() {
    run_spec("literal a | hole").unwrap();
}

#[test]
fn hole_reads_every_connected_input() {
    // p.382: hole reads from all defined input streams.
    let reg = StageRegistry::default();
    let written = Rc::new(RefCell::new(0u32));
    let c1 = Rc::clone(&written);
    let c2 = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src1 = b.add_stage(
        "src1",
        Box::new(move |ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from("p"));
            *c1.borrow_mut() += 1;
        }),
    );
    let src2 = b.add_stage(
        "src2",
        Box::new(move |ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from("s"));
            *c2.borrow_mut() += 1;
        }),
    );
    let hole = b.add_stage("HOLE", reg.create("HOLE", vec![]).unwrap());
    b.connect(src1, StreamId::PRIMARY, hole, StreamId::PRIMARY);
    b.connect(src2, StreamId::PRIMARY, hole, StreamId::SECONDARY);
    b.run().unwrap();
    assert_eq!(*written.borrow(), 2);
}

#[test]
fn hole_rejects_invented_operands() {
    let err = run_spec("literal a | hole all | console").unwrap_err();
    assert!(err.to_string().contains("no operands"), "{err}");
}
