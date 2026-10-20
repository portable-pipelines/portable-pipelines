//! 64ENCODE / 64DECODE — Author's Edition 1.1.11 pp.613–614.
//!
//! Portable ASCII Base-64 alphabet (book says encoded data are EBCDIC).

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

fn run_bytes(verb: &str, args: &[&str], records: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .expect(verb),
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

const LINE_76: &str =
    "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB";

#[test]
fn encode_rfc_vectors_and_padding() {
    assert_eq!(
        run_spec_collect("literal hello| 64encode"),
        vec!["aGVsbG8="]
    );
    assert_eq!(run_spec_collect("literal Man| 64encode"), vec!["TWFu"]);
    assert_eq!(run_spec_collect("literal a| 64encode"), vec!["YQ=="]);
    assert_eq!(run_spec_collect("literal ab| 64encode"), vec!["YWI="]);
    assert_eq!(run_spec_collect("literal abc| 64encode"), vec!["YWJj"]);
    assert_eq!(
        run_spec_collect("literal pleasure.| 64encode"),
        vec!["cGxlYXN1cmUu"]
    );
}

#[test]
fn decode_one_output_per_input() {
    // p.613: one output record for each input record.
    assert_eq!(
        run_spec_collect("literal aGVsbG8=| 64decode"),
        vec!["hello"]
    );
    assert_eq!(
        run_spec_collect("literal YQ==|literal TWFu| 64decode"),
        vec!["Man", "a"]
    );
}

#[test]
fn decode_length_need_not_be_multiple_of_four() {
    // p.613: “should be a multiple of four … not enforced”.
    assert_eq!(run_spec_collect("literal YQ| 64decode"), vec!["a"]);
    assert_eq!(run_spec_collect("literal YWI| 64decode"), vec!["ab"]);
    assert_eq!(run_spec_collect("literal YWJjZA| 64decode"), vec!["abcd"]);
}

#[test]
fn decode_ignores_non_alphabet() {
    assert_eq!(run_spec_collect("literal YQ== | 64decode"), vec!["a"]);
    assert_eq!(run_spec_collect("literal Y Q = =| 64decode"), vec!["a"]);
    assert_eq!(run_spec_collect("literal !!!!| 64decode"), vec![""]);
}

#[test]
fn encode_wraps_at_seventy_six() {
    let a57 = "A".repeat(57);
    assert_eq!(
        run_spec_collect(&format!("literal {a57}| 64encode")),
        vec![LINE_76]
    );
    let a58 = "A".repeat(58);
    assert_eq!(
        run_spec_collect(&format!("literal {a58}| 64encode")),
        vec![LINE_76.to_string(), "QQ==".to_string()]
    );
}

#[test]
fn encode_is_a_byte_stream_nulls_discarded() {
    // p.614: byte stream; null input records are discarded.
    assert_eq!(
        run_spec_collect("literal lo|literal|literal hel| 64encode"),
        vec!["aGVsbG8="]
    );
    assert_eq!(
        run_spec_collect("literal|literal| 64encode"),
        Vec::<String>::new()
    );
}

#[test]
fn encode_spans_records_then_decode_is_per_line() {
    let a30 = "A".repeat(30);
    let a28 = "A".repeat(28);
    assert_eq!(
        run_spec_collect(&format!("literal {a28}|literal {a30}| 64encode")),
        vec![LINE_76.to_string(), "QQ==".to_string()]
    );
    assert_eq!(
        run_bytes(
            "64DECODE",
            &[],
            vec![LINE_76.as_bytes().to_vec(), b"QQ==".to_vec()]
        ),
        vec![vec![b'A'; 57], vec![b'A']]
    );
}

#[test]
fn round_trip_short_records() {
    for s in ["a", "ab", "abc", "hello", "Man", "sure."] {
        assert_eq!(
            run_spec_collect(&format!("literal {s}| 64encode | 64decode")),
            vec![s.to_string()],
            "{s}"
        );
    }
}

#[test]
fn encode_discards_a_lone_null_record() {
    // p.614: nulls are discarded, so a null file encodes to no records.
    assert_eq!(run_spec_collect("literal| 64encode"), Vec::<String>::new());
    // 64DECODE is per input record, including a null.
    assert_eq!(run_spec_collect("literal| 64decode"), vec![""]);
}

#[test]
fn round_trip_binary() {
    assert_eq!(
        run_bytes("64ENCODE", &[], vec![vec![0x00, 0xff, 0x7f]]),
        vec![b"AP9/".to_vec()]
    );
    assert_eq!(
        run_bytes("64DECODE", &[], vec![b"AP9/".to_vec()]),
        vec![vec![0x00, 0xff, 0x7f]]
    );
}

#[test]
fn joined_encoded_lines_decode_the_byte_stream() {
    // 64DECODE is per record; JOIN * reassembles the RFC line-wrapped stream.
    let a58 = "A".repeat(58);
    assert_eq!(
        run_spec_collect(&format!("literal {a58}| 64encode | join * | 64decode")),
        vec![a58]
    );
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | 64encode 76 | hole",
        "literal a | 64decode PAD | hole",
        "literal a | 64encode MIME | hole",
        "literal a | 64decode * | hole",
    ] {
        let err = err_of(spec);
        assert!(err.contains("unexpected operand"), "{spec}: {err}");
    }
}

#[test]
fn verbs_are_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("64ENCODE"));
    assert!(reg.contains("64DECODE"));
    assert!(!reg.contains("64ENCOD"));
    assert!(!reg.contains("64DEC"));
    let err = err_of("literal a | 64encod | hole");
    assert!(err.contains("unknown stage"), "{err}");
}

#[test]
fn extra_input_is_rejected() {
    for verb in ["64ENCODE", "64DECODE"] {
        let reg = StageRegistry::default();
        let mut b = PipelineBuilder::new();
        let src = b.add_stage("src", emit(&["a"]));
        let extra = b.add_stage("extra", emit(&["b"]));
        let stage = b.add_stage(verb, reg.create(verb, vec![]).unwrap());
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
        assert!(err.to_string().contains("primary"), "{verb}: {err}");
    }
}

#[test]
fn premature_termination_without_output() {
    for verb in ["64ENCODE", "64DECODE"] {
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
        let stage = b.add_stage(verb, reg.create(verb, vec![]).unwrap());
        b.connect_primary(src, stage);
        b.run().unwrap();
        assert!(
            written.get() <= 1,
            "{verb} should stop when no output is connected, wrote {}",
            written.get()
        );
    }
}

#[test]
fn secondary_output_is_unused() {
    for verb in ["64ENCODE", "64DECODE"] {
        let rec = if verb == "64ENCODE" {
            "hello"
        } else {
            "aGVsbG8="
        };
        let expect = if verb == "64ENCODE" {
            "aGVsbG8="
        } else {
            "hello"
        };
        let reg = StageRegistry::default();
        let primary = Rc::new(RefCell::new(Vec::new()));
        let secondary = Rc::new(RefCell::new(Vec::new()));
        let mut b = PipelineBuilder::new();
        let src = b.add_stage("src", emit(&[rec]));
        let stage = b.add_stage(verb, reg.create(verb, vec![]).unwrap());
        let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
        let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
        b.connect_primary(src, stage);
        b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
        b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
        b.run().unwrap();
        assert_eq!(*primary.borrow(), vec![expect.to_string()]);
        assert!(secondary.borrow().is_empty(), "{verb}");
    }
}
