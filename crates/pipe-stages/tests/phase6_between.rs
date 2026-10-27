//! BETWEEN / INSIDE / NOTINSIDE / OUTSIDE.
//! Author's Edition 1.1.11 pp.272–273, 393–394, 442–443, 445–446.
//!
//! Portable ASCII: prefix compare is byte-for-byte (hex/binary dstrings are
//! raw bytes). The book's `xf1f2f3` sample is EBCDIC; tests use `x7374617274`
//! (`start`) and `x656e64` (`end`). The OUTSIDE heading example uses an ASCII
//! `1` prefix in place of ASA carriage control (`mctoasa` is out of scope).

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

fn run_spec_collect(spec: &str) -> Vec<String> {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let sink_out = Rc::clone(&sink_out);
        move |_| pipe_stages::collect_to(Rc::clone(&sink_out))
    });
    let full = format!("{spec} | >>COLLECT");
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

fn run_split(verb: &str, records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("{verb}")),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = primary.borrow().clone();
    let secondary = secondary.borrow().clone();
    (primary, secondary)
}

fn recs() -> &'static [&'static str] {
    &["a", "start", "b", "c", "end", "d"]
}

// --- BETWEEN (pp.272–273) ---

#[test]
fn between_book_script_tags() {
    // p.273: tags at the beginning of the record; match is "begins with".
    let (p, s) = run_split(
        "BETWEEN",
        &[":xmp.one", "body", ":exmp.", "after"],
        &["/:xmp./", "/:exmp./"],
    );
    assert_eq!(p, vec![":xmp.one", "body", ":exmp."]);
    assert_eq!(s, vec!["after"]);
}

#[test]
fn between_includes_both_labels() {
    let (p, s) = run_split("BETWEEN", recs(), &["/start/", "/end/"]);
    assert_eq!(p, vec!["start", "b", "c", "end"]);
    assert_eq!(s, vec!["a", "d"]);
}

#[test]
fn between_repeated_groups_same_string() {
    // p.273 Note 1: multiple groups. Start is not also the end of the same group.
    let (p, s) = run_split(
        "BETWEEN",
        &["a", "x", "1", "x", "b", "x", "2", "x", "c"],
        &["/x/", "/x/"],
    );
    assert_eq!(p, vec!["x", "1", "x", "x", "2", "x"]);
    assert_eq!(s, vec!["a", "b", "c"]);
}

#[test]
fn between_count_includes_start() {
    let (p, s) = run_split("BETWEEN", recs(), &["/start/", "3"]);
    assert_eq!(p, vec!["start", "b", "c"]);
    assert_eq!(s, vec!["a", "end", "d"]);
}

#[test]
fn between_open_group_runs_to_eof() {
    let (p, s) = run_split("BETWEEN", &["a", "start", "b", "c"], &["/start/", "/end/"]);
    assert_eq!(p, vec!["start", "b", "c"]);
    assert_eq!(s, vec!["a"]);
}

#[test]
fn between_prefix_not_contains() {
    let (p, s) = run_split(
        "BETWEEN",
        &["xxstart", "start here", "end"],
        &["/start/", "/end/"],
    );
    assert_eq!(p, vec!["start here", "end"]);
    assert_eq!(s, vec!["xxstart"]);
}

#[test]
fn between_anycase_synonyms() {
    for kw in ["ANY", "ANYCASE", "CASEANY", "IGNORECASE", "CASEIGNORE"] {
        let (p, _) = run_split("BETWEEN", &["START", "END"], &[kw, "/start/", "/end/"]);
        assert_eq!(p, vec!["START", "END"], "{kw}");
    }
    let err = err_of("literal a | between an /start/ /end/ | hole");
    assert!(
        err.contains("unrecognized") || err.contains("BETWEEN"),
        "{err}"
    );
}

#[test]
fn between_hex_and_binary_dstrings() {
    let (p, _) = run_split(
        "BETWEEN",
        &["start here", "mid", "end"],
        &["x7374617274", "x656e64"],
    );
    assert_eq!(p, vec!["start here", "mid", "end"]);
    let (p, _) = run_split("BETWEEN", &["abc", "zzz"], &["b01100001", "2"]);
    assert_eq!(p, vec!["abc", "zzz"]);
}

// --- INSIDE (p.393) ---

#[test]
fn inside_excludes_both_labels() {
    // p.394: identical strings, inside does not select the matching records.
    let (p, s) = run_split("INSIDE", recs(), &["/start/", "/end/"]);
    assert_eq!(p, vec!["b", "c"]);
    assert_eq!(s, vec!["a", "start", "end", "d"]);
}

#[test]
fn inside_book_script_example() {
    // p.394: process examples, discarding the begin and end tags.
    let (p, s) = run_split(
        "INSIDE",
        &[":xmp.", "body", ":exmp.", "after"],
        &["/:xmp./", "/:exmp./"],
    );
    assert_eq!(p, vec!["body"]);
    assert_eq!(s, vec![":xmp.", ":exmp.", "after"]);
}

