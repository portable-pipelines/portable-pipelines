//! DAM — Author's Edition 1.1.11 pp.310–311 (example p.311).

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

fn two_stream_dam(
    primary: &[&str],
    secondary: &[&str],
    args: &[&str],
) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(primary));
    let src_s = b.add_stage("src_s", emit(secondary));
    let dam = b.add_stage(
        "DAM",
        reg.create("DAM", args.iter().map(|a| (*a).to_string()).collect())
            .expect("DAM"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, dam, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, dam, StreamId::SECONDARY);
    b.connect(dam, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(dam, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    (primary, secondary)
}

fn three_stream_dam(
    primary: &[&str],
    secondary: &[&str],
    tertiary: &[&str],
    args: &[&str],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(primary));
    let src_s = b.add_stage("src_s", emit(secondary));
    let src_t = b.add_stage("src_t", emit(tertiary));
    let dam = b.add_stage(
        "DAM",
        reg.create("DAM", args.iter().map(|a| (*a).to_string()).collect())
            .expect("DAM"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("sink_t", sink(Rc::clone(&t)));
    b.connect(src_p, StreamId::PRIMARY, dam, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, dam, StreamId::SECONDARY);
    b.connect(src_t, StreamId::PRIMARY, dam, StreamId(2));
    b.connect(dam, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(dam, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.connect(dam, StreamId(2), sink_t, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    let tertiary = t.borrow().clone();
    (primary, secondary, tertiary)
}

/// Book example p.311: output only when the input contains a target string.
/// fanout + elastic holds the file; locate primes DAM; then all streams short.
fn book_target(spec_target: &str, records: &str) -> Vec<String> {
    run_spec_collect(&format!(
        "(ENDCHAR ?) literal {records} | split | o: fanout | locate /{spec_target}/ | d: dam | hole ? o: | elastic | d: | >>COLLECT"
    ))
}

#[test]
fn book_example_opens_when_target_found() {
    // p.311: first matching record opens the floodgate; elastic holds the file.
    assert_eq!(book_target("abc", "a b abc c"), vec!["a", "b", "abc", "c"]);
}

#[test]
fn book_example_empty_when_target_missing() {
    // p.310: primary EOF without a record — terminate without copying.
    assert_eq!(book_target("zzz", "a b c"), Vec::<String>::new());
}

#[test]
fn primary_record_shorts_all_pairs() {
    let (p, s) = two_stream_dam(&["STOP"], &["a", "b"], &[]);
    assert_eq!(p, vec!["STOP"]);
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn empty_primary_does_not_pass_secondary() {
    // Converse of GATE: no primary record means no records are passed (p.310).
    let (p, s) = two_stream_dam(&[], &["a", "b"], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert!(
        s.is_empty(),
        "empty primary must not copy secondary; got {s:?}"
    );
}

#[test]
fn null_record_on_primary_bursts() {
    // A null primary record is still a record (p.310: "the first record arrives").
    let (p, s) = two_stream_dam(&[""], &["a", "b"], &[]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn null_record_on_secondary_is_ordinary_after_burst() {
    let (p, s) = two_stream_dam(&["STOP"], &[""], &[]);
    assert_eq!(p, vec!["STOP"]);
    assert_eq!(s, vec![""]);
}

#[test]
fn null_secondary_without_primary_is_not_passed() {
    let (p, s) = two_stream_dam(&[], &[""], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert!(s.is_empty(), "null secondary must not leak; got {s:?}");
}

#[test]
fn primary_only_shorts_the_file() {
    assert_eq!(
        run_spec_collect("literal a|literal b| dam | >>COLLECT"),
        vec!["b", "a"]
    );
}

#[test]
fn primary_only_empty_file() {
    assert_eq!(
        run_spec_collect("literal a | drop | dam | >>COLLECT"),
        Vec::<String>::new()
    );
}

#[test]
fn premature_termination_without_any_output() {
    // p.310: terminates when no output stream is connected.
    run_spec("literal a | dam").unwrap();
}

#[test]
fn ignores_unconnected_primary_output() {
    // Streams Used: ignore EOF on primary output; still short higher streams.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b abc c | split | o: fanout | locate /abc/ | d: dam ? o: | elastic | d: | >>COLLECT"
        ),
        vec!["a", "b", "abc", "c"]
    );
}

#[test]
fn fanout_without_elastic_does_not_stall() {
    // p.310: IBM SHORT splices every pair at once. Copying pair 0 to EOF
    // before pair 1 deadlocks a FANOUT that writes primary then secondary
    // with no ELASTIC (blocked on secondary, never EOFs primary).
    let collected_p = Rc::new(RefCell::new(Vec::new()));
    let collected_s = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    {
        let p = Rc::clone(&collected_p);
        reg.register(">>COLLECT_P", move |_| {
            pipe_stages::collect_to(Rc::clone(&p))
        });
    }
    {
        let s = Rc::clone(&collected_s);
        reg.register(">>COLLECT_S", move |_| {
            pipe_stages::collect_to(Rc::clone(&s))
        });
    }
    let spec =
        "(ENDCHAR ?) literal a b c | split | f: fanout | d: dam | >>COLLECT_P ? f: | d: | >>COLLECT_S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    assert_eq!(*collected_p.borrow(), vec!["a", "b", "c"]);
    assert_eq!(*collected_s.borrow(), vec!["a", "b", "c"]);
}

#[test]
fn does_not_delay_write_before_consume() {
    // p.310: strictly does not delay — WRITE the peeked record, then READ
    // to consume, including the priming primary record. COPY would READ first.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["x", "y"]));
    let dam = b.add_stage("DAM", reg.create("DAM", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, dam);
    b.connect_primary(dam, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["x", "y"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] DAM "))
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
        "DAM must write before consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn tertiary_pairs_short_after_primary_record() {
    let (p, s, t) = three_stream_dam(&["STOP"], &["a", "b"], &["x", "y"], &[]);
    assert_eq!(p, vec!["STOP"]);
    assert_eq!(s, vec!["a", "b"]);
    assert_eq!(t, vec!["x", "y"]);
}

#[test]
fn tertiary_pairs_pass_nothing_without_primary_record() {
    let (p, s, t) = three_stream_dam(&[], &["a", "b"], &["x"], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert!(s.is_empty(), "secondary {s:?}");
    assert!(t.is_empty(), "tertiary {t:?}");
}

#[test]
fn higher_eof_propagates_while_waiting_for_primary() {
    // Secondary EOF while primary is still waiting: that pair sees EOF;
    // DAM keeps waiting, then shorts when STOP arrives.
    let (p, s) = two_stream_dam(&["STOP"], &[], &[]);
    assert_eq!(p, vec!["STOP"]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("DAM"));
    assert!(!reg.contains("DA"));
    assert!(!reg.contains("D"));
    let err = run_spec("literal a | da | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_ascii_uppercase().contains("UNKNOWN STAGE")
            && msg.to_ascii_uppercase().contains("DA"),
        "{err}"
    );
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | dam forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn gate_strict_rejected_on_dam() {
    // GATE has STRICT; DAM's diagram is a bare DAM (p.310).
    let err = run_spec("literal a | dam strict | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn extra_operand_rejected() {
    let err = run_spec("literal a | dam 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn stop_keyword_rejected() {
    let err = run_spec("literal a | dam stop | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn second_operand_rejected() {
    let err = run_spec("literal a | dam forever now | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}
