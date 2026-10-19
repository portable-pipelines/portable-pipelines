//! SYNCHRONISE — Author's Edition 1.1.11 pp.536–539
//! (examples pp.537–538; tutorial p.84; dispatcher delay p.219).

use std::cell::{Cell, RefCell};
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

fn two_stream(primary: &[&str], secondary: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(primary));
    let src_s = b.add_stage("src_s", emit(secondary));
    let sync = b.add_stage(
        "SYNCHRONISE",
        reg.create(
            "SYNCHRONISE",
            args.iter().map(|a| (*a).to_string()).collect(),
        )
        .expect("SYNCHRONISE"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, sync, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, sync, StreamId::SECONDARY);
    b.connect(sync, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(sync, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    (primary, secondary)
}

fn three_stream(
    primary: &[&str],
    secondary: &[&str],
    tertiary: &[&str],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(primary));
    let src_s = b.add_stage("src_s", emit(secondary));
    let src_t = b.add_stage("src_t", emit(tertiary));
    let sync = b.add_stage(
        "SYNCHRONISE",
        reg.create("SYNCHRONISE", vec![]).expect("SYNCHRONISE"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("sink_t", sink(Rc::clone(&t)));
    b.connect(src_p, StreamId::PRIMARY, sync, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, sync, StreamId::SECONDARY);
    b.connect(src_t, StreamId::PRIMARY, sync, StreamId(2));
    b.connect(sync, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(sync, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.connect(sync, StreamId(2), sink_t, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    let tertiary = t.borrow().clone();
    (primary, secondary, tertiary)
}

#[test]
fn primary_only_copies_without_delay_order() {
    // One pair: peek, write, consume. LITERAL cascade is reverse order.
    assert_eq!(
        run_spec_collect("literal a|literal b| synchronise | >>COLLECT"),
        vec!["b", "a"]
    );
}

#[test]
fn primary_only_empty_file() {
    assert_eq!(
        run_spec_collect("literal a | drop | synchronise | >>COLLECT"),
        Vec::<String>::new()
    );
}

#[test]
fn lockstep_two_streams_equal_length() {
    // p.536: wait until every input has a record, then copy one from each.
    let (p, s) = two_stream(&["a", "b", "c"], &["x", "y", "z"], &[]);
    assert_eq!(p, vec!["a", "b", "c"]);
    assert_eq!(s, vec!["x", "y", "z"]);
}

#[test]
fn secondary_eof_stops_without_copying_leftover_primary() {
    // p.537: EOF on any input terminates; the peeked lower-stream record
    // is not consumed for a further cycle. Only complete pairs are written.
    let (p, s) = two_stream(&["a", "b", "c"], &["x"], &[]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn primary_eof_stops_without_copying_leftover_secondary() {
    // Peek is primary-first, so primary EOF ends the cycle before secondary
    // of that cycle is written. The previous complete pair was already copied.
    let (p, s) = two_stream(&["a"], &["x", "y", "z"], &[]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn both_empty() {
    let (p, s) = two_stream(&[], &[], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn null_records_are_ordinary() {
    // A null record is still a record (p.536: "a record available").
    let (p, s) = two_stream(&[""], &[""], &[]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec![""]);
}

#[test]
fn three_stream_lockstep() {
    let (p, s, t) = three_stream(&["a", "b"], &["x", "y"], &["1", "2"]);
    assert_eq!(p, vec!["a", "b"]);
    assert_eq!(s, vec!["x", "y"]);
    assert_eq!(t, vec!["1", "2"]);
}

#[test]
fn three_stream_shortest_stops() {
    // Tertiary EOF after one pair: only one record from each stream.
    let (p, s, t) = three_stream(&["a", "b", "c"], &["x", "y"], &["1"]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec!["x"]);
    assert_eq!(t, vec!["1"]);
}

#[test]
fn american_spelling_synonym() {
    // p.536 railroad: SYNChronize.
    assert_eq!(
        run_spec_collect("literal a|literal b| synchronize | >>COLLECT"),
        vec!["b", "a"]
    );
}

#[test]
fn abbrev_sync_min_4() {
    // SYNChronise / SYNChronize: SYNC is the uppercase prefix.
    let reg = StageRegistry::default();
    assert!(reg.contains("SYNC"));
    assert!(reg.contains("SYNCH"));
    assert!(reg.contains("SYNCHRONISE"));
    assert!(reg.contains("SYNCHRONIZE"));
    assert!(reg.contains("SYNCHRONIS"));
    assert!(reg.contains("SYNCHRONIZ"));
    assert!(!reg.contains("SYN"));
    assert!(!reg.contains("SY"));
    assert_eq!(run_spec_collect("literal a| sync | >>COLLECT"), vec!["a"]);
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | synchronise forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn extra_operand_rejected() {
    let err = run_spec("literal a | synchronise 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn strict_keyword_rejected() {
    // GATE’s STRICT is not a SYNCHRONISE operand.
    let err = run_spec("literal a | synchronise strict | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn premature_termination_without_any_output() {
    // p.537: discovers the output is not connected during the write phase
    // after inputs have been peeked. Pipeline must complete.
    run_spec("literal a | synchronise").unwrap();
}

#[test]
fn secondary_output_unconnected_writes_primary_then_stops() {
    // pp.537–538 TFTPUDP: peek both, write primary, then discover the
    // secondary output is not connected and do not consume either input.
    // Observable: one primary record; leftover primary is not copied.
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(&["a", "b"]));
    let src_s = b.add_stage("src_s", emit(&["x"]));
    let sync = b.add_stage(
        "SYNCHRONISE",
        StageRegistry::default()
            .create("SYNCHRONISE", vec![])
            .unwrap(),
    );
    let sink_p = b.add_stage("sink_p", sink(out));
    b.connect(src_p, StreamId::PRIMARY, sync, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, sync, StreamId::SECONDARY);
    b.connect(sync, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*collected.borrow(), vec!["a".to_string()]);
}

#[test]
fn leftover_primary_is_not_consumed_when_secondary_eof() {
    // p.537: when EOF is met on an input, lower-numbered streams still have
    // a record available because they were peeked and not consumed.
    let accepted = Rc::new(Cell::new(0usize));
    let accepted_c = Rc::clone(&accepted);
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage(
        "src_p",
        Box::new(move |ctx| {
            for rec in ["a", "b", "c"] {
                if ctx.write(Record::from(rec)).is_ok() {
                    accepted_c.set(accepted_c.get() + 1);
                }
            }
        }),
    );
    let src_s = b.add_stage("src_s", emit(&["x"]));
    let sync = b.add_stage(
        "SYNCHRONISE",
        StageRegistry::default()
            .create("SYNCHRONISE", vec![])
            .unwrap(),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, sync, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, sync, StreamId::SECONDARY);
    b.connect(sync, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(sync, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a".to_string()]);
    assert_eq!(*s.borrow(), vec!["x".to_string()]);
    // First record consumed after a successful cycle; the second is peeked
    // (write Accepted) then left unconsumed when secondary EOF is met.
    assert!(
        accepted.get() >= 2,
        "primary producer should have placed the peeked leftover; accepted={}",
        accepted.get()
    );
}

#[test]
fn spec_lockstep_via_labels() {
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b c | split | S: synchronise | >>COLLECT ? literal x y z | split | S: | hole"
        ),
        vec!["a", "b", "c"]
    );
}

#[test]
fn secondary_via_labels() {
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
        "(ENDCHAR ?) literal a b | split | S: synchronise | >>COLLECT_P ? literal x y | split | S: | >>COLLECT_S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    assert_eq!(
        *collected_p.borrow(),
        vec!["a".to_string(), "b".to_string()]
    );
    assert_eq!(
        *collected_s.borrow(),
        vec!["x".to_string(), "y".to_string()]
    );
}
