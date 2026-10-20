//! DIGEST — Author's Edition 1.1.11 pp.326–327.
//!
//! SHA-1 (RFC 3174), SHA-256/384/512 (FIPS 180-2), MD5 (RFC 1321).
//! Records are concatenated with no inserted separators. APPEND stores
//! `record || digest` (hash-internal padding is not written).

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

fn run_digest(args: &[&str], records: &[&str]) -> Vec<Vec<u8>> {
    run_digest_bytes(
        args,
        records.iter().map(|s| s.as_bytes().to_vec()).collect(),
    )
}

fn run_digest_bytes(args: &[&str], records: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "DIGEST",
        reg.create("DIGEST", args.iter().map(|a| (*a).to_string()).collect())
            .expect("DIGEST"),
    );
    let snk = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

const SHA1_ABC: &str = "A9993E364706816ABA3E25717850C26C9CD0D89D";
const SHA1_EMPTY: &str = "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709";
const SHA1_ABC_LONG: &str = "84983E441C3BD26EBAAE4AA1F95129E5E54670F1";
const SHA256_ABC: &str = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
const SHA256_EMPTY: &str = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
const SHA384_ABC: &str = "CB00753F45A35E8BB5A03D699AC65007272C32AB0EDED1631A8B605A43FF5BED8086072BA1E7CC2358BAECA134C825A7";
const SHA512_ABC: &str = "DDAF35A193617ABACC417349AE20413112E6FA4E89A97EA20A9EEEE64B55D39A2192992A274FC1A836BA3C23A3FEEBBD454D4423643CE80E2A9AC94FA54CA49F";
const MD5_EMPTY: &str = "D41D8CD98F00B204E9800998ECF8427E";
const MD5_A: &str = "0CC175B9C0F1B6A831C399E269772661";
const MD5_ABC: &str = "900150983CD24FB0D6963F7D28E17F72";
const MD5_MESSAGE_DIGEST: &str = "F96B697D7CB7938D525A2F31AAF161D0";

#[test]
fn rfc3174_sha1() {
    assert_eq!(hex(&run_digest(&["SHA1"], &["abc"])[0]), SHA1_ABC);
    assert_eq!(hex(&run_digest(&["sha1"], &[""])[0]), SHA1_EMPTY);
    // RFC 3174 / FIPS two-block message
    assert_eq!(
        hex(&run_digest(
            &["SHA1"],
            &["abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"]
        )[0]),
        SHA1_ABC_LONG
    );
}

#[test]
fn fips180_sha2() {
    assert_eq!(hex(&run_digest(&["SHA256"], &["abc"])[0]), SHA256_ABC);
    assert_eq!(hex(&run_digest(&["SHA256"], &[""])[0]), SHA256_EMPTY);
    assert_eq!(hex(&run_digest(&["SHA384"], &["abc"])[0]), SHA384_ABC);
    assert_eq!(hex(&run_digest(&["SHA512"], &["abc"])[0]), SHA512_ABC);
}

#[test]
fn rfc1321_md5() {
    assert_eq!(hex(&run_digest(&["MD5"], &[""])[0]), MD5_EMPTY);
    assert_eq!(hex(&run_digest(&["MD5"], &["a"])[0]), MD5_A);
    assert_eq!(hex(&run_digest(&["MD5"], &["abc"])[0]), MD5_ABC);
    assert_eq!(
        hex(&run_digest(&["MD5"], &["message digest"])[0]),
        MD5_MESSAGE_DIGEST
    );
}

#[test]
fn concatenates_records_without_separators() {
    // literal cascade is reverse: `literal c|literal ab` yields ab then c.
    assert_eq!(
        run_spec_collect("literal c|literal ab| digest sha1 | spec 1-* c2x"),
        vec![SHA1_ABC]
    );
    assert_eq!(hex(&run_digest(&["SHA1"], &["ab", "c"])[0]), SHA1_ABC);
}

#[test]
fn empty_input_is_empty_message() {
    assert_eq!(
        run_spec_collect("digest sha1 | spec 1-* c2x"),
        vec![SHA1_EMPTY]
    );
}

#[test]
fn spec_form_each_algorithm() {
    assert_eq!(
        run_spec_collect("literal abc| digest sha1 | spec 1-* c2x"),
        vec![SHA1_ABC]
    );
    assert_eq!(
        run_spec_collect("literal abc| digest sha256 | spec 1-* c2x"),
        vec![SHA256_ABC]
    );
    assert_eq!(
        run_spec_collect("literal abc| digest sha384 | spec 1-* c2x"),
        vec![SHA384_ABC]
    );
    assert_eq!(
        run_spec_collect("literal abc| digest sha512 | spec 1-* c2x"),
        vec![SHA512_ABC]
    );
    assert_eq!(
        run_spec_collect("literal abc| digest md5 | spec 1-* c2x"),
        vec![MD5_ABC]
    );
}

