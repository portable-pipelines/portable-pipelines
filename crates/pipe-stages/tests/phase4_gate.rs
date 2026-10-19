//! GATE — Author's Edition 1.1.11 pp.365–366
//! (examples p.366; NOEOFBACK p.441; PIPESTOP p.458; STARMSG p.518).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
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

fn two_stream_gate(
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
    let gate = b.add_stage(
        "GATE",
        reg.create("GATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("GATE"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, gate, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, gate, StreamId::SECONDARY);
    b.connect(gate, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(gate, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    (primary, secondary)
}

fn three_stream_gate(
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
    let gate = b.add_stage(
        "GATE",
        reg.create("GATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("GATE"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("sink_t", sink(Rc::clone(&t)));
    b.connect(src_p, StreamId::PRIMARY, gate, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, gate, StreamId::SECONDARY);
    b.connect(src_t, StreamId::PRIMARY, gate, StreamId(2));
    b.connect(gate, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(gate, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.connect(gate, StreamId(2), sink_t, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    let tertiary = t.borrow().clone();
    (primary, secondary, tertiary)
}

#[test]
fn classic_locate_stops_the_gate() {
    // p.366 totarget-style: records that do not contain the string pass
    // through the secondary until locate writes the trigger to primary.
    // GATE then terminates; later records are not produced.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b abc c | split | L: locate /abc/ | G: gate | hole ? L: | G: | >>COLLECT"
        ),
        vec!["a", "b"]
    );
}

#[test]
fn classic_locate_strict() {
    // STRICT must not pass the trigger or records after it. How many of
    // a/b appear depends on whether LOCATE filled both output slots (p.365).
    let got = run_spec_collect(
        "(ENDCHAR ?) literal a b abc c | split | L: locate /abc/ | G: gate strict | hole ? L: | G: | >>COLLECT",
    );
    assert!(
        got.iter().all(|r| r == "a" || r == "b"),
        "STRICT secondary {got:?}"
    );
    assert!(!got.iter().any(|r| r == "abc" || r == "c"), "{got:?}");
}

#[test]
fn trigger_is_shorted_to_primary_output() {
    // The matching record goes to GATE's primary output (short), not secondary.
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
    let spec = "(ENDCHAR ?) literal a abc b | split | L: locate /abc/ | G: gate | >>COLLECT_P ? L: | G: | >>COLLECT_S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    assert_eq!(*collected_p.borrow(), vec!["abc".to_string()]);
    assert_eq!(*collected_s.borrow(), vec!["a".to_string()]);
}

#[test]
fn no_primary_record_copies_higher_until_eof() {
    // p.365: if no record arrives on primary, terminate when all inputs are EOF.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b c | split | L: locate /zzz/ | G: gate | hole ? L: | G: | >>COLLECT"
        ),
        vec!["a", "b", "c"]
    );
}

#[test]
fn null_record_on_secondary_is_ordinary() {
    let (p, s) = two_stream_gate(&[], &[""], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert_eq!(s, vec![""]);
}

#[test]
fn null_record_on_primary_stops_the_gate() {
    // A null primary record is still a record (p.365: "a record arrives").
    let (p, s) = two_stream_gate(&[""], &["a", "b"], &[]);
    assert_eq!(p, vec![""]);
    assert!(
        s.len() <= 1,
        "null primary must stop the gate; secondary got {s:?}"
    );
}

#[test]
fn primary_only_shorts_the_file() {
    // Primary in/out only: the first record primes the short, then the rest
    // of the primary file is copied (splice stand-in).
    assert_eq!(
        run_spec_collect("literal a|literal b| gate | >>COLLECT"),
        vec!["b", "a"]
    );
}

#[test]
fn primary_only_empty_file() {
    // Primary EOF without a record: terminate without copying (p.365).
    assert_eq!(
        run_spec_collect("literal a | drop | gate | >>COLLECT"),
        Vec::<String>::new()
    );
}

#[test]
fn premature_termination_without_any_output() {
    // p.365: terminates when no output stream is connected.
    run_spec("literal a | gate").unwrap();
}

#[test]
fn ignores_unconnected_primary_output() {
    // Streams Used: ignore EOF on primary output; still pass higher streams.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b STOP | split | L: locate /STOP/ | G: gate ? L: | G: | >>COLLECT"
        ),
        vec!["a", "b"]
    );
}

#[test]
fn tertiary_pair_sees_eof_when_primary_stops() {
    // p.365: secondary and higher-numbered streams; p.366 UDP/STARMSG example
    // uses two higher pairs. A record on primary stops every pair.
    let (p, s, t) = three_stream_gate(&["STOP"], &["a", "b", "c"], &["x", "y"], &["STRICT"]);
    assert_eq!(p, vec!["STOP"]);
    // STRICT: do not consume a higher record when primary is also ready.
    assert!(s.is_empty(), "secondary after stop: {s:?}");
    assert!(t.is_empty(), "tertiary after stop: {t:?}");
}

#[test]
fn tertiary_copies_until_eof_without_primary_record() {
    let (p, s, t) = three_stream_gate(&[], &["a", "b"], &["x"], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert_eq!(s, vec!["a", "b"]);
    assert_eq!(t, vec!["x"]);
}

#[test]
fn strict_stops_without_consuming_higher_when_primary_ready() {
    let (p, s) = two_stream_gate(&["STOP"], &["a", "b"], &["STRICT"]);
    assert_eq!(p, vec!["STOP"]);
    assert!(
        s.is_empty(),
        "STRICT must not pass a higher record while primary is ready; got {s:?}"
    );
}

#[test]
fn without_strict_primary_ready_still_stops() {
    // Without STRICT the dispatcher strategy is unspecified (p.365). We still
    // look at primary first after ANYINPUT, so a ready primary record stops
    // the gate.
    let (p, s) = two_stream_gate(&["STOP"], &["a", "b"], &[]);
    assert_eq!(p, vec!["STOP"]);
    assert!(
        s.len() <= 1,
        "a ready primary record must stop the gate; secondary got {s:?}"
    );
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("GATE"));
    assert!(!reg.contains("GAT"));
    assert!(!reg.contains("GA"));
    let err = run_spec("literal a | gat | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_ascii_uppercase().contains("UNKNOWN STAGE")
            && msg.to_ascii_uppercase().contains("GAT"),
        "{err}"
    );
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | gate forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn invented_operand_after_strict_rejected() {
    let err = run_spec("literal a | gate strict anyeof | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn strict_is_not_abbreviable() {
    // Diagram (p.365) shows STRICT in all caps; STRIC is not an operand.
    let err = run_spec("literal a | gate stric | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn extra_operand_rejected() {
    let err = run_spec("literal a | gate 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn stop_keyword_rejected() {
    let err = run_spec("literal a | gate stop | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn higher_eof_propagates_while_waiting_for_primary() {
    // Secondary EOF while primary is still waiting: that pair sees EOF;
    // GATE keeps waiting, then shorts when STOP arrives.
    let (p, s) = two_stream_gate(&["STOP"], &["a"], &[]);
    assert_eq!(p, vec!["STOP"]);
    assert!(s.len() <= 1, "secondary {s:?}");
}
