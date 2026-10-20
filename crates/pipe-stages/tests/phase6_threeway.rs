//! THREEWAY — Author's Edition 1.1.11 p.557.

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

fn run_three(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let recs: Vec<Vec<u8>> = records.iter().map(|s| s.as_bytes().to_vec()).collect();
    run_three_bytes(&recs, args)
}

fn run_three_bytes(records: &[Vec<u8>], args: &[&str]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new({
            let records = records.to_vec();
            move |ctx: &mut StageCtx| {
                for rec in records {
                    if ctx.write(Record::from(rec)).is_err() {
                        break;
                    }
                }
            }
        }),
    );
    let stage = b.add_stage(
        "THREEWAY",
        reg.create("THREEWAY", args.iter().map(|a| (*a).to_string()).collect())
            .expect("THREEWAY"),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("t", sink(Rc::clone(&t)));
    b.connect_primary(src, stage);
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

#[test]
fn columns_split_three_ways() {
    let (p, s, t) = run_three(&["abcdef"], &["2-4"]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec!["bcd"]);
    assert_eq!(t, vec!["ef"]);
}

#[test]
fn single_column() {
    let (p, s, t) = run_three(&["abc"], &["1"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec!["a"]);
    assert_eq!(t, vec!["bc"]);
}

#[test]
fn whole_record_range() {
    let (p, s, t) = run_three(&["abc"], &["1-*"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec!["abc"]);
    assert_eq!(t, vec![""]);
}

#[test]
fn length_form() {
    let (p, s, t) = run_three(&["abcdef"], &["2.3"]);
    assert_eq!(p, vec!["a"]);
    assert_eq!(s, vec!["bcd"]);
    assert_eq!(t, vec!["ef"]);
}

#[test]
fn negative_from_end() {
    let (p, s, t) = run_three(&["abcdef"], &["-2;-1"]);
    assert_eq!(p, vec!["abcd"]);
    assert_eq!(s, vec!["ef"]);
    assert_eq!(t, vec![""]);
}

#[test]
fn word_range() {
    let (p, s, t) = run_three(&["one two three four"], &["word", "2"]);
    assert_eq!(p, vec!["one "]);
    assert_eq!(s, vec!["two"]);
    assert_eq!(t, vec![" three four"]);
}

#[test]
fn word_abbrev_w1() {
    let (p, s, t) = run_three(&["one two three"], &["w2-3"]);
    assert_eq!(p, vec!["one "]);
    assert_eq!(s, vec!["two three"]);
    assert_eq!(t, vec![""]);
}

#[test]
fn field_range() {
    let (p, s, t) = run_three(&["a\tb\tc"], &["field", "2"]);
    assert_eq!(p, vec!["a\t"]);
    assert_eq!(s, vec!["b"]);
    assert_eq!(t, vec!["\tc"]);
}

#[test]
fn substring_of_word() {
    let (p, s, t) = run_three(
        &["What a rum fellow."],
        &["substring", "2", "of", "word", "3"],
    );
    assert_eq!(p, vec!["What a r"]);
    assert_eq!(s, vec!["u"]);
    assert_eq!(t, vec!["m fellow."]);
}

#[test]
fn missing_range_writes_nulls() {
    // Starting column not in the record: whole record on primary, nulls elsewhere.
    let (p, s, t) = run_three(&["ab"], &["10"]);
    assert_eq!(p, vec!["ab"]);
    assert_eq!(s, vec![""]);
    assert_eq!(t, vec![""]);
}

#[test]
fn null_input_writes_three_nulls() {
    let (p, s, t) = run_three(&[""], &["1"]);
    assert_eq!(p, vec![""]);
    assert_eq!(s, vec![""]);
    assert_eq!(t, vec![""]);
}

#[test]
fn synonym_3way() {
    assert_eq!(
        run_collect(
            "(ENDCHAR ?) literal abcdef| t: 3way 2-4 | f: fanin | >>COLLECT ? t: | f: ? t: | f:"
        ),
        vec!["a", "bcd", "ef"]
    );
}

#[test]
fn spec_fanin_of_three_outputs() {
    assert_eq!(
        run_collect("(ENDCHAR ?) literal abcdef| t: threeway 2-4 | f: fanin | >>COLLECT ? t: | f: ? t: | f:"),
        vec!["a", "bcd", "ef"]
    );
}

#[test]
fn strictly_does_not_delay() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["abcdef"]));
    let stage = b.add_stage(
        "THREEWAY",
        reg.create("THREEWAY", vec!["2-4".into()]).unwrap(),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("t", sink(Rc::clone(&t)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.connect(stage, StreamId::TERTIARY, sink_t, StreamId::PRIMARY);
    b.run().unwrap();

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] THREEWAY "))
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
        vec!["W", "W", "W", "R"],
        "write three parts then consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

fn once() -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx: &mut StageCtx| {
        let _ = ctx.read();
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
    })
}

#[test]
fn eof_on_all_outputs_stops_without_consuming() {
    let trace = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["abcdef", "ghijkl"]));
    let stage = b.add_stage(
        "THREEWAY",
        reg.create("THREEWAY", vec!["2-4".into()]).unwrap(),
    );
    let once_p = b.add_stage("op", once());
    let once_s = b.add_stage("os", once());
    let once_t = b.add_stage("ot", once());
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, once_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, once_s, StreamId::PRIMARY);
    b.connect(stage, StreamId::TERTIARY, once_t, StreamId::PRIMARY);
    b.run().unwrap();
    let reads = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] THREEWAY ") && l.contains(" READ ") && l.contains(" len="))
        .count();
    assert_eq!(
        reads,
        1,
        "second record must remain unconsumed; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn range_required() {
    let err = run_spec("literal a | threeway | hole").unwrap_err();
    assert!(err.to_string().contains("inputRange is required"), "{err}");
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | threeway pad 2 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn extra_after_range_rejected() {
    let err = run_spec("literal a | threeway 1-2 extra | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected operand"), "{err}");
}

#[test]
fn three_streams_must_be_defined() {
    let err = run_spec("literal abc | threeway 1 | hole").unwrap_err();
    assert!(
        err.to_string().contains("three streams must be defined"),
        "{err}"
    );
    let err = run_spec("(ENDCHAR ?) literal abc | t: threeway 1 | hole ? t: | hole").unwrap_err();
    assert!(
        err.to_string().contains("three streams must be defined"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let err = run_spec(
        "(ENDCHAR ?) literal a | t: threeway 1 | hole ? literal b | t: | hole ? t: | hole",
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("no input stream other than the primary"),
        "{err}"
    );
}

#[test]
fn verbs_are_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("THREEWAY"));
    assert!(reg.contains("3WAY"));
    assert!(!reg.contains("THREEWA"));
    assert!(!reg.contains("3WA"));
    let err = run_spec("literal a | threewa 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn fs_wordsep_range() {
    let (p, s, t) = run_three(&["a:b:c"], &["fs", ":", "f2"]);
    assert_eq!(p, vec!["a:"]);
    assert_eq!(s, vec!["b"]);
    assert_eq!(t, vec![":c"]);
}
