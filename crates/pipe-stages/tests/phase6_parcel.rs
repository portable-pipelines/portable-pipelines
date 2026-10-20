//! PARCEL — Author's Edition 1.1.11 p.452.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

fn emit_bytes(records: &[&[u8]]) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<Vec<u8>> = records.iter().map(|s| s.to_vec()).collect();
    Box::new(move |ctx| {
        for rec in records {
            if ctx.write(Record::from(rec)).is_err() {
                break;
            }
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

fn parcel(primary: &[&[u8]], secondary: &[&[u8]]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit_bytes(primary));
    let src_s = b.add_stage("src_s", emit_bytes(secondary));
    let stage = b.add_stage("PARCEL", reg.create("PARCEL", vec![]).unwrap());
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&out)));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn parcel_err(primary: &[&[u8]], secondary: &[&[u8]]) -> String {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit_bytes(primary));
    let src_s = b.add_stage("src_s", emit_bytes(secondary));
    let stage = b.add_stage("PARCEL", reg.create("PARCEL", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    b.run().unwrap_err().to_string()
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

fn recs(got: &[Vec<u8>]) -> Vec<&[u8]> {
    got.iter().map(|v| v.as_slice()).collect()
}

#[test]
fn parcels_from_one_secondary_record() {
    assert_eq!(
        recs(&parcel(&[b"abcdefghij"], &[b"3 3 4"])),
        vec![b"abc".as_slice(), b"def".as_slice(), b"ghij".as_slice()]
    );
}

#[test]
fn parcels_span_primary_records() {
    assert_eq!(
        recs(&parcel(&[b"ab", b"cdef", b"gh"], &[b"3", b"5"])),
        vec![b"abc".as_slice(), b"defgh".as_slice()]
    );
}

#[test]
fn zero_length_is_a_null_record() {
    assert_eq!(
        recs(&parcel(&[b"ab"], &[b"1 0 1"])),
        vec![b"a".as_slice(), b"".as_slice(), b"b".as_slice()]
    );
}

#[test]
fn exact_input_record_is_one_parcel() {
    assert_eq!(
        recs(&parcel(&[b"hello"], &[b"5"])),
        vec![b"hello".as_slice()]
    );
}

#[test]
fn burst_from_one_primary_record() {
    assert_eq!(
        recs(&parcel(&[b"abcdef"], &[b"1 2 3"])),
        vec![b"a".as_slice(), b"bc".as_slice(), b"def".as_slice()]
    );
}

#[test]
fn secondary_eof_stops_without_error() {
    assert_eq!(
        recs(&parcel(&[b"abcdefghij"], &[b"2 2"])),
        vec![b"ab".as_slice(), b"cd".as_slice()]
    );
}

#[test]
fn primary_eof_before_secondary_is_72e() {
    let err = parcel_err(&[b"ab"], &[b"3"]);
    assert!(
        err.contains("72E")
            && err
                .to_ascii_lowercase()
                .contains("last record not complete"),
        "{err}"
    );
}

#[test]
fn christmas_tree_book_example() {
    // p.452: stars parcelled 1 3 5.
    assert_eq!(
        recs(&parcel(&[b"*********"], &[b"1 3 5"])),
        vec![b"*".as_slice(), b"***".as_slice(), b"*****".as_slice()]
    );
}

#[test]
fn spec_label_form() {
    assert_eq!(
        run_spec_collect(
            "(ENDCHAR ?) literal abcdefghij| p: parcel | >>COLLECT ? literal 3 3 4 | p:"
        ),
        vec!["abc", "def", "ghij"]
    );
}

#[test]
fn extra_operand_rejected() {
    let err = run_spec("literal a | parcel 8 | hole")
        .unwrap_err()
        .to_string();
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("PARCEL"));
    assert!(!reg.contains("PARCE"));
    let err = run_spec("literal a | parce | hole")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("PARCE"),
        "{err}"
    );
}

#[test]
fn secondary_output_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit_bytes(&[b"ab"]));
    let src_s = b.add_stage("src_s", emit_bytes(&[b"1"]));
    let stage = b.add_stage("PARCEL", reg.create("PARCEL", vec![]).unwrap());
    let hole_p = b.add_stage(
        "hp",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    let hole_s = b.add_stage(
        "hs",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(stage, StreamId::PRIMARY, hole_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, hole_s, StreamId::PRIMARY);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .contains("secondary output stream must not be connected"),
        "{err}"
    );
}

#[test]
fn missing_secondary_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(&[b"ab"]));
    let stage = b.add_stage("PARCEL", reg.create("PARCEL", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect_primary(src, stage);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .contains("secondary input stream must be defined and connected"),
        "{err}"
    );
}

#[test]
fn premature_termination_without_output() {
    let reg = StageRegistry::default();
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage(
        "src_p",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["aaaa", "bbbb"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let src_s = b.add_stage("src_s", emit_bytes(&[b"2 2 2"]));
    let stage = b.add_stage("PARCEL", reg.create("PARCEL", vec![]).unwrap());
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "PARCEL should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn non_numeric_secondary_rejected() {
    let err = parcel_err(&[b"ab"], &[b"xyz"]);
    assert!(err.contains("number expected"), "{err}");
}
