//! FRLABEL / TOLABEL / WHILELABEL and STR forms.
//! Author's Edition 1.1.11 pp.353–354, 559–560, 596–597, 534.
//!
//! Portable ASCII: prefix compare is byte-for-byte (hex/binary dstrings are
//! raw bytes). The book's `'02'x || 'ESD'` sample is an EBCDIC/REXX concat;
//! tests use `/abc/` and `x02` (p.534 STRFIND example) instead.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{callpipe_from_spec, run_spec, StageRegistry};

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

fn emit_bytes(records: &[Vec<u8>]) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<Vec<u8>> = records.to_vec();
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
    let full = if spec.trim_end().ends_with('|') {
        format!("{spec} >>COLLECT")
    } else {
        format!("{spec}| >>COLLECT")
    };
    let parsed = pipe_parser::parse_pipeline(&full).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    let result = collected.borrow().clone();
    result
}

fn run_split(verb: &str, records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let recs: Vec<Vec<u8>> = records.iter().map(|s| s.as_bytes().to_vec()).collect();
    run_split_bytes(verb, &recs, args)
}

fn run_split_bytes(verb: &str, records: &[Vec<u8>], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
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

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn frlabel_from_first_prefix() {
    // p.353: discard up to the first record beginning with the string; that
    // record and the rest are selected (INCLUSIVE default).
    let (p, s) = run_split("FRLABEL", &["a", "abc", "b", "c"], &["abc"]);
    assert_eq!(p, vec!["abc", "b", "c"]);
    assert_eq!(s, vec!["a"]);
    assert_eq!(
        run_spec_collect("literal c|literal b|literal abc|literal a| frlabel abc"),
        vec!["abc", "b", "c"]
    );
}

#[test]
fn fromlabel_is_a_synonym() {
    // p.353 Note 1.
    assert_eq!(
        run_spec_collect("literal c|literal abc|literal x| fromlabel abc"),
        vec!["abc", "c"]
    );
}

#[test]
fn tolabel_stops_before_the_match() {
    // p.559: select up to (not including) the first prefix match; EXCLUSIVE
    // default. Remaining including the match go to secondary.
    let (p, s) = run_split("TOLABEL", &["a", "b", ".end", "c"], &[".end"]);
    assert_eq!(p, vec!["a", "b"]);
    assert_eq!(s, vec![".end", "c"]);
    assert_eq!(
        run_spec_collect("literal c|literal .end|literal b|literal a| tolabel .end"),
        vec!["a", "b"]
    );
}

#[test]
fn whilelabel_keeps_a_leading_run() {
    // p.596: copy while records begin with the string; first non-match and
    // the rest go to secondary.
    let (p, s) = run_split("WHILELABEL", &["*a", "*b", "c", "*d"], &["*"]);
    assert_eq!(p, vec!["*a", "*b"]);
    assert_eq!(s, vec!["c", "*d"]);
}

#[test]
fn whilelabel_star_with_hole_discards_the_run() {
    // p.597 DropComm: `whilelabel * | hole` so the stage does not stop for
    // want of a primary output. Remaining records are discarded (no secondary).
    assert_eq!(
        run_spec_collect("literal keep|literal *b|literal *a| whilelabel *| hole"),
        Vec::<String>::new()
    );
}

#[test]
fn null_string_matches_the_first_record() {
    // p.353 / p.559 / p.596: a null argument matches any record.
    let (p, s) = run_split("FRLABEL", &["a", "b"], &[]);
    assert_eq!(p, vec!["a", "b"]);
    assert!(s.is_empty());
    let (p, s) = run_split("TOLABEL", &["a", "b"], &[]);
    assert!(p.is_empty());
    assert_eq!(s, vec!["a", "b"]);
    let (p, s) = run_split("WHILELABEL", &["a", "b"], &[]);
    assert_eq!(p, vec!["a", "b"]);
    assert!(s.is_empty());
}

#[test]
fn shorter_record_does_not_match() {
    let (p, s) = run_split("FRLABEL", &["ab", "abc", "x"], &["abc"]);
    assert_eq!(p, vec!["abc", "x"]);
    assert_eq!(s, vec!["ab"]);
}

#[test]
fn blanks_in_the_label_are_literal() {
    // Unlike FIND, a blank in the FRLABEL string is not don't-care.
    let (p, s) = run_split("FRLABEL", &["axc", "a c", "x"], &["a c"]);
    assert_eq!(p, vec!["a c", "x"]);
    assert_eq!(s, vec!["axc"]);
}

#[test]
fn remainder_blanks_are_kept() {
    // Leading blank after the separator is part of the prefix.
    assert_eq!(
        run_spec_collect("literal  abc|literal abc| frlabel  abc"),
        vec![" abc"]
    );
}

#[test]
fn slashes_are_literal_on_the_plain_form() {
    let (p, s) = run_split("FRLABEL", &["/abc/", "abc"], &["/abc/"]);
    assert_eq!(p, vec!["/abc/", "abc"]);
    assert_eq!(s, Vec::<String>::new());
}

#[test]
fn anycase_is_text_on_the_plain_form() {
    let (p, s) = run_split("FRLABEL", &["xyz", "ANYCASE abc", "rest"], &["ANYCASE abc"]);
    assert_eq!(p, vec!["ANYCASE abc", "rest"]);
    assert_eq!(s, vec!["xyz"]);
}

#[test]
fn prefix_not_substring() {
    let (p, s) = run_split("TOLABEL", &["xabc", "abc", "z"], &["abc"]);
    assert_eq!(p, vec!["xabc"]);
    assert_eq!(s, vec!["abc", "z"]);
}

#[test]
fn strfrlabel_delimited_and_exclusive() {
    // p.353 STR branch: EXCLUSIVE discards the matching record.
    let (p, s) = run_split("STRFRLABEL", &["a", "abc", "b"], &["EXCLUSIVE", "/abc/"]);
    assert_eq!(p, vec!["b"]);
    assert_eq!(s, vec!["a"]);
    let (p, _) = run_split("STRFRLABEL", &["a", "abc", "b"], &["INCLUSIVE", "/abc/"]);
    assert_eq!(p, vec!["abc", "b"]);
}

#[test]
fn strfromlabel_synonym() {
    assert_eq!(
        run_spec_collect("literal c|literal abc|literal x| strfromlabel /abc/"),
        vec!["abc", "c"]
    );
}

#[test]
fn strtolabel_inclusive_keeps_the_match() {
    let (p, s) = run_split("STRTOLABEL", &["a", ".end", "c"], &["INCLUSIVE", "/.end/"]);
    assert_eq!(p, vec!["a", ".end"]);
    assert_eq!(s, vec!["c"]);
}

#[test]
fn strwhilelabel_anycase() {
    let (p, s) = run_split(
        "STRWHILELABEL",
        &["Ab", "aC", "x", "Ad"],
        &["ANYCASE", "/a/"],
    );
    assert_eq!(p, vec!["Ab", "aC"]);
    assert_eq!(s, vec!["x", "Ad"]);
}

#[test]
fn strfrlabel_hex_prefix() {
    // p.534: dstring may be hex. Portable ASCII X'02' (the book's ESD sample
    // concatenates EBCDIC X'02' with ESD in REXX).
    let recs = [vec![0x02, b'E', b'S', b'D'], b"rest".to_vec()];
    let (p, s) = run_split_bytes("STRFRLABEL", &recs, &["x02"]);
    assert_eq!(p, vec!["\u{0002}ESD", "rest"]);
    assert!(s.is_empty());
}

#[test]
fn str_null_dstring_matches_any() {
    let (p, s) = run_split("STRFRLABEL", &["a", "b"], &[",,"]);
    assert_eq!(p, vec!["a", "b"]);
    assert!(s.is_empty());
}

#[test]
fn str_form_requires_a_dstring() {
    let err = err_of("literal a | strfrlabel | hole");
    assert!(err.to_ascii_uppercase().contains("DELIMITED"), "{err}");
    let err = err_of("literal a | strfind | hole");
    assert!(err.to_ascii_uppercase().contains("DELIMITED"), "{err}");
}

#[test]
fn str_form_rejects_undelimited_text() {
    let err = err_of("literal a | strfrlabel abc | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn str_form_rejects_invented_operands_and_orderings() {
    let err = err_of("literal a | strfrlabel /abc/ exclusive | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | strfrlabel exclusive anycase /abc/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
    let err = err_of("literal a | strwhilelabel exclusive /a/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
    let err = err_of("literal a | strtolabel first /a/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
    let err = err_of("literal a | strfrlabel exclusi /abc/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn caseany_synonym_for_anycase() {
    let (p, _) = run_split("STRFRLABEL", &["x", "Abc", "z"], &["CASEANY", "/abc/"]);
    assert_eq!(p, vec!["Abc", "z"]);
}

#[test]
fn str_form_anycase_min_three() {
    // Railroad `ANYcase` (pp.353, 559, 596): min 3, no ANYOF.
    let (p, _) = run_split("STRFRLABEL", &["x", "Abc", "z"], &["ANY", "/abc/"]);
    assert_eq!(p, vec!["Abc", "z"]);
    let err = err_of("literal a | strfrlabel an /abc/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
    let err = err_of("literal a | strtolabel an /.end/ | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn buffering_callpipe_cannot_reread_tolabel_match() {
    // p.559: when secondary is not defined, the matching record stays in the
    // pipeline so a CALLPIPE caller can read it again (`callpipe *: | tolabel x | …`
    // then READTO). v1 CALLPIPE buffers with read_all first, so the leftover is
    // lost (subset; streaming CALLPIPE, Phase 4). TOLABEL peeks the match and
    // does not read() it; short_input then drops the nested slot so a
    // standalone producer is not parked.
    let nested_out = Rc::new(RefCell::new(Vec::new()));
    let leftover = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&nested_out);
    let left = Rc::clone(&leftover);

    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", ".end", "c"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            // Book remainder: no blank before `|` after the string (p.559 example).
            callpipe_from_spec(ctx, "*: | tolabel .end| *:", &StageRegistry::default()).unwrap();
            while let Ok(Some(r)) = ctx.read() {
                left.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    let sink = b.add_stage("sink", sink(out));
    b.connect_primary(src, caller);
    b.connect_primary(caller, sink);
    b.run().unwrap();

    assert_eq!(*nested_out.borrow(), vec!["a".to_string(), "b".to_string()]);
    assert!(
        leftover.borrow().is_empty(),
        "buffering CALLPIPE lost the TOLABEL leftover, got {:?}",
        leftover.borrow()
    );
}

#[test]
fn buffering_callpipe_cannot_reread_whilelabel_nonmatch() {
    // p.596: first non-matching record has the same leftover shape as TOLABEL
    // when secondary is not defined. Buffering CALLPIPE cannot restore it.
    let nested_out = Rc::new(RefCell::new(Vec::new()));
    let leftover = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&nested_out);
    let left = Rc::clone(&leftover);

    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["*a", "*b", "c", "*d"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            callpipe_from_spec(ctx, "*: | whilelabel *| *:", &StageRegistry::default()).unwrap();
            while let Ok(Some(r)) = ctx.read() {
                left.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    let sink = b.add_stage("sink", sink(out));
    b.connect_primary(src, caller);
    b.connect_primary(caller, sink);
    b.run().unwrap();

    assert_eq!(
        *nested_out.borrow(),
        vec!["*a".to_string(), "*b".to_string()]
    );
    assert!(
        leftover.borrow().is_empty(),
        "buffering CALLPIPE lost the WHILELABEL leftover, got {:?}",
        leftover.borrow()
    );
}

#[test]
fn verbs_are_exact_except_strwhile() {
    let err = err_of("literal a | frlab abc | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("FRLAB"),
        "{err}"
    );
    let err = err_of("literal a | tolabe .end | hole");
    assert!(err.contains("unknown stage"), "{err}");
    let err = err_of("literal a | whilelab * | hole");
    assert!(err.contains("unknown stage"), "{err}");
    let err = err_of("literal a | strwhil /a/ | hole");
    assert!(err.contains("unknown stage"), "{err}");
    assert_eq!(
        run_spec_collect("literal b|literal *a| strwhile /*/"),
        vec!["*a"]
    );
}

#[test]
fn strliteral_is_registered() {
    // STRLITERAL is implemented (pp.412, 534); see phase6_strliteral.rs.
    let err = err_of("literal a | strliteral foo | hole");
    assert!(
        err.contains("unrecognized operand") && err.to_ascii_uppercase().contains("STRLITERAL"),
        "{err}"
    );
}

#[test]
fn secondary_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("FRLABEL", reg.create("FRLABEL", vec!["a".into()]).unwrap());
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
    let stage = b.add_stage("FRLABEL", reg.create("FRLABEL", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "FRLABEL should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn nested_word_split_args_are_joined_with_a_blank() {
    // IF/ZONE word-split the remainder; FRLABEL joins those tokens with a blank.
    let (p, s) = run_split("FRLABEL", &["xy", "a c", "z"], &["a", "c"]);
    assert_eq!(p, vec!["a c", "z"]);
    assert_eq!(s, vec!["xy"]);
}