#[test]
fn append_then_verify_restores_record() {
    assert_eq!(
        run_spec_collect("literal abc| digest sha1 append | digest sha1 verify"),
        vec!["abc"]
    );
    assert_eq!(
        run_spec_collect("literal hello| digest md5 append | digest md5 verify"),
        vec!["hello"]
    );
}

#[test]
fn append_writes_record_plus_digest() {
    let out = run_digest(&["SHA1", "APPEND"], &["abc"]);
    assert_eq!(out.len(), 1);
    assert_eq!(&out[0][..3], b"abc");
    assert_eq!(hex(&out[0][3..]), SHA1_ABC);
}

#[test]
fn verify_rejects_bad_or_short() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut good = b"abc".to_vec();
    // SHA1("abc") bytes
    let digest = run_digest(&["SHA1"], &["abc"]).into_iter().next().unwrap();
    good.extend_from_slice(&digest);
    let mut bad = good.clone();
    *bad.last_mut().unwrap() ^= 1;
    let short = b"abc".to_vec();

    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(vec![good, bad.clone(), short.clone()]));
    let stage = b.add_stage(
        "DIGEST",
        reg.create("DIGEST", vec!["SHA1".into(), "VERIFY".into()])
            .unwrap(),
    );
    let sink_p = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink_bytes(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec![b"abc".to_vec()]);
    assert_eq!(*secondary.borrow(), vec![bad, short]);
}

#[test]
fn append_rejects_secondary_streams() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "DIGEST",
        reg.create("DIGEST", vec!["SHA1".into(), "APPEND".into()])
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
    assert!(
        err.to_string()
            .contains("secondary streams must not be defined"),
        "{err}"
    );
}

#[test]
fn verify_rejects_secondary_input() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage(
        "DIGEST",
        reg.create("DIGEST", vec!["SHA1".into(), "VERIFY".into()])
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
    assert!(
        err.to_string()
            .contains("secondary input stream must not be connected"),
        "{err}"
    );
}

#[test]
fn batch_secondary_copies_and_signs() {
    // PI → SO; digest → PO at SI trigger and at PI EOF. Prefer-primary
    // plus one data record keeps the copy deterministic; the SHA1("abc")
    // digest is written either on the trigger or at EOF.
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc"]));
    let trig = b.add_stage("trig", emit(&["go"]));
    let stage = b.add_stage("DIGEST", reg.create("DIGEST", vec!["SHA1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(trig, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let digests = primary.borrow().clone();
    assert_eq!(*secondary.borrow(), vec!["abc".to_string()]);
    assert!(!digests.is_empty());
    for d in &digests {
        assert_eq!(d.len(), 20);
    }
    assert!(
        digests.iter().any(|d| hex(d) == SHA1_ABC),
        "expected SHA1(abc) among {:?}",
        digests.iter().map(|d| hex(d)).collect::<Vec<_>>()
    );
}

#[test]
fn batch_eof_digest_of_whole_file() {
    // Secondary output only (no SI): copy through and write digest at EOF.
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["abc"]));
    let stage = b.add_stage("DIGEST", reg.create("DIGEST", vec!["SHA1".into()]).unwrap());
    let sink_p = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(hex(&primary.borrow()[0]), SHA1_ABC);
    assert_eq!(*secondary.borrow(), vec!["abc".to_string()]);
}

#[test]
fn algorithm_required() {
    let err = err_of("literal a | digest | hole");
    assert!(err.contains("algorithm"), "{err}");
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | digest sha1 extra | hole",
        "literal a | digest sha | hole",
        "literal a | digest sha-1 | hole",
        "literal a | digest append sha1 | hole",
        "literal a | digest sha1 sign | hole",
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("unrecognized") || err.contains("unexpected") || err.contains("algorithm"),
            "{spec}: {err}"
        );
    }
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("DIGEST"));
    assert!(!reg.contains("DIGES"));
    assert!(!reg.contains("DIG"));
    let err = err_of("literal a | diges sha1 | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("DIGES"),
        "{err}"
    );
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
            for rec in ["one", "two", "three"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "DIGEST",
        reg.create("DIGEST", vec!["SHA1".into(), "APPEND".into()])
            .unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "APPEND should stop when no output is connected, wrote {}",
        written.get()
    );
}
