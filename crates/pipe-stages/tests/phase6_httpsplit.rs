//! HTTPSPLIT — Author's Edition 1.1.11 p.387.

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

fn split(records: &[&[u8]], args: &[&str]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "HTTPSPLIT",
        reg.create("HTTPSPLIT", args.iter().map(|a| (*a).to_string()).collect())
            .expect("HTTPSPLIT"),
    );
    let sink_p = b.add_stage("p", sink_bytes(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink_bytes(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

fn recs(got: &[Vec<u8>]) -> Vec<&[u8]> {
    got.iter().map(|v| v.as_slice()).collect()
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

fn ascii_to_ebcdic(b: u8) -> u8 {
    match b {
        b'a'..=b'i' => 0x81 + (b - b'a'),
        b'j'..=b'r' => 0x91 + (b - b'j'),
        b's'..=b'z' => 0xA2 + (b - b's'),
        b'A'..=b'I' => 0xC1 + (b - b'A'),
        b'J'..=b'R' => 0xD1 + (b - b'J'),
        b'S'..=b'Z' => 0xE2 + (b - b'S'),
        b'0'..=b'9' => 0xF0 + (b - b'0'),
        b'=' => 0x7E,
        b':' => 0x7A,
        b'-' => 0x60,
        b' ' => 0x40,
        b'/' => 0x61,
        other => other,
    }
}

fn ebcdic_text(s: &str) -> Vec<u8> {
    s.bytes().map(ascii_to_ebcdic).collect()
}

#[test]
fn headers_then_body_without_content_length() {
    let (h, b) = split(&[b"GET / HTTP/1.0\r\nHost: x\r\n\r\nhello"], &[]);
    assert_eq!(
        recs(&h),
        vec![
            b"GET / HTTP/1.0".as_slice(),
            b"Host: x".as_slice(),
            b"".as_slice()
        ]
    );
    assert_eq!(recs(&b), vec![b"hello".as_slice()]);
}

#[test]
fn leading_crlf_discarded() {
    let (h, b) = split(&[b"\r\nGET / HTTP/1.0\r\n\r\nbody"], &[]);
    assert_eq!(recs(&h), vec![b"GET / HTTP/1.0".as_slice(), b"".as_slice()]);
    assert_eq!(recs(&b), vec![b"body".as_slice()]);
}

#[test]
fn crlf_spans_input_records() {
    let (h, b) = split(&[b"GET / HTTP/1.0\r", b"\n\r\nbody"], &[]);
    assert_eq!(recs(&h), vec![b"GET / HTTP/1.0".as_slice(), b"".as_slice()]);
    assert_eq!(recs(&b), vec![b"body".as_slice()]);
}

#[test]
fn body_is_raw_records_without_content_length() {
    let (h, b) = split(&[b"GET / HTTP/1.0\r\n\r\npart1", b"part2"], &[]);
    assert_eq!(recs(&h), vec![b"GET / HTTP/1.0".as_slice(), b"".as_slice()]);
    assert_eq!(recs(&b), vec![b"part1".as_slice(), b"part2".as_slice()]);
}

#[test]
fn content_length_colon_takes_that_many_bytes() {
    let (h, b) = split(
        &[b"HTTP/1.0 200 OK\r\nContent-Length: 4\r\n\r\nABCDREST"],
        &[],
    );
    assert_eq!(
        recs(&h),
        vec![
            b"HTTP/1.0 200 OK".as_slice(),
            b"Content-Length: 4".as_slice(),
            b"".as_slice(),
            b"REST".as_slice()
        ]
    );
    assert_eq!(recs(&b), vec![b"ABCD".as_slice()]);
}

#[test]
fn content_length_equals_form() {
    let (h, b) = split(
        &[b"HTTP/1.0 200 OK\r\ncontent-length=3\r\n\r\nXYZmore"],
        &[],
    );
    assert_eq!(recs(&b), vec![b"XYZ".as_slice()]);
    assert_eq!(recs(&h).last().copied().unwrap_or(b""), b"more".as_slice());
}

#[test]
fn content_length_zero_returns_to_header_mode() {
    let (h, b) = split(
        &[b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\nGET / HTTP/1.0\r\n\r\n"],
        &[],
    );
    assert!(b.is_empty(), "{b:?}");
    assert_eq!(
        recs(&h),
        vec![
            b"HTTP/1.0 200 OK".as_slice(),
            b"Content-Length: 0".as_slice(),
            b"".as_slice(),
            b"GET / HTTP/1.0".as_slice(),
            b"".as_slice()
        ]
    );
}

#[test]
fn content_length_spans_body_records() {
    let (_h, b) = split(
        &[b"HTTP/1.0 200 OK\r\nContent-Length: 5\r\n\r\nAB", b"CDEXX"],
        &[],
    );
    assert_eq!(recs(&b), vec![b"AB".as_slice(), b"CDE".as_slice()]);
}

#[test]
fn ebcdic_crlf_and_content_length() {
    let mut rec = ebcdic_text("GET / HTTP/1.0");
    rec.extend_from_slice(b"\x0d\x25");
    rec.extend(ebcdic_text("content-length=2"));
    rec.extend_from_slice(b"\x0d\x15");
    rec.extend_from_slice(b"\x0d\x25");
    rec.extend_from_slice(b"XYZZ");
    let (h, b) = split(&[&rec], &["EBCDIC"]);
    assert_eq!(h.len(), 4, "{h:?}");
    assert_eq!(h[2], b"");
    assert_eq!(recs(&b), vec![b"XY".as_slice()]);
    assert_eq!(h[3], b"ZZ");
}

#[test]
fn invented_operands_rejected() {
    let err = err_of("literal a | httpsplit ASCII | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("HTTPSPLIT"));
    assert!(!reg.contains("HTTPSPLI"));
    let err = err_of("literal a | httpspli | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("HTTPSPLI"),
        "{err}"
    );
}

#[test]
fn extra_input_rejected() {
    let reg = StageRegistry::default();
    let mut bld = PipelineBuilder::new();
    let src = bld.add_stage("src", emit_bytes(&[b"GET / HTTP/1.0\r\n\r\n"]));
    let extra = bld.add_stage("extra", emit_bytes(&[b"x"]));
    let stage = bld.add_stage("HTTPSPLIT", reg.create("HTTPSPLIT", vec![]).unwrap());
    let hole = bld.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    bld.connect_primary(src, stage);
    bld.connect(extra, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    bld.connect_primary(stage, hole);
    let err = bld.run().unwrap_err();
    assert!(
        err.to_string()
            .contains("no input stream other than the primary"),
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
            for rec in ["GET / HTTP/1.0\r\n\r\n", "aaaa"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("HTTPSPLIT", reg.create("HTTPSPLIT", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "HTTPSPLIT should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn spec_form_with_change() {
    let collected_p = Rc::new(RefCell::new(Vec::new()));
    let collected_s = Rc::new(RefCell::new(Vec::new()));
    let mut reg = StageRegistry::default();
    reg.register(">>P", {
        let o = Rc::clone(&collected_p);
        move |_| pipe_stages::collect_to(Rc::clone(&o))
    });
    reg.register(">>S", {
        let o = Rc::clone(&collected_s);
        move |_| pipe_stages::collect_to(Rc::clone(&o))
    });
    let spec = "(ENDCHAR ?) literal body|literal CRLHST xCRLCRL|literal GET / HTTP/1.0| change /CRL/ x0d0a | change /HST/ /Host:/ | h: httpsplit | >>P ? h: | >>S";
    let parsed = pipe_parser::parse_pipeline(spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();
    assert_eq!(
        *collected_p.borrow(),
        vec![
            "GET / HTTP/1.0".to_string(),
            "Host: x".to_string(),
            String::new()
        ]
    );
    assert_eq!(*collected_s.borrow(), vec!["body".to_string()]);
}
