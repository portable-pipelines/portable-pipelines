//! SPACE — Author's Edition 1.1.11 p.495.
//!
//! Portable ASCII: xorc `BLANK`/`SPACE` is X'20' (the book cites EBCDIC X'40');
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
        "SPACE",
        reg.create("SPACE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("SPACE"),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&primary)));
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
fn book_example_default_and_blank() {
    // p.495: `literal         a   b    b    |space|insert /*/ after`
    // Leading/trailing blanks removed; internal runs become one blank.
    assert_eq!(
        run_spec_collect("literal         a   b    b    |space|insert /*/ after"),
        vec!["a b b*"]
    );
    assert_eq!(
        run_spec_collect("literal         a   b    b    |space blank|insert /*/ after"),
        vec!["a b b*"]
    );
}

#[test]
fn book_example_zero_count_anyof_string() {
    // p.495: `space 0 /,.=/` — count 0 so the one string is the delimiter (Note 2).
    // No space before `|space` so LITERAL has no trailing blank.
    assert_eq!(
        run_spec_collect("literal 3.6=7,3|space 0 /,.=/"),
        vec!["3673"]
    );
    // Space before `|` is a LITERAL trailing blank (p.412). Blank is not in
    // `/,.=/`, so that trailing blank is kept.
    assert_eq!(
        run_spec_collect("literal 3.6=7,3 |space 0 /,.=/"),
        vec!["3673 "]
    );
}

#[test]
fn count_and_xorc_replacement() {
    assert_eq!(run_primary(&["a   b    c"], &["2"]), vec!["a  b  c"]);
    assert_eq!(run_primary(&["a   b"], &["0"]), vec!["ab"]);
    assert_eq!(run_primary(&["a   b"], &["3", "*"]), vec!["a***b"]);
    assert_eq!(run_primary(&["a   b"], &["*"]), vec!["a*b"]);
}

#[test]
fn string_replacement_and_anyof_delimiter() {
    assert_eq!(
        run_primary(&["a b c"], &["STRING", "/--/"]),
        vec!["a--b--c"]
    );
    assert_eq!(
        run_primary(&["a,b.c"], &["STRING", "/-/", "ANYOF", "/,./"]),
        vec!["a-b-c"]
    );
    assert_eq!(run_primary(&["a,b,,c"], &["ANYOF", "/,/"]), vec!["a b c"]);
}

#[test]
fn string_abbrev_min_3() {
    assert_eq!(run_primary(&["a b"], &["STR", "/xx/"]), vec!["axxb"]);
    let err = err_of("literal a b | space st /xx/ | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    assert_eq!(run_primary(&["a,b"], &["ANY", "/,/"]), vec!["a b"]);
    let err = err_of("literal a,b | space an /,/ | hole");
    assert!(err.contains("unexpected operand"), "{err}");
}

#[test]
fn note2_one_string_roles() {
    assert_eq!(run_primary(&["a b"], &["/-/"]), vec!["a-b"]);
    assert_eq!(run_primary(&["a*b**c"], &["0", "*"]), vec!["abc"]);
    assert_eq!(run_primary(&["a.b,c"], &["0", "/-/", "/.,/"]), vec!["abc"]);
}

#[test]
fn note1_numeric_xorc_needs_count() {
    // `space 40` is count 40 (one-blank replacement), not xorc X'40'.
    let long = format!("a{}b", " ".repeat(3));
    let out = run_primary(&[&long], &["40"]);
    assert_eq!(out, vec![format!("a{}b", " ".repeat(40))]);
    // `space 1 2a` — xorc X'2A' is `*` on ASCII.
    assert_eq!(run_primary(&["a b"], &["1", "2a"]), vec!["a*b"]);
    // `space 1 40` — xorc X'40' is ASCII '@' (book X'40' is EBCDIC blank).
    assert_eq!(run_primary(&["a b"], &["1", "40"]), vec!["a@b"]);
}

#[test]
fn leading_trailing_and_all_delimiter() {
    assert_eq!(run_primary(&["***a**b***"], &["1", "-", "*"]), vec!["a-b"]);
    assert_eq!(run_primary(&["****"], &["1", "-", "*"]), vec![""]);
    assert_eq!(run_primary(&[""], &[]), vec![""]);
    assert_eq!(run_primary(&["abc"], &[]), vec!["abc"]);
}

#[test]
fn tabulate_xorc() {
    assert_eq!(
        run_primary(&["a\tb\t\tc"], &["1", "-", "TABULATE"]),
        vec!["a-b-c"]
    );
    assert_eq!(run_primary(&["a\tb"], &["1", "-", "TAB"]), vec!["a-b"]);
}

#[test]
fn hex_dstring_replacement() {
    // Portable ASCII x2d2d is `--`.
    assert_eq!(run_primary(&["a b"], &["STRING", "x2d2d"]), vec!["a--b"]);
}

#[test]
fn invented_operands_rejected() {
    for w in [
        "PAD", "BOTH", "LEADING", "TRAILING", "LEFT", "ANYCASE", "FIRST",
    ] {
        let err = err_of(&format!("literal a | space {w} | hole"));
        assert!(err.contains("unexpected operand"), "{w}: {err}");
    }
    let err = err_of("literal a | space string | hole");
    assert!(err.contains("STRING requires a delimited string"), "{err}");
    let err = err_of("literal a | space anyof | hole");
    assert!(err.contains("ANYOF requires a delimited string"), "{err}");
}

#[test]
fn diagram_order_is_fixed() {
    let err = err_of("literal a,b | space anyof /,/ 2 | hole");
    assert!(err.contains("number must appear first"), "{err}");
    let err = err_of("literal a | space /-/ string /x/ | hole");
    assert!(err.contains("STRING"), "{err}");
}

#[test]
fn verb_is_exact() {
    for w in ["spac", "spa", "sp"] {
        let err = err_of(&format!("literal a | {w} | hole"));
        assert!(err.contains("unknown stage"), "{w}: {err}");
    }
}

#[test]
fn extra_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("SPACE", reg.create("SPACE", vec![]).unwrap());
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
            for rec in ["one two", "three four"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("SPACE", reg.create("SPACE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "SPACE should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn secondary_output_is_unused() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a   b"]));
    let stage = b.add_stage("SPACE", reg.create("SPACE", vec![]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["a b".to_string()]);
    assert!(secondary.borrow().is_empty());
}

#[test]
fn raw_bytes_delimiter() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        emit_bytes(vec![vec![b'a', 0x00, b'b', 0x00, 0x00, b'c']]),
    );
    let stage = b.add_stage(
        "SPACE",
        reg.create("SPACE", vec!["1".into(), "-".into(), "00".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec![b"a-b-c".to_vec()]);
}

#[test]
fn default_space_on_two_records() {
    assert_eq!(run_primary(&["  a  b  ", "c   d"], &[]), vec!["a b", "c d"]);
}
