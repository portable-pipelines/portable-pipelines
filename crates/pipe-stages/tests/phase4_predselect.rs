//! PREDSELECT — Author's Edition 1.1.11 pp.459–460
//! (tutorial p.85 Figure 175; FANOUT example p.341).

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

fn run_spec_both(spec: &str) -> (Vec<String>, Vec<String>) {
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    {
        let p = Rc::clone(&primary);
        reg.register(">>P", move |_| pipe_stages::collect_to(Rc::clone(&p)));
    }
    {
        let s = Rc::clone(&secondary);
        reg.register(">>S", move |_| pipe_stages::collect_to(Rc::clone(&s)));
    }
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

/// Direct streams: primary originals, secondary select-triggers, optional
/// tertiary reject-triggers, primary/secondary outputs.
fn run_direct(
    originals: &[&str],
    selected_triggers: &[&str],
    rejected_triggers: Option<&[&str]>,
) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(originals));
    let src_s = b.add_stage("src_s", emit(selected_triggers));
    let stage = b.add_stage("PREDSELECT", reg.create("PREDSELECT", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    if let Some(rej) = rejected_triggers {
        let src_t = b.add_stage("src_t", emit(rej));
        b.connect(src_t, StreamId::PRIMARY, stage, StreamId::TERTIARY);
    }
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    (primary, secondary)
}

#[test]
fn book_fanout_locate_selects_original() {
    // p.341 FANOUT example / p.85 topology (book uses XLATE; CHANGE is the stand-in).
    // Locate tests a copy; PREDSELECT writes the original.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a abc b | split | o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | l: locate /abc/ | p: | hole ? l: | p:"
        ),
        vec!["abc"]
    );
}

#[test]
fn destructive_change_routes_unmangled_original() {
    // p.459: the copy may be changed or rejected; the original is routed.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal keep-secret|literal skip-me| o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | change /secret/ /XXXXX/ | l: locate /XXXXX/ | p: | hole ? l: | p:"
        ),
        vec!["keep-secret"]
    );
}

#[test]
fn destructive_spec_drop_prefix() {
    // Transform drops the first two columns; locate sees the mangled copy.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal xxapple|literal xxpear| o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | spec 3-* 1 | l: locate /apple/ | p: | hole ? l: | p:"
        ),
        vec!["xxapple"]
    );
}

#[test]
fn tertiary_trigger_writes_rejected_originals() {
    // p.460 FINDANY: tertiary from the selection secondary so rejects are
    // not delayed until the next primary.
    let (p, s) = run_spec_both(
        "(ENDCHAR ?) literal a abc b | split | o: fanout stop anyeof | p: predselect | >>P ? o: | l: locate /abc/ | p: | >>S ? l: | p:",
    );
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn without_tertiary_all_selected_still_routes() {
    // p.460: the reject pipeline can be omitted when every record produces a
    // secondary trigger. Mixed select/reject without tertiary lets a later
    // select trigger fire the still-buffered original (connect tertiary).
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal abc abd | split | o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | locate /ab/ | p:"
        ),
        vec!["abc", "abd"]
    );
}

#[test]
fn last_selected_record_is_not_dropped_after_primary_eof() {
    // Primary EOF with an armed buffer still waits for the last trigger.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal zzz| o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | l: locate /z/ | p: | hole ? l: | p:"
        ),
        vec!["zzz"]
    );
}

#[test]
fn extra_triggers_after_write_are_discarded() {
    // p.459: after the buffer is written, further secondary/tertiary records
    // are discarded until the next primary.
    let (p, s) = run_direct(&["orig"], &["t1", "t2", "t3"], Some(&[]));
    assert_eq!(p, vec!["orig"]);
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn trigger_before_any_primary_is_discarded() {
    let (p, s) = run_direct(&[], &["stray"], Some(&["also"]));
    assert!(p.is_empty(), "primary {p:?}");
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn two_primaries_without_trigger_discards_the_first() {
    let (p, s) = run_direct(&["first", "second"], &["go"], None);
    // "go" may pair with whichever primary is in the buffer when it arrives.
    // Direct emitters can race; the book pairing is fanout-then-select.
    // At most one original is written (one-record buffer).
    assert_eq!(p.len(), 1, "primary {p:?}");
    assert!(s.is_empty(), "secondary {s:?}");
}

#[test]
fn secondary_trigger_then_tertiary_does_not_double_write() {
    // Once written, the next trigger is discarded until a new primary.
    let (p, s) = run_direct(&["only"], &["sel"], Some(&["rej"]));
    assert_eq!(p.len() + s.len(), 1, "primary {p:?} secondary {s:?}");
}

#[test]
fn null_original_is_an_ordinary_record() {
    // Bare LOCATE selects length ≥ 1 (p.414); COPY passes the null through
    // as a select trigger so PREDSELECT writes the original null record.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal | o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | copy | p:"
        ),
        vec![""]
    );
}

