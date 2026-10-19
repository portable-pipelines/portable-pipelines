//! PAD — Author's Edition 1.1.11 p.451.
//!
//! Portable ASCII: xorc `BLANK`/`SPACE` is X'20' (book cites EBCDIC X'40');
//! `TABULATE` is ASCII HT X'09' (book cites EBCDIC X'05'). Hex/binary xorc
//! bytes are raw; the book's X'40' blank is not an ASCII space.

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

fn run_primary(records: &[&str], args: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "PAD",
        reg.create("PAD", args.iter().map(|a| (*a).to_string()).collect())
            .expect("PAD"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn run_primary_bytes(records: Vec<Vec<u8>>, args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "PAD",
        reg.create("PAD", args.iter().map(|a| (*a).to_string()).collect())
            .expect("PAD"),
    );
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn book_example_fixed_format_pad_then_chop() {
    // p.451: `pipe console | pad 80 | chop 80 | > input file a fixed`
    // Portable ASCII stand-in (no console / CMS file mode).
    let out = run_spec_collect("literal hi| pad 80 | chop 80");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].len(), 80);
    assert_eq!(&out[0][..2], "hi");
    assert!(out[0][2..].chars().all(|c| c == ' '), "{:?}", out[0]);
}

#[test]
fn default_right_blank() {
    // Default: pad on the right with blank.
    assert_eq!(run_primary(&["hi"], &["5"]), vec!["hi   "]);
    assert_eq!(run_spec_collect("literal hi| pad 5"), vec!["hi   "]);
}

#[test]
fn explicit_right_and_blank() {
    assert_eq!(run_primary(&["hi"], &["right", "5"]), vec!["hi   "]);
    assert_eq!(run_primary(&["hi"], &["RIGHT", "5", "BLANK"]), vec!["hi   "]);
    assert_eq!(run_spec_collect("literal hi| pad right 5 blank"), vec!["hi   "]);
}

#[test]
fn left_right_aligns_the_text() {
    // p.451: LEFT pads on the left-hand side (text is right-aligned).
    assert_eq!(run_primary(&["hi"], &["left", "5"]), vec!["   hi"]);
    assert_eq!(run_primary(&["hi"], &["LEFT", "5", "*"]), vec!["***hi"]);
    assert_eq!(run_spec_collect("literal hi| pad left 5"), vec!["   hi"]);
}

#[test]
fn side_abbreviations_min_1() {
    // Right / Left mixed-case prefixes (p.451).
    assert_eq!(run_primary(&["hi"], &["l", "5"]), vec!["   hi"]);
    assert_eq!(run_primary(&["hi"], &["le", "5"]), vec!["   hi"]);
    assert_eq!(run_primary(&["hi"], &["lef", "5"]), vec!["   hi"]);
    assert_eq!(run_primary(&["hi"], &["r", "5"]), vec!["hi   "]);
    assert_eq!(run_primary(&["hi"], &["ri", "5"]), vec!["hi   "]);
    assert_eq!(run_primary(&["hi"], &["righ", "5"]), vec!["hi   "]);
}

#[test]
fn longer_and_equal_records_are_unmodified() {
    assert_eq!(run_primary(&["hello"], &["5"]), vec!["hello"]);
    assert_eq!(run_primary(&["hello!"], &["5"]), vec!["hello!"]);
    assert_eq!(run_primary(&["abcde"], &["5"]), vec!["abcde"]);
}

#[test]
fn pad_zero_is_a_no_op() {
    // Minimum length is nonnegative; 0 never expands a record.
    assert_eq!(run_primary(&["", "a", "ab"], &["0"]), vec!["", "a", "ab"]);
}

#[test]
fn null_record_is_padded() {
    assert_eq!(run_primary(&[""], &["3"]), vec!["   "]);
    assert_eq!(run_primary(&[""], &["left", "3", "*"]), vec!["***"]);
}

#[test]
fn space_xorc_synonym() {
    // Shared xorc helper (p.197): SPACE ≡ BLANK. Portable ASCII X'20'.
    assert_eq!(run_primary(&["hi"], &["5", "SPACE"]), vec!["hi   "]);
}

