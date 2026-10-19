//! EOFBACK — Author's Edition 1.1.11 p.337
//! (device-driver example p.665 message 39I: `eofback > trace file1 a`).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId, TraceConfig};
use pipe_stages::{run_spec, StageRegistry};

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cms-pipelines-eofback-{name}"))
}

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

fn read_host_lines(path: &std::path::Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    if text.is_empty() {
        return Vec::new();
    }
    text.split_terminator('\n').map(|s| s.to_string()).collect()
}

#[test]
fn copies_through_including_null() {
    // A null record is an ordinary record (p.337 does not special-case it).
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "", "b"]));
    let stage = b.add_stage(
        "EOFBACK",
        reg.create("EOFBACK", vec!["HOLE".into()]).unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a", "", "b"]);
}

#[test]
fn copies_a_file() {
    assert_eq!(
        run_spec_collect("literal a b c | split | eofback hole | >>COLLECT"),
        vec!["a", "b", "c"]
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert_eq!(
        run_spec_collect("literal a | drop | eofback hole | >>COLLECT"),
        Vec::<String>::new()
    );
}

#[test]
fn book_console_example_with_take_as_partition() {
    // p.337: `... | eofback console | tolabel ...` — only records consumed
    // by the following (partitioning) stages reach the driver. TOLABEL is
    // not in this port; TAKE is the same class of stop-after-N consumer.
    assert_eq!(
        run_spec_collect("literal a b c | split | eofback hole | take 1 | >>COLLECT"),
        vec!["a"]
    );
}

#[test]
fn nested_output_driver_sees_only_consumed_records() {
    // p.665: `... | eofback > trace file1 a | ...` — the file stage runs as
    // the nested driver whose output is not connected.
    let path = tmp("driver-consumed.txt");
    let _ = std::fs::remove_file(&path);
    let spec = format!(
        "literal a b c | split | eofback > {} | take 1 | >>COLLECT",
        path.display()
    );
    assert_eq!(run_spec_collect(&spec), vec!["a"]);
    assert_eq!(
        read_host_lines(&path),
        vec!["a"],
        "nested > must see only the record TAKE consumed"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn without_eofback_file_writer_keeps_reading() {
    // Contrast: a device driver writes the record to the file *before* the
    // copy-through, so TAKE 1 still lets a later record into the file.
    let path = tmp("bare-file-take.txt");
    let _ = std::fs::remove_file(&path);
    let spec = format!(
        "literal a b c | split | > {} | take 1 | >>COLLECT",
        path.display()
    );
    assert_eq!(run_spec_collect(&spec), vec!["a"]);
    let got = read_host_lines(&path);
    assert!(
        got.len() > 1,
        "bare > should write past the consumed record; got {got:?}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn output_eof_propagates_backwards() {
    // Converse of NOEOFBACK p.440: when the output severs, do not consume
    // remaining input (short the producer).
    let produced = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        counting_emit(&["a", "b", "c", "d"], Rc::clone(&produced)),
    );
    let stage = b.add_stage(
        "EOFBACK",
        reg.create("EOFBACK", vec!["HOLE".into()]).unwrap(),
    );
    let taker = b.add_stage("take1", take_n(1, Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, taker);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a"]);
    assert_eq!(
        *produced.borrow(),
        vec!["a"],
        "producer must see EOF travelling backwards"
    );
}

#[test]
fn unconnected_output_is_premature_termination() {
    // p.337: terminates when the output stream is not connected (unlike
    // NOEOFBACK, which drains).
    let produced = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", counting_emit(&["a", "b"], Rc::clone(&produced)));
    let stage = b.add_stage(
        "EOFBACK",
        reg.create("EOFBACK", vec!["HOLE".into()]).unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        produced.borrow().is_empty(),
        "unconnected output must not consume input; produced {p:?}",
        p = produced.borrow()
    );
}

#[test]
fn unconnected_output_spec_completes() {
    run_spec("literal a|literal b| eofback hole").unwrap();
}

#[test]
fn eof_propagates_forwards() {
    let out = Rc::new(RefCell::new(Vec::new()));
    let extra = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["only"]));
    let stage = b.add_stage(
        "EOFBACK",
        reg.create("EOFBACK", vec!["HOLE".into()]).unwrap(),
    );
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
    // p.337: strictly does not delay — WRITE the peeked record, wait until
    // it is consumed, feed the driver, then READ.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["x", "y"]));
    let stage = b.add_stage(
        "EOFBACK",
        reg.create("EOFBACK", vec!["HOLE".into()]).unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["x", "y"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] EOFBACK "))
        .filter_map(|l| {
            if l.contains(" WRITE stream=0 ") && l.contains(" len=") {
                Some("W0")
            } else if l.contains(" WAIT output stream=0") {
                Some("Wait")
            } else if l.contains(" WRITE stream=1 ") && l.contains(" len=") {
                Some("W1")
            } else if l.contains(" READ stream=0 ") && l.contains(" len=") {
                Some("R")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        events,
        vec!["W0", "Wait", "W1", "R", "W0", "Wait", "W1", "R"],
        "EOFBACK must write, wait for consume, feed the driver, then consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn nested_driver_output_is_not_connected() {
    // p.337: the specified device driver's output stream is not connected.
    // COPY as the nested word would copy through if it had an output; with
    // a null-ended output it still only sees records EOFBACK feeds it.
    let path = tmp("copy-driver.txt");
    let _ = std::fs::remove_file(&path);
    let spec = format!(
        "literal a|literal b| eofback > {} | >>COLLECT",
        path.display()
    );
    assert_eq!(run_spec_collect(&spec), vec!["b", "a"]);
    assert_eq!(read_host_lines(&path), vec!["b", "a"]);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn secondary_streams_are_rejected() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let stage = b.add_stage(
        "EOFBACK",
        reg.create("EOFBACK", vec!["HOLE".into()]).unwrap(),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .to_ascii_uppercase()
            .contains("ONLY THE PRIMARY"),
        "{err}"
    );
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("EOFBACK"));
    assert!(!reg.contains("EOFBAC"));
    assert!(!reg.contains("EOF"));
    let err = run_spec("literal a | eofbac hole | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_ascii_uppercase().contains("UNKNOWN STAGE")
            && msg.to_ascii_uppercase().contains("EOFBAC"),
        "{err}"
    );
}

#[test]
fn missing_word_rejected() {
    let err = run_spec("literal a | eofback | hole").unwrap_err();
    assert!(
        err.to_string()
            .contains("a word (the name of the program to run) is required"),
        "{err}"
    );
}

#[test]
fn invented_word_rejected() {
    let err = run_spec("literal a | eofback forever | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn number_as_word_rejected() {
    let err = run_spec("literal a | eofback 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn unknown_nested_stage_rejected() {
    let err = run_spec("literal a | eofback nosuch | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn nested_stage_rejects_its_own_undocumented_operands() {
    let err = run_spec("literal a | eofback copy forever | hole").unwrap_err();
    assert!(err.to_string().contains("no operands are allowed"), "{err}");
}

#[test]
fn console_abbrev_is_a_valid_nested_word() {
    // Nested word uses the registry, so CONSole min 4 (p.303) is accepted.
    // Output unconnected: premature termination, nested CONSOLE never runs.
    run_spec("literal a| eofback cons").unwrap();
}
