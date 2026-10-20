//! QPENCODE / QPDECODE — Author's Edition 1.1.11 pp.466–467.
//!
//! Portable ASCII quoted-printable (book operates in the ASCII domain).

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

fn run_qpdecode_two_stream(records: Vec<Vec<u8>>) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage("QPDECODE", reg.create("QPDECODE", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink_bytes(Rc::clone(&primary)));
    let sink_s = b.add_stage("sink_s", sink_bytes(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let prim = primary.borrow().clone();
    let sec = secondary.borrow().clone();
    (prim, sec)
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn encode_literals_equals_and_null() {
    assert_eq!(run_spec_collect("literal abc| qpencode"), vec!["abc"]);
    assert_eq!(run_spec_collect("literal a=b| qpencode"), vec!["a=3Db"]);
    assert_eq!(run_spec_collect("literal| qpencode"), vec![""]);
}

#[test]
fn encode_trailing_whitespace() {
    assert_eq!(run_spec_collect("literal a b| qpencode"), vec!["a b"]);
    assert_eq!(
        run_bytes("QPENCODE", &[], vec![b"a ".to_vec()]),
        vec![b"a=20".to_vec()]
    );
    assert_eq!(
        run_bytes("QPENCODE", &[], vec![b"a\t".to_vec()]),
        vec![b"a=09".to_vec()]
    );
    assert_eq!(
        run_bytes("QPENCODE", &[], vec![b"a b ".to_vec()]),
        vec![b"a b=20".to_vec()]
    );
}

#[test]
fn encode_ibm_extra_ascii() {
    // p.467 additional escapes (ASCII hex of non-invariant graphics).
    assert_eq!(run_spec_collect("literal !| qpencode"), vec!["=21"]);
    assert_eq!(run_spec_collect("literal @| qpencode"), vec!["=40"]);
    assert_eq!(run_spec_collect("literal _| qpencode"), vec!["_"]);
    assert_eq!(run_spec_collect("literal %az| qpencode"), vec!["%az"]);
    assert_eq!(
        run_bytes("QPENCODE", &[], vec![b"[\\]^`{|}~".to_vec()]),
        vec![b"=5B=5C=5D=5E=60=7B=7C=7D=7E".to_vec()]
    );
}

#[test]
fn encode_wraps_at_seventy_six_with_soft_break() {
    let a76 = "a".repeat(76);
    assert_eq!(
        run_spec_collect(&format!("literal {a76}| qpencode")),
        vec![a76]
    );
    let a77 = "a".repeat(77);
    assert_eq!(
        run_spec_collect(&format!("literal {a77}| qpencode")),
        vec![format!("{}=", "a".repeat(75)), "aa".to_string()]
    );
}

#[test]
fn decode_splices_soft_break() {
    assert_eq!(
        run_spec_collect("literal aa|literal aaaaaaaa=| qpdecode"),
        vec!["aaaaaaaaaa"]
    );
    let a77 = "a".repeat(77);
    assert_eq!(
        run_spec_collect(&format!("literal {a77}| qpencode | qpdecode")),
        vec![a77]
    );
}

#[test]
fn decode_hex_case_and_passthrough() {
    assert_eq!(run_spec_collect("literal a=3db| qpdecode"), vec!["a=b"]);
    assert_eq!(run_spec_collect("literal a=3Db| qpdecode"), vec!["a=b"]);
    // Note 3: non-escape sequences are not validated.
    assert_eq!(run_spec_collect("literal a b| qpdecode"), vec!["a b"]);
    assert_eq!(
        run_bytes("QPDECODE", &[], vec![vec![0x01, b'A']]),
        vec![vec![0x01, b'A']]
    );
}

#[test]
fn decode_invalid_without_secondary_fails() {
    let err = err_of("literal a=GG | qpdecode | hole");
    assert!(err.contains("invalid escape"), "{err}");
    let err = err_of("literal a=3 | qpdecode | hole");
    assert!(err.contains("invalid escape"), "{err}");
    let err = err_of("literal a=z | qpdecode | hole");
    assert!(err.contains("invalid escape"), "{err}");
}

#[test]
fn decode_invalid_with_secondary_continues() {
    // p.466: splice buffer → primary; entire bad record → secondary.
    let (prim, sec) =
        run_qpdecode_two_stream(vec![b"AB=".to_vec(), b"xx=GG".to_vec(), b"ok".to_vec()]);
    assert_eq!(prim, vec![b"AB".to_vec(), b"ok".to_vec()]);
    assert_eq!(sec, vec![b"xx=GG".to_vec()]);
}

#[test]
fn decode_eof_flushes_soft_break() {
    assert_eq!(run_spec_collect("literal hello=| qpdecode"), vec!["hello"]);
}

#[test]
fn encode_decode_round_trip_forms() {
    for s in ["", "abc", "a=b", "hello world", "!@#", "%&*", "a_b"] {
        let spec = if s.is_empty() {
            "literal| qpencode | qpdecode".to_string()
        } else {
            format!("literal {s}| qpencode | qpdecode")
        };
        assert_eq!(run_spec_collect(&spec), vec![s.to_string()], "{s}");
    }
    let long = "a".repeat(200);
    assert_eq!(
        run_spec_collect(&format!("literal {long}| qpencode | qpdecode")),
        vec![long]
    );
    assert_eq!(
        run_bytes(
            "QPDECODE",
            &[],
            run_bytes("QPENCODE", &[], vec![b"a b ".to_vec()])
        ),
        vec![b"a b ".to_vec()]
    );
    assert_eq!(
        run_bytes(
            "QPDECODE",
            &[],
            run_bytes("QPENCODE", &[], vec![vec![0x00, 0xff, b'=']])
        ),
        vec![vec![0x00, 0xff, b'=']]
    );
}

#[test]
fn wrap_does_not_split_equals_triplet() {
    let mut rec = vec![b'a'; 74];
    rec.push(b'=');
    let encoded = run_bytes("QPENCODE", &[], vec![rec.clone()]);
    assert!(encoded.len() >= 2, "{encoded:?}");
    assert_eq!(encoded.last().unwrap(), b"=3D");
    assert!(encoded[0].ends_with(b"="));
    assert_eq!(run_bytes("QPDECODE", &[], encoded), vec![rec]);
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | qpencode 76 | hole",
        "literal a | qpdecode PAD | hole",
        "literal a | qpencode MIME | hole",
        "literal a | qpdecode * | hole",
        "literal a | qpencode BINARY | hole",
    ] {
        let err = err_of(spec);
        assert!(err.contains("unexpected operand"), "{spec}: {err}");
    }
}

#[test]
fn verbs_are_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("QPENCODE"));
    assert!(reg.contains("QPDECODE"));
    assert!(!reg.contains("QPENCOD"));
    assert!(!reg.contains("QPDEC"));
    let err = err_of("literal a | qpencod | hole");
    assert!(err.contains("unknown stage"), "{err}");
}

#[test]
fn extra_input_is_rejected() {
    for verb in ["QPENCODE", "QPDECODE"] {
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
    for verb in ["QPENCODE", "QPDECODE"] {
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
fn qpencode_secondary_output_is_unused() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc"]));
    let stage = b.add_stage("QPENCODE", reg.create("QPENCODE", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["abc".to_string()]);
    assert!(secondary.borrow().is_empty());
}