#[test]
fn inside_same_string_excludes_labels() {
    let (p, s) = run_split(
        "INSIDE",
        &["a", "x", "1", "x", "b", "x", "2", "x", "c"],
        &["/x/", "/x/"],
    );
    assert_eq!(p, vec!["1", "2"]);
    assert_eq!(s, vec!["a", "x", "x", "b", "x", "x", "c"]);
}

#[test]
fn inside_count_zero_selects_nothing() {
    // p.393: number must be non-negative; 0 is an empty group.
    let (p, s) = run_split(
        "INSIDE",
        &["a", "start", "b", "start", "c"],
        &["/start/", "0"],
    );
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(s, vec!["a", "start", "b", "start", "c"]);
}

#[test]
fn inside_count_one() {
    let (p, s) = run_split(
        "INSIDE",
        &["a", "start", "b", "c", "start", "d"],
        &["/start/", "1"],
    );
    assert_eq!(p, vec!["b", "d"]);
    assert_eq!(s, vec!["a", "start", "c", "start"]);
}

#[test]
fn inside_count_extends_to_eof() {
    let (p, s) = run_split("INSIDE", &["a", "start", "b", "c"], &["/start/", "5"]);
    assert_eq!(p, vec!["b", "c"]);
    assert_eq!(s, vec!["a", "start"]);
}

#[test]
fn inside_open_group_runs_to_eof() {
    let (p, s) = run_split("INSIDE", &["a", "start", "b", "c"], &["/start/", "/end/"]);
    assert_eq!(p, vec!["b", "c"]);
    assert_eq!(s, vec!["a", "start"]);
}

#[test]
fn inside_end_label_is_not_the_next_start() {
    let (p, s) = run_split("INSIDE", &["x", "1", "x", "2"], &["/x/", "/x/"]);
    assert_eq!(p, vec!["1"]);
    assert_eq!(s, vec!["x", "x", "2"]);
}

#[test]
fn inside_anycase() {
    let (p, s) = run_split(
        "INSIDE",
        &["a", "START", "b", "END", "c"],
        &["ANYCASE", "/start/", "/end/"],
    );
    assert_eq!(p, vec!["b"]);
    assert_eq!(s, vec!["a", "START", "END", "c"]);
}

// --- NOTINSIDE (pp.442–443) ---

#[test]
fn notinside_is_converse_of_inside() {
    let recs = recs();
    let (ip, is) = run_split("INSIDE", recs, &["/start/", "/end/"]);
    let (np, ns) = run_split("NOTINSIDE", recs, &["/start/", "/end/"]);
    assert_eq!(np, is);
    assert_eq!(ns, ip);
}

#[test]
fn notinside_book_keeps_tags() {
    // p.443: remove lines inside example tags, retaining the tags.
    let (p, s) = run_split(
        "NOTINSIDE",
        &[":xmp.", "body", ":exmp.", "after"],
        &["/:xmp./", "/:exmp./"],
    );
    assert_eq!(p, vec![":xmp.", ":exmp.", "after"]);
    assert_eq!(s, vec!["body"]);
}

#[test]
fn ninside_synonym() {
    assert_eq!(
        run_spec_collect("literal a start b end d | split | ninside /start/ /end/"),
        vec!["a", "start", "end", "d"]
    );
}

#[test]
fn notinside_abbrev_min_7() {
    // NOTINSIDe p.442: NOTINSI works; NOTINS is shorter than the uppercase prefix.
    let reg = StageRegistry::default();
    assert!(reg.contains("NOTINSI"));
    assert!(reg.contains("NOTINSID"));
    assert!(reg.contains("NOTINSIDE"));
    assert!(!reg.contains("NOTINS"));
    assert_eq!(
        run_spec_collect("literal a start b end | split | notinsi /start/ /end/"),
        vec!["a", "start", "end"]
    );
    let err = err_of("literal a | notins /start/ /end/ | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("NOTINS"),
        "{err}"
    );
}

#[test]
fn notinside_count_zero() {
    let (p, s) = run_split("NOTINSIDE", &["a", "start", "b"], &["/start/", "0"]);
    assert_eq!(p, vec!["a", "start", "b"]);
    assert!(s.is_empty(), "{s:?}");
}

// --- OUTSIDE (pp.445–446) ---

#[test]
fn outside_is_converse_of_between() {
    let recs = recs();
    let (bp, bs) = run_split("BETWEEN", recs, &["/start/", "/end/"]);
    let (op, os) = run_split("OUTSIDE", recs, &["/start/", "/end/"]);
    assert_eq!(op, bs);
    assert_eq!(os, bp);
}

#[test]
fn outside_book_heading_count() {
    // p.446: `outside /1/ 2` drops two heading lines (ASA `1` → ASCII prefix).
    let (p, s) = run_split(
        "OUTSIDE",
        &[
            "1heading",
            "rest of heading",
            "body",
            "1next",
            "more heading",
            "body2",
        ],
        &["/1/", "2"],
    );
    assert_eq!(p, vec!["body", "body2"]);
    assert_eq!(
        s,
        vec!["1heading", "rest of heading", "1next", "more heading"]
    );
}

