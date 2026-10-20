//! COLLATE — Author's Edition 1.1.11 pp.295–297.

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

fn run_collate(
    masters: &[&str],
    details: &[&str],
    args: &[&str],
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_m = b.add_stage("masters", emit(masters));
    let src_d = b.add_stage("details", emit(details));
    let stage = b.add_stage(
        "COLLATE",
        reg.create("COLLATE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("COLLATE"),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("t", sink(Rc::clone(&t)));
    b.connect(src_m, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_d, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.connect(stage, StreamId::TERTIARY, sink_t, StreamId::PRIMARY);
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
fn default_master_then_details() {
    let (p, s, t) = run_collate(&["apple", "carrot"], &["banana", "carrot"], &[]);
    assert_eq!(p, vec!["carrot", "carrot"]);
    assert_eq!(s, vec!["apple"]);
    assert_eq!(t, vec!["banana"]);
}

#[test]
fn master_only_discards_details() {
    let (p, s, t) = run_collate(&["a", "c"], &["b", "c", "c"], &["MASTER"]);
    assert_eq!(p, vec!["c"]);
    assert_eq!(s, vec!["a"]);
    assert_eq!(t, vec!["b"]);
}

#[test]
fn detail_only_discards_master() {
    let (p, s, t) = run_collate(&["a", "c"], &["b", "c", "c"], &["DETAIL"]);
    assert_eq!(p, vec!["c", "c"]);
    assert_eq!(s, vec!["a"]);
    assert_eq!(t, vec!["b"]);
}

#[test]
fn detail_master_writes_details_first() {
    let (p, s, t) = run_collate(&["k"], &["k", "k"], &["DETAIL", "MASTER"]);
    assert_eq!(p, vec!["k", "k", "k"]);
    assert!(s.is_empty());
    assert!(t.is_empty());
}

#[test]
fn key_range_and_anycase() {
    let (p, s, t) = run_collate(&["Ax", "Cz"], &["ay", "Cz"], &["ANYCASE", "1"]);
    assert_eq!(p, vec!["Ax", "ay", "Cz", "Cz"]);
    assert!(s.is_empty());
    assert!(t.is_empty());
}

#[test]
fn pad_extends_shorter_key() {
    let (p, s, t) = run_collate(&["ab"], &["ab "], &["PAD", "BLANK", "1.3"]);
    assert_eq!(p, vec!["ab", "ab "]);
    assert!(s.is_empty());
    assert!(t.is_empty());
}

#[test]
fn nopad_unequal_length_keys_differ() {
    let (p, s, t) = run_collate(&["ab"], &["ab "], &["NOPAD", "1.3"]);
    assert_eq!(p, Vec::<String>::new());
    assert_eq!(s, vec!["ab"]);
    assert_eq!(t, vec!["ab "]);
}

#[test]
fn stop_anyeof_leaves_other_unconsumed() {
    // Secondary is empty (EOF immediately). Remaining masters stay unconsumed.
    let consumed = Rc::new(RefCell::new(Vec::new()));
    let leftover = Rc::clone(&consumed);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src_m = b.add_stage(
        "masters",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b"] {
                leftover.borrow_mut().push(format!("w{rec}"));
                if ctx.write(Record::from(rec)).is_err() {
                    leftover.borrow_mut().push(format!("unconsumed-{rec}"));
                    break;
                }
            }
        }),
    );
    let src_d = b.add_stage("details", emit(&[]));
    let stage = b.add_stage(
        "COLLATE",
        reg.create("COLLATE", vec!["STOP".into(), "ANYEOF".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect(src_m, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_d, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert!(
        consumed
            .borrow()
            .iter()
            .any(|s| s.starts_with("unconsumed-")),
        "STOP ANYEOF must leave the other stream unconsumed: {:?}",
        consumed.borrow()
    );
}

#[test]
fn eof_without_stop_copies_unmatched() {
    let (p, s, t) = run_collate(&["a", "b"], &[], &[]);
    assert!(p.is_empty());
    assert_eq!(s, vec!["a", "b"]);
    assert!(t.is_empty());
}

#[test]
fn eof_without_stop_copies_unmatched_details() {
    let (p, s, t) = run_collate(&[], &["a", "b"], &[]);
    assert!(p.is_empty());
    assert!(s.is_empty());
    assert_eq!(t, vec!["a", "b"]);
}

#[test]
fn two_ranges_apply_per_stream() {
    let (p, s, t) = run_collate(&["ax", "cx"], &["xa", "xc"], &["1", "2"]);
    assert_eq!(p, vec!["ax", "xa", "cx", "xc"]);
    assert!(s.is_empty());
    assert!(t.is_empty());
}

#[test]
fn caseany_synonym() {
    let (p, s, t) = run_collate(&["Ax"], &["ax"], &["CASEANY"]);
    assert_eq!(p, vec!["Ax", "ax"]);
    assert!(s.is_empty());
    assert!(t.is_empty());
}

#[test]
fn spec_label_three_streams() {
    assert_eq!(
        run_collect(
            "(ENDCHAR ?) literal carrot|literal apple | c: collate | >>COLLECT \
             ? literal carrot|literal banana | c: | hole ? c: | hole"
        ),
        vec!["carrot", "carrot"]
    );
}

#[test]
fn does_not_delay_write_before_consume() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src_m = b.add_stage("masters", emit(&["k"]));
    let src_d = b.add_stage("details", emit(&["k"]));
    let stage = b.add_stage("COLLATE", reg.create("COLLATE", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(src_m, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_d, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["k", "k"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] COLLATE "))
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
    assert!(
        events.windows(2).any(|w| w == ["W", "R"]),
        "strictly no delay: write then consume; events={events:?}\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn tertiary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage("COLLATE", reg.create("COLLATE", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect_primary(src, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId(2));
    b.connect_primary(stage, snk);
    let err = b.run().unwrap_err().to_string();
    assert!(err.contains("tertiary"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    let err = err_of("literal a | collate strict | hole");
    assert!(
        err.contains("unrecognized operand") || err.contains("STRICT"),
        "{err}"
    );
    let err = err_of("literal a | collate stop | hole");
    assert!(err.contains("ANYEOF"), "{err}");
    let err = err_of("literal a | collat | hole");
    assert!(err.contains("unknown stage"), "{err}");
}

#[test]
fn no_verb_prefix() {
    assert!(StageRegistry::default().contains("COLLATE"));
    assert!(!StageRegistry::default().contains("COLLAT"));
}
