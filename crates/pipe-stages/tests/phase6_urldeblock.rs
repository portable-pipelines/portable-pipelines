//! URLDEBLOCK — Author's Edition 1.1.11 pp.577–578.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId, TraceConfig};
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

fn urldeblock(records: &[&[u8]], args: &[&str]) -> Vec<Vec<u8>> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(records));
    let stage = b.add_stage(
        "URLDEBLOCK",
        reg.create(
            "URLDEBLOCK",
            args.iter().map(|a| (*a).to_string()).collect(),
        )
        .expect("URLDEBLOCK"),
    );
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn recs(got: &[Vec<u8>]) -> Vec<&[u8]> {
    got.iter().map(|v| v.as_slice()).collect()
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
        b'%' => 0x6C,
        b'+' => 0x4E,
        b'&' => 0x50,
        b';' => 0x5E,
        b' ' => 0x40,
        b'/' => 0x61,
        other => other,
    }
}

fn ebcdic_text(s: &str) -> Vec<u8> {
    s.bytes().map(ascii_to_ebcdic).collect()
}

#[test]
fn book_sample_url_after_join() {
    // p.578 SAMPLE URL, after join (two physical lines concatenated).
    let joined =
        b"name=Craig%20R.%20Doe&class=90&grad=&email=xyz@prodigy.com&bdate=03%2F15%2F68&url=";
    assert_eq!(
        recs(&urldeblock(&[joined], &[])),
        vec![
            b"name=Craig R. Doe".as_slice(),
            b"class=90".as_slice(),
            b"grad=".as_slice(),
            b"email=xyz@prodigy.com".as_slice(),
            b"bdate=03/15/68".as_slice(),
            b"url=".as_slice(),
        ]
    );
}

#[test]
fn book_sample_via_join_spec() {
    // LITERAL cascade is reverse order, so the first SAMPLE line is last.
    assert_eq!(
        run_spec_collect(
            "literal bdate=03%2F15%2F68&url=|literal name=Craig%20R.%20Doe&class=90&grad=&email=xyz@prodigy.com&| join | urldeblock"
        ),
        vec![
            "name=Craig R. Doe",
            "class=90",
            "grad=",
            "email=xyz@prodigy.com",
            "bdate=03/15/68",
            "url=",
        ]
    );
}

#[test]
fn ampersand_and_semicolon_split() {
    assert_eq!(
        recs(&urldeblock(&[b"a=1&b=2;c=3"], &[])),
        vec![b"a=1".as_slice(), b"b=2".as_slice(), b"c=3".as_slice()]
    );
}

#[test]
fn plus_becomes_blank() {
    assert_eq!(
        recs(&urldeblock(&[b"a+b+c"], &[])),
        vec![b"a b c".as_slice()]
    );
}

#[test]
fn percent_hex_is_raw_byte() {
    // `%2b` becomes ASCII plus, not a blank (the blank substitution is only
    // for a raw `+`).
    assert_eq!(
        recs(&urldeblock(&[b"%2b%20%2B"], &[])),
        vec![b"+ +".as_slice()]
    );
}

#[test]
fn encoded_separator_does_not_split() {
    assert_eq!(
        recs(&urldeblock(&[b"a=%26b%3bc&x=1"], &[])),
        vec![b"a=&b;c".as_slice(), b"x=1".as_slice()]
    );
}

#[test]
fn empty_fields_from_consecutive_separators() {
    assert_eq!(
        recs(&urldeblock(&[b"a&&b&"], &[])),
        vec![
            b"a".as_slice(),
            b"".as_slice(),
            b"b".as_slice(),
            b"".as_slice()
        ]
    );
}

#[test]
fn null_input_records_are_discarded() {
    assert_eq!(
        recs(&urldeblock(&[b"", b"a=1", b"", b"b=2"], &[])),
        vec![b"a=1".as_slice(), b"b=2".as_slice()]
    );
}

#[test]
fn no_splitters_copies_the_record() {
    assert_eq!(
        recs(&urldeblock(&[b"http://example/path"], &[])),
        vec![b"http://example/path".as_slice()]
    );
}

#[test]
fn incomplete_percent_copied_unchanged() {
    assert_eq!(
        recs(&urldeblock(&[b"a%&b%2&c%zz"], &[])),
        vec![b"a%".as_slice(), b"b%2".as_slice(), b"c%zz".as_slice()]
    );
}

