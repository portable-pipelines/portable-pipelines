//! NOEOFBACK — Author's Edition 1.1.11 pp.440–441
//! (GATE example p.441; synonym NOTEOFBACK p.441).

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

fn counting_emit(
    records: &[&str],
    produced: Rc<RefCell<Vec<String>>>,
) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<String> = records.iter().map(|s| (*s).to_string()).collect();
    Box::new(move |ctx| {
        for rec in records {
            if ctx.write(Record::from(rec.as_str())).is_err() {
                break;
            }
            produced.borrow_mut().push(rec);
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

fn take_n(n: usize, out: Rc<RefCell<Vec<String>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        for _ in 0..n {
            match ctx.read() {
                Ok(Some(r)) => out.borrow_mut().push(r.to_string_lossy()),
                _ => return,
            }
        }
        // Same as TAKE without a secondary: sever the producer so its next
        // write sees end-of-file (p.440). NOEOFBACK must keep consuming.
        ctx.short_input();
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
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

#[test]
fn passes_records_including_null() {
    // A null record is an ordinary record (p.440 does not special-case it).
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "", "b"]));
    let stage = b.add_stage("NOEOFBACK", reg.create("NOEOFBACK", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a", "", "b"]);
}

#[test]
fn copies_a_file() {
    assert_eq!(
        run_spec_collect("literal a b c | split | noeofback | >>COLLECT"),
        vec!["a", "b", "c"]
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert_eq!(
        run_spec_collect("literal a | drop | noeofback | >>COLLECT"),
        Vec::<String>::new()
    );
}

#[test]
fn synonym_noteofback() {
    // p.441 Note 1.
    assert!(StageRegistry::default().contains("NOTEOFBACK"));
    assert_eq!(
        run_spec_collect("literal hello| noteofback | >>COLLECT"),
        vec!["hello"]
    );
}

#[test]
fn book_callpipe_topology_omits_the_target() {
    // p.441 CALLPIPE fragment: first pipeline ends at GATE (primary output
    // unconnected). The match itself is not written downstream. Without
    // STRICT, GATE may pass one more secondary record after the trigger
    // is ready (p.365).
    let got = run_spec_collect(
        "(ENDCHAR ?) literal a b abc c | split | L: locate /abc/ | G: gate ? L: | G: | noeofback | >>COLLECT",
    );
    assert_eq!(got, vec!["a", "b", "c"]);
    assert!(got.iter().all(|r| r != "abc"), "got {got:?}");
}

#[test]
fn book_gate_example_with_take_still_reaches_the_target() {
    // p.441: without NOEOFBACK, EOF from TAKE would propagate backwards
    // through GATE and the subroutine could end before the target. With it,
    // TAKE gets the first non-match and NOEOFBACK drains the rest so GATE
    // stays open until locate writes the trigger.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b abc c | split | L: locate /abc/ | G: gate ? L: | G: | noeofback | take 1 | >>COLLECT"
        ),
        vec!["a"]
    );
}

#[test]
fn output_eof_consumes_remaining_input() {
    // p.440: when the output is at EOF, consume the remaining input (do not
    // propagate EOF backwards). TAKE 1 severs after the first record.
    let produced = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        counting_emit(&["a", "b", "c", "d"], Rc::clone(&produced)),
    );
    let stage = b.add_stage("NOEOFBACK", reg.create("NOEOFBACK", vec![]).unwrap());
    let taker = b.add_stage("take1", take_n(1, Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, taker);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a"]);
    assert_eq!(
        *produced.borrow(),
        vec!["a", "b", "c", "d"],
        "producer must not see EOF travelling backwards"
    );
}

#[test]
fn unconnected_output_consumes_remaining_input() {
    // p.440: an unconnected output is at end-of-file; consume remaining input.
    // Premature termination is not listed (unlike COPY p.305).
    let produced = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", counting_emit(&["a", "b"], Rc::clone(&produced)));
    let stage = b.add_stage("NOEOFBACK", reg.create("NOEOFBACK", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert_eq!(*produced.borrow(), vec!["a", "b"]);
}

#[test]
fn unconnected_output_spec_completes() {
    run_spec("literal a|literal b| noeofback").unwrap();
}

#[test]
fn eof_propagates_forwards() {
    // After the input file, the consumer sees end-of-file (no extra records).
    let out = Rc::new(RefCell::new(Vec::new()));
    let extra = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["only"]));
    let stage = b.add_stage("NOEOFBACK", reg.create("NOEOFBACK", vec![]).unwrap());
    let snk = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                extra.borrow_mut().push(r.to_string_lossy());
            }
            extra.borrow_mut().push("EOF".into());
        }),
    );
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["only", "EOF"]);
}

#[test]
fn does_not_delay_write_before_consume() {
    // p.440: strictly does not delay — WRITE the peeked record, then READ
    // to consume. COPY would READ first (p.305).
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["x", "y"]));
    let stage = b.add_stage("NOEOFBACK", reg.create("NOEOFBACK", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["x", "y"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] NOEOFBACK "))
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
        "NOEOFBACK must write before consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn secondary_output_is_unused() {
    // Streams Used is omitted (p.440). Primary in / primary out; a connected
    // secondary output is unused (not rejected).
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b"]));
    let stage = b.add_stage("NOEOFBACK", reg.create("NOEOFBACK", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a", "b"]);
    assert!(s.borrow().is_empty(), "secondary {s:?}", s = s.borrow());
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("NOEOFBACK"));
    assert!(!reg.contains("NOEOFBAC"));
    assert!(!reg.contains("NOEOF"));
    assert!(!reg.contains("NOE"));
    let err = run_spec("literal a | noeofbac | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_ascii_uppercase().contains("UNKNOWN STAGE")
            && msg.to_ascii_uppercase().contains("NOEOFBAC"),
        "{err}"
    );
}

#[test]
fn synonym_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("NOTEOFBACK"));
    assert!(!reg.contains("NOTEOFBAC"));
    assert!(!reg.contains("NOTEOF"));
    let err = run_spec("literal a | noteofbac | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_ascii_uppercase().contains("UNKNOWN STAGE")
            && msg.to_ascii_uppercase().contains("NOTEOFBAC"),
        "{err}"
    );
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | noeofback forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn extra_operand_rejected() {
    let err = run_spec("literal a | noeofback 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn strict_keyword_rejected() {
    let err = run_spec("literal a | noeofback strict | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn synonym_rejects_invented_operand() {
    let err = run_spec("literal a | noteofback stop | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}
