//! CASEI — Author's Edition 1.1.11 pp.285–286.

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

fn run_both(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "CASEI",
        reg.create("CASEI", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing CASEI"),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

fn run_primary_only(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "CASEI",
        reg.create("CASEI", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing CASEI"),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
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

#[test]
fn casei_locate_book_style() {
    // User-requested: `casei locate /abc/`. Originals are written, not the
    // uppercase copies. Portable ASCII uppercase (A–Z).
    let recs = &["abc", "Abc", "ABC", "xyz", "abCdef"];
    let (p, s) = run_both(recs, &["locate", "/abc/"]);
    assert_eq!(p, vec!["abc", "Abc", "ABC", "abCdef"]);
    assert_eq!(s, vec!["xyz"]);
}

#[test]
fn casei_find_book_example() {
    // p.285: `casei find :figref` — GML tag at the beginning, ignoring case.
    // Portable ASCII; the book’s EBCDIC letters are the same 26.
    let recs = &[" :skip", ":FigRef rest", ":figref", "other", ":FIGREF"];
    let (p, s) = run_both(recs, &["find", ":figref"]);
    assert_eq!(p, vec![":FigRef rest", ":figref", ":FIGREF"]);
    assert_eq!(s, vec![" :skip", "other"]);
}

#[test]
fn casei_find_abc() {
    // User-requested: `casei find abc`.
    let recs = &["abcZZ", "Abc", "xabc", "ABC"];
    let (p, s) = run_both(recs, &["find", "abc"]);
    assert_eq!(p, vec!["abcZZ", "Abc", "ABC"]);
    assert_eq!(s, vec!["xabc"]);
}

#[test]
fn casei_end_to_end_locate() {
    let out = run_spec_collect("literal xyz|literal Abc|literal keep| casei locate /abc/");
    // literal cascade is reverse order.
    assert_eq!(out, vec!["Abc"]);
}

#[test]
fn casei_end_to_end_find() {
    let out = run_spec_collect("literal :FigRef id| casei find :figref");
    assert_eq!(out, vec![":FigRef id"]);
}

#[test]
fn casei_writes_the_original_record() {
    let recs = &["abCxxx"];
    let (p, s) = run_both(recs, &["locate", "/abc/"]);
    assert_eq!(p, vec!["abCxxx"]);
    assert!(s.is_empty());
}

#[test]
fn casei_zone_uppercases_only_the_range() {
    // p.285: ZONE — only that part is passed to the nested stage.
    // Columns 1–3 of `abCxxx` become `ABC`; `/xyz/` is in the unzoned tail.
    let recs = &["abCxxx", "xyZzzz", "abCxyz"];
    let (p, s) = run_both(recs, &["zone", "1-3", "locate", "/abc/"]);
    assert_eq!(p, vec!["abCxxx", "abCxyz"]);
    assert_eq!(s, vec!["xyZzzz"]);
}

#[test]
fn casei_zone_equivalent_to_zone_casei() {
    let recs = &["abCxxx", "zzzxxx"];
    let (p, s) = run_both(recs, &["zone", "1-3", "locate", "/abc/"]);
    assert_eq!(p, vec!["abCxxx"]);
    assert_eq!(s, vec!["zzzxxx"]);
}

#[test]
fn casei_reverse_whole_record() {
    // REVERSE after uppercase: `noX` → `NOX` → `XON`; locate `/xon/`.
    let recs = &["noX", "keep"];
    let (p, s) = run_both(recs, &["reverse", "locate", "/xon/"]);
    assert_eq!(p, vec!["noX"]);
    assert_eq!(s, vec!["keep"]);
}

#[test]
fn casei_zone_then_reverse() {
    // Railroad: ZONE then REVERSE. Last 3 of `xxon` → `xon` → `XON` → `NOX`.
    let recs = &["xxon", "keep"];
    let (p, s) = run_both(recs, &["zone", "-3;-1", "reverse", "locate", "/nox/"]);
    assert_eq!(p, vec!["xxon"]);
    assert_eq!(s, vec!["keep"]);
}

#[test]
fn casei_nlocate() {
    let recs = &["Abc", "xyz"];
    let (p, s) = run_both(recs, &["nlocate", "/abc/"]);
    assert_eq!(p, vec!["xyz"]);
    assert_eq!(s, vec!["Abc"]);
}

#[test]
fn casei_pick() {
    let recs = &["aa", "AA", "bb"];
    let (p, s) = run_both(recs, &["pick", "1-*", "==", "/aa/"]);
    assert_eq!(p, vec!["aa", "AA"]);
    assert_eq!(s, vec!["bb"]);
}

#[test]
fn casei_copy_passes_originals_through() {
    let recs = &["aBc", "Xy"];
    let (p, s) = run_both(recs, &["copy"]);
    assert_eq!(p, vec!["aBc", "Xy"]);
    assert!(s.is_empty());
}

#[test]
fn casei_secondary_discarded_when_not_connected() {
    let out = run_primary_only(&["Abc", "xyz"], &["locate", "/abc/"]);
    assert_eq!(out, vec!["Abc"]);
}

#[test]
fn casei_null_records() {
    let recs = &["", "Abc", ""];
    let (p, s) = run_both(recs, &["locate", "/abc/"]);
    assert_eq!(p, vec!["Abc"]);
    assert_eq!(s, vec!["".to_string(), "".into()]);
}

#[test]
fn casei_empty_input() {
    let (p, s) = run_both(&[], &["locate", "/abc/"]);
    assert!(p.is_empty());
    assert!(s.is_empty());
}

#[test]
fn casei_strfind_anycase_is_redundant_but_works() {
    // Note 1: built-in ANYCASE exists; CASEI still wraps STRFIND.
    let recs = &["Abc", "xyz"];
    let (p, s) = run_both(recs, &["strfind", "/abc/"]);
    assert_eq!(p, vec!["Abc"]);
    assert_eq!(s, vec!["xyz"]);
}

#[test]
fn casei_word_and_string_are_parsed_once() {
    // Note 2: parsed once with CASEI, not as a nested pipeline spec.
    let parsed =
        pipe_parser::parse_pipeline("literal x | casei locate /abc/ | hole").expect("parse");
    assert_eq!(parsed.pipelines[0][1].verb.to_ascii_uppercase(), "CASEI");
    let args: Vec<&str> = parsed.pipelines[0][1]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(args, vec!["locate", "/abc/"]);
}

#[test]
fn casei_missing_word_is_rejected() {
    let err = run_spec("literal a | casei | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );
}

#[test]
fn casei_zone_without_range_is_rejected() {
    let err = run_spec("literal a | casei zone locate /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("inputRange"), "{err}");
}

#[test]
fn casei_zone_without_word_is_rejected() {
    let err = run_spec("literal a | casei zone 1-3 | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );
}

#[test]
fn casei_unknown_nested_stage_is_rejected() {
    let err = run_spec("literal a | casei nosuch /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("NOSUCH"),
        "{err}"
    );
}

#[test]
fn casei_anycase_is_not_a_casei_option() {
    // ANYCASE is not in the CASEI railroad; it is the nested verb.
    let err = run_spec("literal a | casei anycase locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("ANYCASE"),
        "{err}"
    );
}

#[test]
fn casei_reverse_without_zone_then_nested_zone() {
    // Railroad is `[ZONE inputRange] [REVERSE] word`. REVERSE without ZONE
    // is the CASEI option; a following ZONE is the nested verb, not a CASEI
    // option out of order.
    let recs = &["abCxxx"];
    let (p, s) = run_both(recs, &["reverse", "zone", "1-3", "locate", "/cba/"]);
    // Whole record `abCxxx` → `ABCXXX` → reverse `XXXCBA`; ZONE 1-3 is `XXX`.
    assert_eq!(p, Vec::<String>::new());
    assert_eq!(s, vec!["abCxxx"]);

    let (p, s) = run_both(recs, &["reverse", "zone", "-3;-1", "locate", "/cba/"]);
    assert_eq!(p, vec!["abCxxx"]);
    assert_eq!(s, Vec::<String>::new());
}

#[test]
fn casei_invented_caseless_is_rejected() {
    let err = run_spec("literal a | casei caseless locate /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn casei_nested_operand_error_is_the_selection_rc() {
    let err = run_spec("literal a | casei locate /a/ extra | hole").unwrap_err();
    let t = err.to_string();
    assert!(
        t.contains("unexpected operand") || t.contains("LOCATE"),
        "{err}"
    );
}

#[test]
fn casei_verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("CASEI"));
    assert!(!reg.contains("CASE"));
    assert!(!reg.contains("CAS"));
    let err = run_spec("literal a | case locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("CASE"),
        "{err}"
    );
}

#[test]
fn casei_secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "CASEI",
        reg.create("CASEI", vec!["locate".into(), "/a/".into()])
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
fn casei_premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(std::cell::Cell::new(0u32));
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
        "CASEI",
        reg.create("CASEI", vec!["locate".into(), "/one/".into()])
            .unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "CASEI should stop reading when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn casei_hex_dstring_is_uppercased_and_raw_ascii() {
    // `/abc/` becomes `/ABC/`; hex `x414243` is ASCII `ABC` (not EBCDIC).
    let recs = &["abc", "xyz"];
    let (p, s) = run_both(recs, &["locate", "x414243"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["xyz"]);
}

#[test]
fn casei_does_not_delay_order() {
    let recs = &["a", "Abc", "b"];
    let (p, s) = run_both(recs, &["locate", "/abc/"]);
    assert_eq!(p, vec!["Abc"]);
    assert_eq!(s, vec!["a", "b"]);
}

#[test]
fn casei_end_to_end_secondary() {
    let collected_p = Rc::new(RefCell::new(Vec::new()));
    let collected_s = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    reg.register(">>P", {
        let c = Rc::clone(&collected_p);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    reg.register(">>S", {
        let c = Rc::clone(&collected_s);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    let spec = "(ENDCHAR ?) literal xyz|literal Abc| C: casei locate /abc/ | >>P ? C: | >>S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    assert_eq!(*collected_p.borrow(), vec!["Abc".to_string()]);
    assert_eq!(*collected_s.borrow(), vec!["xyz".to_string()]);
}