#[test]
fn outside_count_must_be_at_least_two() {
    let err = err_of("literal a | outside /x/ 1 | hole");
    assert!(err.contains("at least 2"), "{err}");
}

#[test]
fn outside_anycase() {
    let (p, s) = run_split(
        "OUTSIDE",
        &["a", "START", "b", "END", "c"],
        &["CASEIGNORE", "/start/", "/end/"],
    );
    assert_eq!(p, vec!["a", "c"]);
    assert_eq!(s, vec!["START", "b", "END"]);
}

#[test]
fn not_between_matches_outside() {
    // NOT wrapping BETWEEN inverts streams the same way OUTSIDE does.
    let recs = recs();
    let (op, os) = run_split("OUTSIDE", recs, &["/start/", "/end/"]);
    let (np, ns) = run_split("NOT", recs, &["between", "/start/", "/end/"]);
    assert_eq!(np, op);
    assert_eq!(ns, os);
}

#[test]
fn not_inside_matches_notinside() {
    let recs = recs();
    let (np, ns) = run_split("NOTINSIDE", recs, &["/start/", "/end/"]);
    let (wp, ws) = run_split("NOT", recs, &["inside", "/start/", "/end/"]);
    assert_eq!(wp, np);
    assert_eq!(ws, ns);
}

// --- Shared streams / delay / rejection ---

#[test]
fn all_caps_verbs_are_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("BETWEEN"));
    assert!(!reg.contains("BETWEE"));
    assert!(!reg.contains("BET"));
    assert!(reg.contains("INSIDE"));
    assert!(!reg.contains("INSID"));
    assert!(!reg.contains("INS"));
    assert!(reg.contains("OUTSIDE"));
    assert!(!reg.contains("OUTSID"));
    assert!(!reg.contains("OUT"));
    let err = err_of("literal a | betwee /a/ /b/ | hole");
    assert!(err.contains("unknown stage"), "{err}");
    let err = err_of("literal a | insid /a/ /b/ | hole");
    assert!(err.contains("unknown stage"), "{err}");
    let err = err_of("literal a | outsid /a/ 2 | hole");
    assert!(err.contains("unknown stage"), "{err}");
}

#[test]
fn undocumented_operands_are_rejected() {
    for spec in [
        "literal a | between 1-4 /x/ | hole",
        "literal a | inside range 1 /x/ /y/ | hole",
        "literal a | outside /x/ mask | hole",
        "literal a | notinside start end | hole",
        "literal a | between /x/ anycase /y/ | hole",
        "literal a | inside /x/ /y/ extra | hole",
        "literal a | between * /x/ | hole",
        "literal a | inside /x/ -1 | hole",
        "literal a | between /x/ +2 | hole",
        "literal a | inside FIRST /x/ /y/ | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized operand")
                || err.contains("unexpected operand")
                || err.contains("BETWEEN")
                || err.contains("INSIDE")
                || err.contains("OUTSIDE")
                || err.contains("NOTINSIDE"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn missing_operands() {
    for verb in ["between", "inside", "outside", "notinside"] {
        let err = err_of(&format!("literal a | {verb} | hole"));
        assert!(err.contains("requires two operands"), "{verb}: {err}");
        let err = err_of(&format!("literal a | {verb} /x/ | hole"));
        assert!(err.contains("requires two operands"), "{verb}: {err}");
    }
}

#[test]
fn first_operand_must_be_dstring() {
    let err = err_of("literal a | between start /end/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn anycase_after_dstring_is_rejected() {
    let err = err_of("literal a | inside /start/ anycase /end/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn secondary_unconnected_discards_the_other_stream() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "x", "b", "x", "c"]));
    let stage = b.add_stage(
        "INSIDE",
        reg.create("INSIDE", vec!["/x/".into(), "/x/".into()])
            .unwrap(),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["b".to_string()]);
}

#[test]
fn secondary_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage(
        "BETWEEN",
        reg.create("BETWEEN", vec!["/a/".into(), "2".into()])
            .unwrap(),
    );
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect_primary(src, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("secondary input"), "{err}");
}

#[test]
fn premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["one", "two", "three", "four"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "INSIDE",
        reg.create("INSIDE", vec!["/one/".into(), "/two/".into()])
            .unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "INSIDE should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn strictly_does_not_delay() {
    // p.272 / p.393 / p.442 / p.445: peek, write, then consume.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit(&["start", "end"]));
    let stage = b.add_stage(
        "BETWEEN",
        reg.create("BETWEEN", vec!["/start/".into(), "/end/".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["start", "end"]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] BETWEEN "))
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
        "BETWEEN must write before consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn null_delimited_string_matches_every_record() {
    // Empty dstring is a prefix of every record. BETWEEN // // groups pairs.
    let (p, s) = run_split("BETWEEN", &["a", "b", "c", "d"], &["//", "//"]);
    assert_eq!(p, vec!["a", "b", "c", "d"]);
    assert!(s.is_empty(), "{s:?}");
    let (p, s) = run_split("INSIDE", &["a", "b", "c"], &["//", "//"]);
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(s, vec!["a", "b", "c"]);
}