#[test]
fn single_character_xorc() {
    assert_eq!(run_primary(&["hi"], &["5", "*"]), vec!["hi***"]);
    assert_eq!(run_primary(&["hi"], &["left", "4", "0"]), vec!["00hi"]);
}

#[test]
fn two_hex_xorc_is_raw_byte() {
    // p.451 xorc: two-character hex code. ASCII '*' is X'2A' (not EBCDIC X'5C').
    assert_eq!(run_primary(&["hi"], &["5", "2A"]), vec!["hi***"]);
    assert_eq!(
        run_primary_bytes(vec![b"ab".to_vec()], &["4", "00"]),
        vec![vec![b'a', b'b', 0, 0]]
    );
}

#[test]
fn tabulate_xorc() {
    // Shared xorc: TABULATE min 3 is ASCII HT (book cites EBCDIC X'05').
    assert_eq!(
        run_primary_bytes(vec![b"ab".to_vec()], &["4", "TAB"]),
        vec![vec![b'a', b'b', b'\t', b'\t']]
    );
}

#[test]
fn socka2ip_style_hex_null_pad() {
    // p.519: `… | pad 16 00 | socka2ip | …` — pad with X'00' to length 16.
    let rec = vec![0u8, 2, 0, 17, 1, 2, 3, 4];
    let mut want = rec.clone();
    want.resize(16, 0);
    assert_eq!(run_primary_bytes(vec![rec], &["16", "00"]), vec![want]);
}

#[test]
fn modulo_without_offset() {
    // Default offset 0: pad to the next multiple of the modulo.
    assert_eq!(run_primary(&["a"], &["modulo", "4"]), vec!["a   "]);
    assert_eq!(run_primary(&["ab"], &["modulo", "4"]), vec!["ab  "]);
    assert_eq!(run_primary(&["abc"], &["modulo", "4"]), vec!["abc "]);
    assert_eq!(run_primary(&["abcd"], &["modulo", "4"]), vec!["abcd"]);
    assert_eq!(run_primary(&["abcde"], &["modulo", "4"]), vec!["abcde   "]);
    assert_eq!(run_spec_collect("literal abcde| pad modulo 4"), vec!["abcde   "]);
}

#[test]
fn modulo_with_offset() {
    // Output length is the smallest k*modulo+offset ≥ input length.
    // offset 1, modulo 4 → 1, 5, 9, …
    assert_eq!(run_primary(&[""], &["1", "modulo", "4"]), vec![" "]);
    assert_eq!(run_primary(&["a"], &["1", "modulo", "4"]), vec!["a"]);
    assert_eq!(run_primary(&["ab"], &["1", "modulo", "4"]), vec!["ab   "]);
    assert_eq!(run_primary(&["abcde"], &["1", "modulo", "4"]), vec!["abcde"]);
    assert_eq!(run_primary(&["abcdef"], &["1", "modulo", "4"]), vec!["abcdef   "]);
    assert_eq!(run_spec_collect("literal ab| pad 1 modulo 4"), vec!["ab   "]);
}

#[test]
fn left_modulo_and_xorc() {
    assert_eq!(
        run_primary(&["ab"], &["left", "1", "modulo", "4", "X"]),
        vec!["XXXab"]
    );
}

#[test]
fn explicit_zero_offset_modulo() {
    assert_eq!(
        run_primary(&["abc"], &["0", "modulo", "4"]),
        vec!["abc "]
    );
}

#[test]
fn modulo_one_never_pads() {
    // Every length is already 0 + k*1.
    assert_eq!(
        run_primary(&["", "a", "ab"], &["modulo", "1"]),
        vec!["", "a", "ab"]
    );
}

