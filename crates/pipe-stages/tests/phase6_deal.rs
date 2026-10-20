//! DEAL — Author's Edition 1.1.11 pp.313–316.

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

fn run_deal(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let (p, s, _) = run_deal_n(records, args, 2);
    (p, s)
}

fn run_deal_n(
    records: &[&str],
    args: &[&str],
    outputs: usize,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "DEAL",
        reg.create("DEAL", args.iter().map(|a| (*a).to_string()).collect())
            .expect("DEAL"),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    if outputs >= 2 {
        let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
        b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    }
    if outputs >= 3 {
        let sink_t = b.add_stage("t", sink(Rc::clone(&t)));
        b.connect(stage, StreamId::TERTIARY, sink_t, StreamId::PRIMARY);
    }
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    let tertiary = t.borrow().clone();
    (primary, secondary, tertiary)
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
fn round_robin_two_outputs() {
    let (p, s) = run_deal(&["a", "b", "c", "d"], &[]);
    assert_eq!(p, vec!["a", "c"]);
    assert_eq!(s, vec!["b", "d"]);
}

#[test]
fn round_robin_three_outputs() {
    let (p, s, t) = run_deal_n(&["a", "b", "c", "d", "e"], &[], 3);
    assert_eq!(p, vec!["a", "d"]);
    assert_eq!(s, vec!["b", "e"]);
    assert_eq!(t, vec!["c"]);
}

#[test]
fn stop_anyeof_stops_without_consuming() {
    let leftover = Rc::new(Cell::new(false));
    let flag = Rc::clone(&leftover);
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b", "c", "d"] {
                if ctx.write(Record::from(rec)).is_err() {
                    flag.set(true);
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        "DEAL",
        reg.create("DEAL", vec!["STOP".into(), "ANYEOF".into()])
            .unwrap(),
    );
    let take = b.add_stage("TAKE", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, take, StreamId::PRIMARY);
    b.connect_primary(take, sink_p);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a"]);
    assert_eq!(*s.borrow(), vec!["b"]);
    assert!(
        leftover.get(),
        "STOP ANYEOF must leave the next input unconsumed"
    );
}

#[test]
fn key_keeps_a_run_on_one_stream() {
    let (p, s) = run_deal(&["aa", "ab", "ba", "bb", "ca"], &["KEY", "1"]);
    assert_eq!(p, vec!["aa", "ab", "ca"]);
    assert_eq!(s, vec!["ba", "bb"]);
}

#[test]
fn key_strip_prefix() {
    let (p, s) = run_deal(&["Axx", "Ayy", "Bzz"], &["KEY", "1", "STRIP"]);
    assert_eq!(p, vec!["xx", "yy"]);
    assert_eq!(s, vec!["zz"]);
}

#[test]
fn stop_count_two() {
    let leftover = Rc::new(Cell::new(false));
    let flag = Rc::clone(&leftover);
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b", "c", "d", "e"] {
                if ctx.write(Record::from(rec)).is_err() {
                    flag.set(true);
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        "DEAL",
        reg.create("DEAL", vec!["STOP".into(), "2".into()]).unwrap(),
    );
    let take_p = b.add_stage("TAKEP", reg.create("TAKE", vec!["1".into()]).unwrap());
    let take_s = b.add_stage("TAKES", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("t", sink(Rc::clone(&t)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, take_p, StreamId::PRIMARY);
    b.connect_primary(take_p, sink_p);
    b.connect(stage, StreamId::SECONDARY, take_s, StreamId::PRIMARY);
    b.connect_primary(take_s, sink_s);
    b.connect(stage, StreamId::TERTIARY, sink_t, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["a"]);
    assert_eq!(*s.borrow(), vec!["b"]);
    assert_eq!(*t.borrow(), vec!["c"]);
    assert!(leftover.get(), "STOP 2 must leave later input unconsumed");
}

#[test]
fn streamid_routes_each_record() {
    let (p, s, t) = run_deal_n(&["0x", "2y", "1z", "0w"], &["STREAMID", "1"], 3);
    assert_eq!(p, vec!["0x", "0w"]);
    assert_eq!(s, vec!["1z"]);
    assert_eq!(t, vec!["2y"]);
}

#[test]
fn streamid_strip_prefix() {
    let (p, s) = run_deal(&["0abc", "1xyz"], &["STREAMID", "1", "STRIP"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["xyz"]);
}

#[test]
fn streamid_abbrev_min_7() {
    let (p, s) = run_deal(&["0a", "1b"], &["STREAMI", "1"]);
    assert_eq!(p, vec!["0a"]);
    assert_eq!(s, vec!["1b"]);
    let err = err_of("literal a | deal stream 1 | hole");
    assert!(
        err.contains("unrecognized") || err.contains("unknown"),
        "{err}"
    );
}

#[test]
fn secondary_pairs_stream_ids() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("data", emit(&["aa", "bb", "cc"]));
    let src_s = b.add_stage("ids", emit(&["0", "1", "0"]));
    let stage = b.add_stage(
        "DEAL",
        reg.create("DEAL", vec!["SECONDARY".into()]).unwrap(),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, sink_p);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["aa", "cc"]);
    assert_eq!(*s.borrow(), vec!["bb"]);
}

#[test]
fn secondary_release_consumes_id_first() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src_p = b.add_stage("data", emit(&["aa"]));
    let src_s = b.add_stage("ids", emit(&["0"]));
    let stage = b.add_stage(
        "DEAL",
        reg.create("DEAL", vec!["SECONDARY".into(), "RELEASE".into()])
            .unwrap(),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    assert_eq!(*p.borrow(), vec!["aa"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] DEAL "))
        .filter_map(|l| {
            if l.contains(" READ stream=1 ") && l.contains(" len=") {
                Some("R1")
            } else if l.contains(" READ stream=0 ") && l.contains(" len=") {
                Some("R0")
            } else if l.contains(" WRITE ") && l.contains(" len=") {
                Some("W")
            } else {
                None
            }
        })
        .collect();
    assert!(
        events.windows(2).any(|w| w == ["R1", "W"]) || events.first() == Some(&"R1"),
        "RELEASE consumes secondary before the primary write; events={events:?}\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn named_stream_id_rejected() {
    let err = err_of("(ENDCHAR ?) literal named | d: deal streamid 1-5 | hole ? d: | hole");
    assert!(
        err.contains("not implemented") || err.contains("stream identifier"),
        "{err}"
    );
}

#[test]
fn does_not_delay_write_before_consume() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["a"]));
    let stage = b.add_stage("DEAL", reg.create("DEAL", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["a"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] DEAL "))
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
fn spec_discard_every_second() {
    assert_eq!(
        run_collect(
            "(ENDCHAR ?) literal d|literal c|literal b|literal a| d: deal | >>COLLECT ? d: | hole"
        ),
        vec!["a", "c"]
    );
}

#[test]
fn invented_operands_rejected() {
    let err = err_of("literal a | deal pad | hole");
    assert!(err.contains("unrecognized"), "{err}");
    let err = err_of("literal a | dea | hole");
    assert!(err.contains("unknown stage"), "{err}");
}

#[test]
fn no_verb_prefix() {
    assert!(StageRegistry::default().contains("DEAL"));
    assert!(!StageRegistry::default().contains("DEA"));
}