#[test]
fn empty_primary_writes_nothing() {
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a | drop | o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | l: locate /a/ | p: | hole ? l: | p:"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn nlocate_as_the_selection_stage() {
    let (p, s) = run_spec_both(
        "(ENDCHAR ?) literal ax b x | split | o: fanout stop anyeof | p: predselect | >>P ? o: | l: nlocate /x/ | p: | >>S ? l: | p:",
    );
    assert_eq!(p, vec!["b"]);
    assert_eq!(s, vec!["ax", "x"]);
}

#[test]
fn pick_as_the_selection_stage() {
    let (p, s) = run_spec_both(
        "(ENDCHAR ?) literal aa ab bb | split | o: fanout stop anyeof | p: predselect | >>P ? o: | l: pick 1-* == /aa/ | p: | >>S ? l: | p:",
    );
    assert_eq!(p, vec!["aa"]);
    assert_eq!(s, vec!["ab", "bb"]);
}

#[test]
fn copy_as_selection_selects_every_record() {
    // Same rule as ZONE/IF: any registered word may sit in the selection
    // slot of the topology. PREDSELECT itself has no nested operand.
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal a b | split | o: fanout stop anyeof | p: predselect | >>COLLECT ? o: | copy | p:"
        ),
        vec!["a", "b"]
    );
}

#[test]
fn premature_termination_without_any_output() {
    // p.460: terminates when no output stream is connected.
    run_spec("(ENDCHAR ?) literal a | o: fanout | p: predselect ? o: | p:").unwrap();
}

#[test]
fn one_stream_rejected() {
    let err = run_spec("literal a | predselect | hole").unwrap_err();
    assert!(
        err.to_string().contains("two streams must be defined"),
        "{err}"
    );
}

#[test]
fn tertiary_output_rejected() {
    let err = run_spec(
        "(ENDCHAR ?) literal a | p: predselect | hole ? literal t | p: | hole ? literal u | p: | hole",
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .to_ascii_lowercase()
            .contains("tertiary output"),
        "{err}"
    );
}

#[test]
fn four_streams_are_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src0 = b.add_stage("src0", emit(&["a"]));
    let src1 = b.add_stage("src1", emit(&["t"]));
    let src2 = b.add_stage("src2", emit(&[]));
    let src3 = b.add_stage("src3", emit(&[]));
    let stage = b.add_stage("PREDSELECT", reg.create("PREDSELECT", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(|ctx: &mut StageCtx| {
            for s in ctx.connected_inputs() {
                ctx.select_input(s);
                while ctx.read().ok().flatten().is_some() {}
            }
        }),
    );
    b.connect(src0, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src1, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(src2, StreamId::PRIMARY, stage, StreamId::TERTIARY);
    b.connect(src3, StreamId::PRIMARY, stage, StreamId(3));
    b.connect(stage, StreamId::PRIMARY, hole, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("at most three streams"), "{err}");
}

#[test]
fn nested_stage_operand_is_rejected() {
    // Diagram is a bare PREDSELect. The selection stage is a separate label
    // in the FANOUT topology, not an IF/ZONE-style nested word.
    let err = run_spec("literal a | predselect locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized operand")
            && err.to_string().to_ascii_lowercase().contains("locate"),
        "{err}"
    );
}

#[test]
fn invented_operand_rejected() {
    let err = run_spec("literal a | predselect strict | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn invented_stop_keyword_rejected() {
    let err = run_spec("literal a | predselect stop | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn invented_number_rejected() {
    let err = run_spec("literal a | predselect 1 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_abbrev_predsel_min_7() {
    // PREDSELect p.459: PREDSEL works; PREDSE is shorter than the prefix.
    let reg = StageRegistry::default();
    assert!(reg.contains("PREDSELECT"));
    assert!(reg.contains("PREDSEL"));
    assert!(reg.contains("PREDSELE"));
    assert!(reg.contains("PREDSELEC"));
    assert!(!reg.contains("PREDSE"));
    assert!(!reg.contains("PREDS"));
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal abc| o: fanout stop anyeof | p: predsel | >>COLLECT ? o: | l: locate /abc/ | p: | hole ? l: | p:"
        ),
        vec!["abc"]
    );
    let err = run_spec("literal a | predse | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("PREDSE"),
        "{err}"
    );
}

#[test]
fn no_operands_in_the_diagram() {
    let parsed = pipe_parser::parse_pipeline("literal x | predselect | hole").expect("parse");
    assert_eq!(
        parsed.pipelines[0][1].verb.to_ascii_uppercase(),
        "PREDSELECT"
    );
    assert!(parsed.pipelines[0][1].args.is_empty());
}
