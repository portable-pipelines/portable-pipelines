//! ZONE — Author's Edition 1.1.11 pp. 610–611 (tutorial pp. 52, 67 Figure 140).

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

fn emit_bytes(records: Vec<Vec<u8>>) -> Box<dyn FnOnce(&mut StageCtx)> {
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

fn sink_bytes(out: Rc<RefCell<Vec<Vec<u8>>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        while let Ok(Some(r)) = ctx.read() {
            out.borrow_mut().push(r.as_bytes().to_vec());
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
        "ZONE",
        reg.create("ZONE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing ZONE"),
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
        "ZONE",
        reg.create("ZONE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing ZONE"),
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

fn run_both_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "ZONE",
        reg.create("ZONE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing ZONE"),
    );
    let sink_p = b.add_stage("sink_p", sink_bytes(p));
    let sink_s = b.add_stage("sink_s", sink_bytes(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

// --- Book examples (1.1.11 pp. 610, 67). Snippets have no Ready; listing;
// the asserted output is the selection the surrounding prose describes.

#[test]
fn zone_book_reverse_between_on_off() {
    // p.610: `zone -3;-1 reverse between /no/ /ffo/`
    // “select records between those that end in on and off”.
    let recs = &["xxon", "middle", "xxoff", "after"];
    let (p, s) = run_both(recs, &["-3;-1", "reverse", "between", "/no/", "/ffo/"]);
    assert_eq!(p, vec!["xxon", "middle", "xxoff"]);
    assert_eq!(s, vec!["after"]);
}

#[test]
fn zone_book_figure_140_opcode_columns() {
    // p.67 Figure 140: `zone 9.8 find _PIPERM_` is equivalent to
    // `locate 9.8 / PIPERM /`. FIND is not registered (Phase 6); LOCATE on the
    // extracted zone is the portable equivalent. Pattern is 8 characters
    // (space + PIPERM + space) in columns 9–16.
    let hit = "xxxxxxxx PIPERM rest";
    let miss = "xxxxxxxx COPY   rest";
    let other = "PIPERM at start";
    let (p, s) = run_both(&[hit, miss, other], &["9.8", "locate", "/ PIPERM /"]);
    assert_eq!(p, vec![hit]);
    assert_eq!(s, vec![miss.to_string(), other.into()]);
}

#[test]
fn zone_book_figure_140_find_is_unknown() {
    // Figure 140 prints `find`; that verb is not registered.
    let err = run_spec("literal xxxxxxxx PIPERM x | zone 9.8 find _PIPERM_ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("FIND"),
        "{err}"
    );
}

#[test]
fn zone_writes_the_original_record() {
    let recs = &["ABCdef", "XYZdef"];
    let (p, s) = run_both(recs, &["1-3", "locate", "/ABC/"]);
    assert_eq!(p, vec!["ABCdef"]);
    assert_eq!(s, vec!["XYZdef"]);
}

#[test]
fn zone_casei_uppercases_zone_and_argument() {
    // CASEI (p.610 / see p.285) uppercases the zone and the inner argument.
    let recs = &["abCxxx", "zzzxxx"];
    let (p, s) = run_both(recs, &["1-3", "casei", "locate", "/abc/"]);
    assert_eq!(p, vec!["abCxxx"]);
    assert_eq!(s, vec!["zzzxxx"]);
}

#[test]
fn zone_casei_then_reverse() {
    // Railroad: CASEI then REVERSE. “on” → last 3 “xon” → “XON” → “NOX”.
    let recs = &["xxon", "keep"];
    let (p, s) = run_both(recs, &["-3;-1", "casei", "reverse", "locate", "/nox/"]);
    assert_eq!(p, vec!["xxon"]);
    assert_eq!(s, vec!["keep"]);
}

#[test]
fn zone_word_range_locate() {
    let recs = &["aa target zz", "aa other zz"];
    let (p, s) = run_both(recs, &["word", "2", "locate", "/target/"]);
    assert_eq!(p, vec!["aa target zz"]);
    assert_eq!(s, vec!["aa other zz"]);
}

#[test]
fn zone_pick_on_columns() {
    let recs = &["0759 rest", "0800 rest", "1600 rest"];
    let (p, s) = run_both(recs, &["1.4", "pick", "1-*", "<<", "/0800/"]);
    assert_eq!(p, vec!["0759 rest"]);
    assert_eq!(s, vec!["0800 rest".to_string(), "1600 rest".into()]);
}

#[test]
fn zone_nlocate() {
    let recs = &["ABzz", "XXzz"];
    let (p, s) = run_both(recs, &["1-2", "nlocate", "/AB/"]);
    assert_eq!(p, vec!["XXzz"]);
    assert_eq!(s, vec!["ABzz"]);
}

#[test]
fn zone_take_first() {
    let recs = &["a1", "b1", "c1"];
    let (p, s) = run_both(recs, &["1", "take", "2"]);
    assert_eq!(p, vec!["a1", "b1"]);
    assert_eq!(s, vec!["c1"]);
}

#[test]
fn zone_unique_on_prefix() {
    let recs = &["aa1", "aa2", "bb1", "aa3"];
    let (p, s) = run_both(recs, &["1-2", "unique"]);
    assert_eq!(p, vec!["aa2", "bb1", "aa3"]);
    assert_eq!(s, vec!["aa1"]);
}

#[test]
fn zone_hex_dstring_is_raw_ascii_bytes() {
    // Portable ASCII: `x4142` is "AB". The book's `xf1f2f3` example is EBCDIC.
    let recs = &["ABc", "XYc"];
    let (p, s) = run_both(recs, &["1-2", "locate", "x4142"]);
    assert_eq!(p, vec!["ABc"]);
    assert_eq!(s, vec!["XYc"]);
}

#[test]
fn zone_binary_dstring_is_raw_bytes() {
    // `b01000001` is ASCII 'A' (not EBCDIC).
    let recs = &["Abc", "Xbc"];
    let (p, _) = run_both(recs, &["1", "locate", "b01000001"]);
    assert_eq!(p, vec!["Abc"]);
}

#[test]
fn zone_null_input_records() {
    let recs = &["", "abc", ""];
    let (p, s) = run_both(recs, &["1-3", "locate", "/a/"]);
    assert_eq!(p, vec!["abc"]);
    assert_eq!(s, vec!["".to_string(), "".into()]);
}

#[test]
fn zone_null_record_with_nlocate_no_operand() {
    // NLOCATE with no operand selects null records (p.439). An empty zone
    // from a null input is a null record to the selection stage.
    let recs = &["", "x", ""];
    let (p, s) = run_both(recs, &["1-*", "nlocate"]);
    assert_eq!(p, vec!["".to_string(), "".into()]);
    assert_eq!(s, vec!["x"]);
}

#[test]
fn zone_short_record_extracts_empty() {
    // Columns 10–20 of a 2-byte record yield a null zone.
    let recs = &["hi", "abcdefghijklmnopxxxx"];
    let (p, s) = run_both(recs, &["10-20", "locate", "/xx/"]);
    assert_eq!(p, vec!["abcdefghijklmnopxxxx"]);
    assert_eq!(s, vec!["hi"]);
}

#[test]
fn zone_empty_zone_bytes() {
    let recs = vec![Vec::new(), b"AB".to_vec()];
    let (p, s) = run_both_bytes(recs, &["1-2", "nlocate"]);
    assert_eq!(p, vec![Vec::<u8>::new()]);
    assert_eq!(s, vec![b"AB".to_vec()]);
}

#[test]
fn zone_secondary_discarded_when_not_connected() {
    let out = run_primary_only(&["ABzz", "XXzz"], &["1-2", "locate", "/AB/"]);
    assert_eq!(out, vec!["ABzz"]);
}

#[test]
fn zone_end_to_end_parser() {
    let out = run_spec_collect(
        "literal xxon middle xxoff after | split | zone -3;-1 reverse between /no/ /ffo/",
    );
    assert_eq!(out, vec!["xxon", "middle", "xxoff"]);

    let parsed = pipe_parser::parse_pipeline("literal x | zone 1-3 casei reverse locate /AB/")
        .expect("parse");
    assert_eq!(parsed.pipelines[0][1].verb.to_ascii_uppercase(), "ZONE");
    let args: Vec<&str> = parsed.pipelines[0][1]
        .args
        .iter()
        .map(|a| a.as_str())
        .collect();
    assert_eq!(args, vec!["1-3", "casei", "reverse", "locate", "/AB/"]);

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
    let spec =
        "(ENDCHAR ?) literal ABCzzz|literal XYZzzz| Z: zone 1-3 locate /ABC/ | >>P ? Z: | >>S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    // literal cascade is reverse order: XYZzzz then ABCzzz.
    assert_eq!(*collected_p.borrow(), vec!["ABCzzz".to_string()]);
    assert_eq!(*collected_s.borrow(), vec!["XYZzzz".to_string()]);
}

#[test]
fn zone_operand_errors() {
    let err = run_spec("literal a | zone | hole").unwrap_err();
    assert!(err.to_string().contains("inputRange"), "{err}");

    let err = run_spec("literal a | zone locate /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("inputRange"), "{err}");

    let err = run_spec("literal a | zone 1-3 | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );

    let err = run_spec("literal a | zone 1-3 casei | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );

    let err = run_spec("literal a | zone 1-3 reverse | hole").unwrap_err();
    assert!(
        err.to_string().contains("word") || err.to_string().contains("program"),
        "{err}"
    );

    // REVERSE before CASEI is not in the railroad; CASEI is then the program name.
    let err = run_spec("literal a | zone 1-3 reverse casei locate /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");

    // ANYCASE is not a ZONE option (CASEI is).
    let err = run_spec("literal a | zone 1-3 anycase locate /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");

    let err = run_spec("literal a | zone 1-3 nosuch /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage")
            && err.to_string().to_ascii_uppercase().contains("NOSUCH"),
        "{err}"
    );

    // Invented synonyms.
    let err = run_spec("literal a | zone 1-3 caseless locate /a/ | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn zone_copy_passes_originals_through() {
    // p.610: the diagram takes any `word`. COPY writes the extracted zone to
    // primary; ZONE writes the original record to that stream.
    let recs = &["abcdef", "xyz", "12"];
    let (p, s) = run_both(recs, &["1.3", "copy"]);
    assert_eq!(p, vec!["abcdef", "xyz", "12"]);
    assert_eq!(s, Vec::<String>::new());

    assert_eq!(
        run_spec_collect("literal abcdef| zone 1.3 copy"),
        vec!["abcdef"]
    );
}

#[test]
fn zone_verb_is_not_abbreviable() {
    // Diagram is all-caps ZONE (p.610); `zon` is not a verb abbreviation.
    assert!(StageRegistry::default().contains("ZONE"));
    assert!(!StageRegistry::default().contains("ZON"));
    let err = run_spec("literal a | zon 1-3 locate /a/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("ZON"),
        "{err}"
    );
}

#[test]
fn zone_premature_termination_without_output() {
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
        "ZONE",
        reg.create("ZONE", vec!["1-*".into(), "locate".into(), "/one/".into()])
            .unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "ZONE should stop reading when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn zone_secondary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "ZONE",
        reg.create("ZONE", vec!["1-*".into(), "locate".into(), "/a/".into()])
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
