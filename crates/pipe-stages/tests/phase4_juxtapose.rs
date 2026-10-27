//! JUXTAPOSE — Author's Edition 1.1.11 pp.403–405
//! (tutorial pp.83–84 Figure 173; collate example p.297; examples p.405).

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

fn two_stream(
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
    let stage = b.add_stage(
        "JUXTAPOSE",
        reg.create(
            "JUXTAPOSE",
            args.iter().map(|a| (*a).to_string()).collect(),
        )
        .expect("JUXTAPOSE"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    (primary, secondary)
}

fn count_field(n: u64, rest: &str) -> String {
    format!("{n:>10}{rest}")
}

#[test]
fn fanout_prefaces_each_copy() {
    // Common-source topology (p.84 / p.404 getfiles example): both streams
    // come from FANOUT, so the primary copy is stored before the secondary
    // copy of the same record is appended.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b | split | f: fanout | j: juxtapose | >>COLLECT ? f: | j:"
        ),
        vec!["aa", "bb"]
    );
}

#[test]
fn tutorial_user_id_prefix() {
    // Figure 173 / Figure 174 (pp.83–84): the primary is a 10-character user
    // ID field; the secondary resource is appended.
    let (p, s) = two_stream(&["JOHN      "], &["MAINT 190"], &[]);
    assert_eq!(p, vec!["JOHN      MAINT 190"]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn secondary_appended_to_last_primary() {
    let (p, s) = two_stream(&["PRE"], &["A", "B"], &[]);
    assert_eq!(p, vec!["PREA", "PREB"]);
    assert!(s.is_empty(), "used primary must not go to secondary {s:?}");
}

#[test]
fn unused_primary_goes_to_secondary_output() {
    // p.404: a primary that no secondary matched is written to secondary out
    // before the next primary is stored. COUNT does not prefix that record.
    let (p, s) = two_stream(&["keep", "drop"], &["X"], &[]);
    assert_eq!(p, vec!["keepX"]);
    assert_eq!(s, vec!["drop"]);
}

#[test]
fn unused_primary_count_does_not_prefix_secondary() {
    let (p, s) = two_stream(&["keep", "drop"], &["X"], &["COUNT"]);
    assert_eq!(p, vec![count_field(1, "keepX")]);
    assert_eq!(s, vec!["drop"]);
}

#[test]
fn all_primary_unused_when_secondary_empty() {
    // Secondary EOF with no records: every primary is unused (p.404).
    let (p, s) = two_stream(&["a", "b"], &[], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn secondary_without_primary_uses_empty_prefix() {
    // p.405: if the secondary produces first, the output is just that record
    // (empty buffer). Remaining secondaries keep appending to the empty buffer
    // until a primary arrives.
    let (p, s) = two_stream(&[], &["def", "ghi"], &[]);
    assert_eq!(p, vec!["def", "ghi"]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn primary_eof_keeps_last_buffer_for_remaining_secondary() {
    let (p, s) = two_stream(&["abc"], &["def", "ghi"], &[]);
    assert_eq!(p, vec!["abcdef", "abcghi"]);
    assert!(s.is_empty(), "used primary {s:?}");
}

#[test]
fn count_starts_at_one_per_buffer() {
    // p.404: COUNT prefixes the combined (primary) record; the count of
    // secondaries appended to the current buffer starts as one.
    let (p, s) = two_stream(&["P"], &["x", "y"], &["COUNT"]);
    assert_eq!(p, vec![count_field(1, "Px"), count_field(2, "Py")]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn count_resets_when_primary_replaced() {
    let (p, s) = two_stream(&["A", "B"], &["1", "2"], &["COUNT"]);
    // Prefer primary when both ready: store A, append 1 → 1A1; store B
    // (A was used); append 2 → 1B2.
    assert_eq!(p, vec![count_field(1, "A1"), count_field(1, "B2")]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn count_fanout_dup_two_appends() {
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal P| f: fanout | j: juxtapose count | >>COLLECT ? f: | dup | j:"
        ),
        vec![count_field(1, "PP"), count_field(2, "PP")]
    );
}

#[test]
fn null_primary_is_empty_prefix() {
    let (p, s) = two_stream(&[""], &["x"], &[]);
    assert_eq!(p, vec!["x"]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn null_secondary_appends_nothing() {
    let (p, s) = two_stream(&["ab"], &[""], &[]);
    assert_eq!(p, vec!["ab"]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn unused_null_primary_to_secondary() {
    let (p, s) = two_stream(&[""], &[], &[]);
    assert!(p.is_empty(), "primary {p:?}");
    assert_eq!(s, vec![""]);
}

#[test]
fn no_secondary_input_copies_unused_to_secondary_output() {
    // Two streams: primary in/out plus secondary out. Every primary is unused.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b | split | j: juxtapose | hole ? j: | >>COLLECT"
        ),
        vec!["a", "b"]
    );
}

#[test]
fn premature_termination_without_any_output() {
    // p.404: terminates when no output stream is connected.
    run_spec("(ENDCHAR ?) literal a | j: juxtapose ? literal b | j:").unwrap();
}

#[test]
fn one_stream_rejected() {
    let err = run_spec("literal a | juxtapose | hole").unwrap_err();
    assert!(
        err.to_string().to_ascii_lowercase().contains("two streams"),
        "{err}"
    );
}

#[test]
fn tertiary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(&["a"]));
    let src_s = b.add_stage("src_s", emit(&["b"]));
    let src_t = b.add_stage("src_t", emit(&["c"]));
    let stage = b.add_stage("JUXTAPOSE", reg.create("JUXTAPOSE", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(src_t, StreamId::PRIMARY, stage, StreamId(2));
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .to_ascii_lowercase()
            .contains("primary and secondary"),
        "{err}"
    );
}

#[test]
fn tertiary_output_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(&["a"]));
    let src_s = b.add_stage("src_s", emit(&["b"]));
    let stage = b.add_stage("JUXTAPOSE", reg.create("JUXTAPOSE", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::new(RefCell::new(Vec::new()))));
    let sink_t = b.add_stage("sink_t", sink(Rc::new(RefCell::new(Vec::new()))));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId(2), sink_t, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .to_ascii_lowercase()
            .contains("primary and secondary"),
        "{err}"
    );
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("JUXTAPOSE"));
    assert!(!reg.contains("JUXTAPOS"));
    assert!(!reg.contains("JUXT"));
    let err = run_spec("literal a | juxtap | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_ascii_uppercase().contains("UNKNOWN STAGE")
            && msg.to_ascii_uppercase().contains("JUXTAP"),
        "{err}"
    );
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | juxtapose strict | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn invented_operand_after_count_rejected() {
    let err = run_spec("literal a | juxtapose count pad | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn count_is_not_abbreviable() {
    // Diagram (p.403) shows COUNT in all caps; COUN is not an operand.
    let err = run_spec("literal a | juxtapose coun | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn extra_operand_rejected() {
    let err = run_spec("literal a | juxtapose 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn count_keyword_case_insensitive() {
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal P| f: fanout | j: juxtapose Count | >>COLLECT ? f: | j:"
        ),
        vec![count_field(1, "PP")]
    );
}

#[test]
fn both_eof_writes_nothing() {
    let (p, s) = two_stream(&[], &[], &[]);
    assert!(p.is_empty() && s.is_empty(), "primary {p:?} secondary {s:?}");
}

#[test]
fn secondary_output_unconnected_discards_unused() {
    // Used pair still appears on primary; unused drop is discarded.
    let (p, _) = two_stream(&["keep", "drop"], &["X"], &[]);
    assert_eq!(p, vec!["keepX"]);
}