#[test]
fn each_input_record_is_independent() {
    // Does not span records: trailing `&` on the first line is an empty field.
    assert_eq!(
        recs(&urldeblock(
            &[
                b"name=Craig%20R.%20Doe&class=90&",
                b"bdate=03%2F15%2F68&url="
            ],
            &[]
        )),
        vec![
            b"name=Craig R. Doe".as_slice(),
            b"class=90".as_slice(),
            b"".as_slice(),
            b"bdate=03/15/68".as_slice(),
            b"url=".as_slice(),
        ]
    );
}

#[test]
fn ebcdic_option_uses_ebcdic_specials() {
    // `%2b` inserts the raw hex byte X'2B' (ASCII plus), not EBCDIC plus
    // X'4E' — Note 2: escape values stay ASCII even under EBCDIC.
    let mut rec = ebcdic_text("a+b=%2b");
    rec.push(ascii_to_ebcdic(b'&'));
    rec.extend(ebcdic_text("c=1"));
    let got = urldeblock(&[&rec], &["EBCDIC"]);
    assert_eq!(got.len(), 2);
    let mut first = ebcdic_text("a b=");
    first.push(0x2B);
    assert_eq!(got[0], first);
    assert_eq!(got[1], ebcdic_text("c=1"));
}

#[test]
fn ebcdic_keyword_is_case_insensitive() {
    let rec = {
        let mut v = ebcdic_text("a");
        v.push(ascii_to_ebcdic(b'&'));
        v.extend(ebcdic_text("b"));
        v
    };
    let got = urldeblock(&[&rec], &["ebcdic"]);
    assert_eq!(got, vec![ebcdic_text("a"), ebcdic_text("b")]);
}

#[test]
fn ascii_ampersand_does_not_split_under_ebcdic() {
    // Note 2: an ASCII `&` (X'26') is not the EBCDIC ampersand (X'50').
    assert_eq!(
        recs(&urldeblock(&[b"a&b"], &["EBCDIC"])),
        vec![b"a&b".as_slice()]
    );
}

#[test]
fn invented_operands_rejected() {
    let err = err_of("literal a | urldeblock ASCII | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
    let err = err_of("literal a | urldeblock EBCDIC EXTRA | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_is_exact() {
    let reg = StageRegistry::default();
    assert!(reg.contains("URLDEBLOCK"));
    assert!(!reg.contains("URLDEBLOC"));
    let err = err_of("literal a | urldeblo | hole");
    assert!(
        err.contains("unknown stage") && err.contains("URLDEBLO"),
        "{err}"
    );
}

#[test]
fn does_not_delay_write_before_consume() {
    // p.578: urldeblock does not delay the record — write fields, then READ.
    let trace = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let src = b.add_stage("src", emit_bytes(&[b"a&b"]));
    let stage = b.add_stage("URLDEBLOCK", reg.create("URLDEBLOCK", vec![]).unwrap());
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(recs(&out.borrow()), vec![b"a".as_slice(), b"b".as_slice()]);

    let events: Vec<&str> = trace
        .borrow()
        .iter()
        .filter(|l| l.contains("] URLDEBLOCK "))
        .filter_map(|l| {
            if l.contains(" WRITE ") && l.contains(" len=") {
                Some("W")
            } else if l.contains(" READ ") && l.contains(" len=") {
                Some("R")
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        events,
        vec!["W", "W", "R"],
        "URLDEBLOCK must write fields before consume; trace:\n{}",
        trace.borrow().join("\n")
    );
}

#[test]
fn premature_termination_without_output() {
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in [b"a&b".as_slice(), b"c&d".as_slice()] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("URLDEBLOCK", reg.create("URLDEBLOCK", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "URLDEBLOCK should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn secondary_output_is_unused() {
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit_bytes(&[b"a&b"]));
    let stage = b.add_stage("URLDEBLOCK", reg.create("URLDEBLOCK", vec![]).unwrap());
    let sink_p = b.add_stage("p", sink_bytes(Rc::clone(&p)));
    let sink_s = b.add_stage("s", sink_bytes(Rc::clone(&s)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(recs(&p.borrow()), vec![b"a".as_slice(), b"b".as_slice()]);
    assert!(s.borrow().is_empty());
}

#[test]
fn extra_input_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src1 = b.add_stage("src1", emit_bytes(&[b"a"]));
    let src2 = b.add_stage("src2", emit_bytes(&[b"b"]));
    let stage = b.add_stage("URLDEBLOCK", reg.create("URLDEBLOCK", vec![]).unwrap());
    let hole = b.add_stage(
        "hole",
        Box::new(
            |ctx: &mut StageCtx| {
                while ctx.read().ok().flatten().is_some() {}
            },
        ),
    );
    b.connect(src1, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src2, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, hole);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string()
            .contains("no input stream other than the primary"),
        "{err}"
    );
}