#[test]
fn each_record_is_padded_independently() {
    assert_eq!(
        run_primary(&["a", "abcd", "ab"], &["4"]),
        vec!["a   ", "abcd", "ab  "]
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert!(run_primary(&[], &["80"]).is_empty());
}

#[test]
fn missing_number_rejected() {
    let err = err_of("literal a | pad | hole");
    assert!(err.contains("number is required"), "{err}");
    let err = err_of("literal a | pad left | hole");
    assert!(err.contains("number is required"), "{err}");
    let err = err_of("literal a | pad modulo | hole");
    assert!(err.contains("MODULO requires a number"), "{err}");
    let err = err_of("literal a | pad 1 modulo | hole");
    assert!(err.contains("MODULO requires a number"), "{err}");
}

#[test]
fn first_number_must_be_smaller_than_second() {
    let err = err_of("literal a | pad 4 modulo 4 | hole");
    assert!(err.contains("smaller"), "{err}");
    let err = err_of("literal a | pad 5 modulo 4 | hole");
    assert!(err.contains("smaller"), "{err}");
}

#[test]
fn modulo_number_must_be_positive() {
    let err = err_of("literal a | pad modulo 0 | hole");
    assert!(err.contains("positive"), "{err}");
    let err = err_of("literal a | pad 0 modulo 0 | hole");
    assert!(err.contains("positive"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | pad first 80 | hole",
        "literal a | pad both 80 | hole",
        "literal a | pad center 80 | hole",
        "literal a | pad leading 80 | hole",
        "literal a | pad nopad 80 | hole",
        "literal a | pad * 80 | hole",
        "literal a | pad 80 first | hole",
        "literal a | pad 80 string | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unexpected") || err.contains("invalid xorc"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn wrong_order_rejected() {
    let err = err_of("literal a | pad 80 left | hole");
    assert!(
        err.contains("LEFT") && err.contains("before the number"),
        "{err}"
    );
    let err = err_of("literal a | pad 80 right | hole");
    assert!(
        err.contains("RIGHT") && err.contains("before the number"),
        "{err}"
    );
}

#[test]
fn modulo_is_a_whole_word() {
    // MODULO is all-caps in the railroad (p.451); `mod` is not an abbreviation.
    let err = err_of("literal a | pad mod 4 | hole");
    assert!(
        err.contains("unexpected") || err.contains("invalid xorc"),
        "{err}"
    );
    let err = err_of("literal a | pad 1 mod 4 | hole");
    assert!(err.contains("invalid xorc") || err.contains("unexpected"), "{err}");
}

#[test]
fn blank_is_a_whole_word() {
    let err = err_of("literal a | pad 5 blan | hole");
    assert!(err.contains("invalid xorc"), "{err}");
}

#[test]
fn extra_operand_rejected() {
    let err = err_of("literal a | pad 5 blank * | hole");
    assert!(err.contains("unexpected"), "{err}");
    let err = err_of("literal a | pad 5 * extra | hole");
    assert!(err.contains("unexpected"), "{err}");
}

#[test]
fn signed_and_plus_numbers_rejected() {
    let err = err_of("literal a | pad -1 | hole");
    assert!(err.contains("unexpected"), "{err}");
    let err = err_of("literal a | pad +80 | hole");
    assert!(err.contains("unexpected"), "{err}");
}

#[test]
fn two_hex_after_number_is_xorc_not_a_second_length() {
    // `pad 8 10` is min-length 8 with xorc X'10', not offset 8 modulo 10.
    assert_eq!(
        run_primary_bytes(vec![b"ab".to_vec()], &["8", "10"]),
        vec![vec![b'a', b'b', 0x10, 0x10, 0x10, 0x10, 0x10, 0x10]]
    );
}

#[test]
fn r_after_number_is_xorc_not_right() {
    assert_eq!(run_primary(&["hi"], &["4", "R"]), vec!["hiRR"]);
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("PAD"));
    assert!(!reg.contains("PA"));
    assert!(!reg.contains("P"));
    let err = err_of("literal a | pa 5 | hole");
    assert!(
        err.contains("unknown stage") && err.contains("PA"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("PAD", reg.create("PAD", vec!["5".into()]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(|ctx: &mut StageCtx| {
            while ctx.read().ok().flatten().is_some() {}
        }),
    );
    b.connect_primary(src, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("primary"), "{err}");
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
    let stage = b.add_stage("PAD", reg.create("PAD", vec!["5".into()]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "PAD should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn secondary_output_is_unused() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["hi"]));
    let stage = b.add_stage("PAD", reg.create("PAD", vec!["4".into()]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["hi  ".to_string()]);
    assert!(secondary.borrow().is_empty());
}
