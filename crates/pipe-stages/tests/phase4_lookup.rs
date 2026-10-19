//! LOOKUP — Author's Edition 1.1.11 pp. 416–423 (tutorial pp. 87–88, Figure 178).

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

struct Streams {
    primary: Vec<String>,
    secondary: Vec<String>,
    tertiary: Vec<String>,
    quaternary: Vec<String>,
    quinary: Vec<String>,
    senary: Vec<String>,
}

fn run_full(
    details: &[&str],
    masters: &[&str],
    adds: &[&str],
    deletes: &[&str],
    args: &[&str],
) -> Streams {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let t = Rc::new(RefCell::new(Vec::new()));
    let q = Rc::new(RefCell::new(Vec::new()));
    let qn = Rc::new(RefCell::new(Vec::new()));
    let sn = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let dsrc = b.add_stage("details", emit(details));
    let msrc = b.add_stage("masters", emit(masters));
    let asrc = b.add_stage("adds", emit(adds));
    let xsrc = b.add_stage("deletes", emit(deletes));
    let stage = b.add_stage(
        "LOOKUP",
        reg.create("LOOKUP", args.iter().map(|a| (*a).to_string()).collect())
            .expect("missing LOOKUP"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    let sink_t = b.add_stage("sink_t", sink(Rc::clone(&t)));
    let sink_q = b.add_stage("sink_q", sink(Rc::clone(&q)));
    let sink_qn = b.add_stage("sink_qn", sink(Rc::clone(&qn)));
    let sink_sn = b.add_stage("sink_sn", sink(Rc::clone(&sn)));
    b.connect(dsrc, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(msrc, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(asrc, StreamId::PRIMARY, stage, StreamId(2));
    b.connect(xsrc, StreamId::PRIMARY, stage, StreamId(3));
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.connect(stage, StreamId(2), sink_t, StreamId::PRIMARY);
    b.connect(stage, StreamId(3), sink_q, StreamId::PRIMARY);
    b.connect(stage, StreamId(4), sink_qn, StreamId::PRIMARY);
    b.connect(stage, StreamId(5), sink_sn, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = p.borrow().clone();
    let secondary = s.borrow().clone();
    let tertiary = t.borrow().clone();
    let quaternary = q.borrow().clone();
    let quinary = qn.borrow().clone();
    let senary = sn.borrow().clone();
    Streams {
        primary,
        secondary,
        tertiary,
        quaternary,
        quinary,
        senary,
    }
}

fn run_three(details: &[&str], masters: &[&str], args: &[&str]) -> Streams {
    run_full(details, masters, &[], &[], args)
}

fn run_spec_collect(spec: &str) -> Vec<String> {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let sink_out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let sink_out = Rc::clone(&sink_out);
        move |_| pipe_stages::collect_to(Rc::clone(&sink_out))
    });
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse options");
    let full = format!("{spec} {} >>COLLECT", parsed.options.separator);
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn run_two_collect(spec: &str) -> (Vec<String>, Vec<String>) {
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    reg.register(">>P", {
        let c = Rc::clone(&primary);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    reg.register(">>S", {
        let c = Rc::clone(&secondary);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

#[allow(dead_code)]
fn _keep_run_two_collect_signature() {
    let _ = run_two_collect as fn(&str) -> (Vec<String>, Vec<String>);
}

fn pad10(n: i32) -> String {
    format!("{n:>10}")
}

#[test]
fn lookup_book_default_detail_then_master() {
    // Tutorial p.87 / inventory p.418: default writes the detail followed by
    // the matching master. Portable ASCII keys (the book prints no hex here).
    let out = run_three(
        &["apple", "apricot", "banana"],
        &["apple", "banana", "cherry"],
        &[],
    );
    assert_eq!(out.primary, vec!["apple", "apple", "banana", "banana"]);
    assert_eq!(out.secondary, vec!["apricot"]);
    assert_eq!(out.tertiary, vec!["cherry"]);
}

#[test]
fn lookup_book_dictionary_three_streams() {
    // p.420 generic EXEC: details, masters, unmatched details, unreferenced masters.
    // Key is columns 1–5 (portable stand-in for the printed `lookup 1.10`).
    let out = run_three(
        &["apple pie", "zebra"],
        &["apple fruit", "peach fruit"],
        &["1.5"],
    );
    assert_eq!(out.primary, vec!["apple pie", "apple fruit"]);
    assert_eq!(out.secondary, vec!["zebra"]);
    assert_eq!(out.tertiary, vec!["peach fruit"]);
}

#[test]
fn lookup_book_autoadd_keyonly_first_occurrence() {
    // p.421: `lookup 1.5 autoadd keyonly` with primary in + secondary out only.
    // First occurrence of each key is unmatched and written; later copies match.
    let out = run_three(
        &["aaaaa", "bbbbb", "aaaaa", "ccccc", "aaaaa"],
        &[],
        &["1.5", "AUTOADD", "KEYONLY"],
    );
    assert_eq!(out.secondary, vec!["aaaaa", "bbbbb", "ccccc"]);
    // Matched later copies go to primary (KEYONLY master is the key).
    assert_eq!(out.primary, vec!["aaaaa", "aaaaa", "aaaaa", "aaaaa"]);
}

#[test]
fn lookup_book_figure_178_stop_words() {
    // Tutorial Figure 178 p.88: stop-word list on the secondary; matching
    // details are discarded (primary unused); unmatched details are the
    // words to keep; unused stops leave on the tertiary.
    let out = run_three(
        &["the", "cat", "and", "the", "hat"],
        &["the", "and", "of"],
        &[],
    );
    assert_eq!(out.primary, vec!["the", "the", "and", "and", "the", "the"]);
    assert_eq!(out.secondary, vec!["cat", "hat"]);
    assert_eq!(out.tertiary, vec!["of"]);
}

#[test]
fn lookup_end_to_end_labels() {
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let tertiary = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    reg.register(">>P", {
        let c = Rc::clone(&primary);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    reg.register(">>S", {
        let c = Rc::clone(&secondary);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    reg.register(">>T", {
        let c = Rc::clone(&tertiary);
        move |_| pipe_stages::collect_to(Rc::clone(&c))
    });
    // p.420 dictionary EXEC: details | l: lookup | matched ? masters | l: |
    // unmatched ? l: | leftover. One record per LITERAL (no cascade).
    let spec = "(ENDCHAR ?) literal apricot| L: lookup | >>P ? literal apple| L: | >>S ? L: | >>T";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap_or_else(|e| panic!("run failed: {e}"));
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    let t = tertiary.borrow().clone();
    assert_eq!(p, Vec::<String>::new());
    assert_eq!(s, vec!["apricot"]);
    assert_eq!(t, vec!["apple"]);
}

#[test]
fn lookup_routes_every_output_stream() {
    let out = run_full(
        &["keep", "miss"],
        &["keep", "keep", "dropme"],
        &["added"],
        &["dropme", "nobody"],
        &["ALLMASTER"],
    );
    // ALLMASTER: matching masters only (detail discarded). Duplicate master kept.
    assert_eq!(out.primary, vec!["keep", "keep"]);
    assert_eq!(out.secondary, vec!["miss"]);
    // added is unreferenced; dropme was deleted (not leftover).
    assert_eq!(out.tertiary, vec!["added"]);
    assert_eq!(out.quaternary, vec!["dropme"]);
    assert!(out.quinary.is_empty(), "{:?}", out.quinary);
    assert_eq!(out.senary, vec!["nobody"]);
}

#[test]
fn lookup_duplicate_masters_to_quinary() {
    let out = run_three(&["a"], &["a", "a", "b"], &[]);
    assert_eq!(out.primary, vec!["a", "a"]);
    assert_eq!(out.quinary, vec!["a"]);
    assert_eq!(out.tertiary, vec!["b"]);
}

#[test]
fn lookup_allmaster_keeps_duplicates() {
    let out = run_three(&["a"], &["a-one", "a-two", "b"], &["1", "ALLMASTER"]);
    assert_eq!(out.primary, vec!["a-one", "a-two"]);
    assert!(out.quinary.is_empty());
    assert_eq!(out.tertiary, vec!["b"]);
}

#[test]
fn lookup_detail_allmaster_pairwise() {
    let out = run_three(
        &["x"],
        &["x1", "x2"],
        &["1", "DETAIL", "ALLMASTER", "PAIRWISE"],
    );
    assert_eq!(out.primary, vec!["x", "x1", "x", "x2"]);
}

#[test]
fn lookup_allmaster_detail_pairwise() {
    let out = run_three(
        &["x"],
        &["x1", "x2"],
        &["1", "ALLMASTER", "DETAIL", "PAIRWISE"],
    );
    assert_eq!(out.primary, vec!["x1", "x", "x2", "x"]);
}

#[test]
fn lookup_write_mode_detail_and_master() {
    let d = run_three(&["a", "b"], &["a"], &["DETAIL"]);
    assert_eq!(d.primary, vec!["a"]);
    assert_eq!(d.secondary, vec!["b"]);

    let m = run_three(&["a", "b"], &["a-master"], &["1", "MASTER"]);
    assert_eq!(m.primary, vec!["a-master"]);
    assert_eq!(m.secondary, vec!["b"]);

    let md = run_three(&["a"], &["a-master"], &["1", "MASTER", "DETAIL"]);
    assert_eq!(md.primary, vec!["a-master", "a"]);
}

#[test]
fn lookup_both_and_matching_synonyms() {
    let both = run_three(&["a"], &["a"], &["BOTH"]);
    assert_eq!(both.primary, vec!["a", "a"]);
    let matching = run_three(&["a", "b"], &["a"], &["MATCHING"]);
    assert_eq!(matching.primary, vec!["a"]);
    assert_eq!(matching.secondary, vec!["b"]);
}

#[test]
fn lookup_count_prefixes_all_masters_on_tertiary() {
    let out = run_three(&["a", "a", "b"], &["a", "c"], &["COUNT"]);
    assert_eq!(
        out.tertiary,
        vec![format!("{}a", pad10(2)), format!("{}c", pad10(0))]
    );
}

#[test]
fn lookup_without_count_writes_only_unreferenced() {
    let out = run_three(&["a"], &["a", "c"], &[]);
    assert_eq!(out.tertiary, vec!["c"]);
}

#[test]
fn lookup_trackcount_prefixes_primary_master() {
    let out = run_three(&["a"], &["a"], &["TRACKCOUNT", "MASTER"]);
    assert_eq!(out.primary, vec![format!("{}a", pad10(1))]);
}

#[test]
fn lookup_setcount_and_increment() {
    // SETCOUNT: first 10 columns of the master are the starting count.
    // INCREMENT: first 10 columns of the detail are added, then stripped.
    let masters = [format!("{}item", pad10(4))];
    let details = [format!("{}item", pad10(2))];
    let master_refs: Vec<&str> = masters.iter().map(|s| s.as_str()).collect();
    let detail_refs: Vec<&str> = details.iter().map(|s| s.as_str()).collect();
    let out = run_three(
        &detail_refs,
        &master_refs,
        &["COUNT", "INCREMENT", "SETCOUNT"],
    );
    assert_eq!(out.primary, vec!["item", "item"]);
    assert_eq!(out.tertiary, vec![format!("{}item", pad10(6))]);
}

#[test]
fn lookup_increment_blank_defaults_to_one() {
    let details = [format!("{}z", "          ")];
    let d: Vec<&str> = details.iter().map(|s| s.as_str()).collect();
    let out = run_three(&d, &["z"], &["COUNT", "INCREMENT"]);
    assert_eq!(out.primary, vec!["z", "z"]);
    assert_eq!(out.tertiary, vec![format!("{}z", pad10(1))]);
}

#[test]
fn lookup_increment_zero_stays_unreferenced() {
    let details = [format!("{}z", pad10(0))];
    let d: Vec<&str> = details.iter().map(|s| s.as_str()).collect();
    let out = run_three(&d, &["z"], &["INCREMENT"]);
    assert_eq!(out.primary, vec!["z", "z"]);
    assert_eq!(out.tertiary, vec!["z"]);
}

#[test]
fn lookup_countdbg_four_spaces_before_count() {
    let out = run_three(&["a"], &["a", "b"], &["COUNTDBG"]);
    assert_eq!(
        out.tertiary,
        vec![format!("    {}a", pad10(1)), format!("    {}b", pad10(0))]
    );
}

#[test]
fn lookup_anycase_and_synonyms() {
    let out = run_three(&["Foo"], &["foo"], &["ANYCASE"]);
    assert_eq!(out.primary, vec!["Foo", "foo"]);
    for kw in ["ANY", "CASEANY", "IGNORECASE", "CASEIGNORE"] {
        let o = run_three(&["Foo"], &["foo"], &[kw]);
        assert_eq!(o.primary, vec!["Foo", "foo"], "{kw}");
    }
}

#[test]
fn lookup_pad_xorc_and_nopad() {
    // Portable ASCII: BLANK is X'20' (book cites EBCDIC X'40').
    let padded = run_three(&["AB"], &["AB  "], &["PAD", "BLANK"]);
    assert_eq!(padded.primary, vec!["AB", "AB  "]);
    let nopad = run_three(&["AB"], &["AB  "], &["NOPAD"]);
    assert!(nopad.primary.is_empty());
    assert_eq!(nopad.secondary, vec!["AB"]);
}

#[test]
fn lookup_autoadd_before_counts_one() {
    let out = run_three(&["new", "new"], &[], &["COUNT", "AUTOADD", "BEFORE"]);
    assert_eq!(out.primary[0], "new");
    assert_eq!(out.secondary, Vec::<String>::new());
    assert_eq!(out.tertiary, vec![format!("{}new", pad10(2))]);
}

#[test]
fn lookup_autoadd_without_before_first_is_unmatched() {
    let out = run_three(&["new", "new"], &[], &["AUTOADD"]);
    assert_eq!(out.secondary, vec!["new"]);
    assert_eq!(out.primary, vec!["new", "new"]);
}

#[test]
fn lookup_ceiling_and_floor() {
    let ceil = run_three(&["b"], &["a", "c", "e"], &["CEILING"]);
    assert_eq!(ceil.primary, vec!["b", "c"]);
    let high = run_three(&["z"], &["a", "c"], &["CEILING"]);
    assert_eq!(high.secondary, vec!["z"]);

    let floor = run_three(&["b"], &["a", "c"], &["FLOOR"]);
    assert_eq!(floor.primary, vec!["b", "a"]);
    let low = run_three(&[" "], &["a"], &["FLOOR"]);
    assert_eq!(low.secondary, vec![" "]);
}

#[test]
fn lookup_keyonly_stores_key() {
    let out = run_three(&["apple pie"], &["apple fruit"], &["KEYONLY", "1.5"]);
    assert_eq!(out.primary, vec!["apple pie", "apple"]);
}

#[test]
fn lookup_two_ranges() {
    let out = run_three(&["xxapple"], &["appleYY"], &["3.5", "1.5"]);
    assert_eq!(out.primary, vec!["xxapple", "appleYY"]);
}

#[test]
fn lookup_null_records() {
    let out = run_three(&[""], &[""], &[]);
    assert_eq!(out.primary, vec!["", ""]);
    assert!(out.secondary.is_empty());
    assert!(out.tertiary.is_empty());

    let miss = run_three(&[""], &["x"], &[]);
    assert_eq!(miss.secondary, vec![""]);
    assert_eq!(miss.tertiary, vec!["x"]);
}

#[test]
fn lookup_tertiary_sorts_by_key() {
    let out = run_three(&[], &["c", "a", "b"], &[]);
    assert_eq!(out.tertiary, vec!["a", "b", "c"]);
}

#[test]
fn lookup_word_range() {
    let out = run_three(&["keep value"], &["keep master"], &["WORD", "1"]);
    assert_eq!(out.primary, vec!["keep value", "keep master"]);
}

#[test]
fn lookup_strict_is_accepted() {
    let out = run_three(&["a"], &["a"], &["STRICT"]);
    assert_eq!(out.primary, vec!["a", "a"]);
}

#[test]
fn lookup_increment_setcount_short_record_errors() {
    let err =
        run_spec("(ENDCHAR ?) literal short | L: lookup increment | hole ? literal master | L:")
            .unwrap_err();
    assert!(
        err.to_string().contains("10") || err.to_string().contains("INCREMENT"),
        "{err}"
    );

    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let d = b.add_stage("d", emit(&["abcdefghijX"]));
    let m = b.add_stage("m", emit(&["short"]));
    let stage = b.add_stage(
        "LOOKUP",
        reg.create("LOOKUP", vec!["SETCOUNT".into()]).unwrap(),
    );
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect_primary(d, stage);
    b.connect(m, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string().contains("10") || err.to_string().contains("SETCOUNT"),
        "{err}"
    );
}

#[test]
fn lookup_operand_errors() {
    let err = run_spec("literal a | lookup after | hole").unwrap_err();
    assert!(err.to_string().contains("AFTER"), "{err}");

    let err = run_spec("literal a | lookup unique | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | lookup pad | hole").unwrap_err();
    assert!(err.to_string().contains("PAD"), "{err}");

    let err = run_spec("literal a | lookup detail count | hole").unwrap_err();
    assert!(
        err.to_string().contains("diagram order") || err.to_string().contains("COUNT"),
        "{err}"
    );

    let err = run_spec("literal a | lookup pairwise | hole").unwrap_err();
    assert!(err.to_string().contains("PAIRWISE"), "{err}");

    let err = run_spec("literal a | lookup allmaster pairwise | hole").unwrap_err();
    assert!(err.to_string().contains("PAIRWISE"), "{err}");

    let err = run_spec("literal a | lookup autoadd 1.5 2.5 | hole").unwrap_err();
    assert!(err.to_string().contains("AUTOADD"), "{err}");

    let err = run_spec("literal a | lookup increment extra | hole").unwrap_err();
    assert!(err.to_string().contains("unexpected"), "{err}");

    let err = run_spec("literal a | lookup before | hole").unwrap_err();
    assert!(
        err.to_string().contains("BEFORE") || err.to_string().contains("diagram order"),
        "{err}"
    );
}

#[test]
fn lookup_no_invented_abbreviation() {
    let reg = StageRegistry::default();
    assert!(reg.contains("LOOKUP"));
    assert!(!reg.contains("LOOK"));
    assert!(!reg.contains("LOOKU"));
    let err = run_spec("literal a | looku | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn lookup_increment_trackcount_abbreviations() {
    let details = [format!("{}a", pad10(1))];
    let d: Vec<&str> = details.iter().map(|s| s.as_str()).collect();
    let out = run_three(&d, &["a"], &["INCREMEN", "TRACKCOU", "MASTER"]);
    assert_eq!(out.primary, vec![format!("{}a", pad10(1))]);
}

#[test]
fn lookup_quinary_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let d = b.add_stage("d", emit(&["a"]));
    let extra = b.add_stage("x", emit(&["z"]));
    let stage = b.add_stage("LOOKUP", reg.create("LOOKUP", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect_primary(d, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId(4));
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string().contains("quinary") || err.to_string().contains("senary"),
        "{err}"
    );
}

#[test]
fn lookup_premature_termination_no_output() {
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["one", "two"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let masters = b.add_stage("masters", emit(&["one"]));
    let stage = b.add_stage("LOOKUP", reg.create("LOOKUP", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.connect(masters, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.run().unwrap();
    assert_eq!(
        written.get(),
        0,
        "disconnected output must short the producer"
    );
}

#[test]
fn lookup_one_stream_without_autoadd_rejected() {
    let err = run_spec("literal a | lookup | hole").unwrap_err();
    assert!(
        err.to_string().contains("two streams") || err.to_string().contains("LOOKUP"),
        "{err}"
    );
}

#[test]
fn lookup_sticky_count_floor() {
    let details = [format!("{}a", pad10(-5))];
    let d: Vec<&str> = details.iter().map(|s| s.as_str()).collect();
    let out = run_three(&d, &["a"], &["COUNT", "INCREMENT"]);
    assert_eq!(out.tertiary, vec![format!("{}a", pad10(0))]);
}

#[test]
fn lookup_hex_range_is_ascii_not_ebcdic() {
    // Book examples that print EBCDIC hex (e.g. xf1f2f3 → 123) are not used
    // as keys here; portable ASCII `x61` is 'a'.
    let out = run_spec_collect(
        "(ENDCHAR ?) literal apple| L: lookup 1 | hole ? literal apple| L: | hole",
    );
    assert!(out.is_empty());
}

#[test]
fn lookup_tertiary_input_adds_to_reference() {
    // STRICT (p.418): tertiary before primary, so the add is in the
    // reference before the detail is looked up.
    let out = run_full(&["late"], &["early"], &["late"], &[], &["STRICT"]);
    assert_eq!(out.primary, vec!["late", "late"]);
    assert!(out.secondary.is_empty());
    assert_eq!(out.tertiary, vec!["early"]);
}

#[test]
fn lookup_allmasters_prose_spelling() {
    let out = run_three(&["a"], &["a1", "a2"], &["1", "ALLMASTERS"]);
    assert_eq!(out.primary, vec!["a1", "a2"]);
}
