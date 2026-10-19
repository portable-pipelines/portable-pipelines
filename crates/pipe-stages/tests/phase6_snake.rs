//! SNAKE — Author's Edition 1.1.11 pp.491–492.
//!
//! Portable ASCII: records are bytes (one character is one byte, matching
//! EBCDIC in the book). The walkthrough (pp.16–17) and listpds (p.411)
//! examples use CMS producers (`< many users`, `cnctdn`, `listpds`); tests
//! use `literal` / `split` / `emit.rexx` / `pad`. SNAKE itself neither pads
//! nor truncates.

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
        "SNAKE",
        reg.create("SNAKE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("SNAKE"),
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
fn book_example_letters_in_a_square() {
    // p.492: `pipe literal a b c d | split | snake 2 | console` → ac / bd
    assert_eq!(
        run_spec_collect("literal a b c d | split | snake 2"),
        vec!["ac", "bd"]
    );
}

#[test]
fn omitted_depth_fills_columns_down_then_across() {
    // p.491: first output line is records 1, n+1, 2*n+1 with n = ceil(N/C).
    assert_eq!(
        run_primary(&["1", "2", "3", "4", "5", "6"], &["3"]),
        vec!["135", "246"]
    );
    // Last column not filled completely (p.491).
    assert_eq!(
        run_primary(&["1", "2", "3", "4", "5"], &["2"]),
        vec!["14", "25", "3"]
    );
}

#[test]
fn specified_page_depth() {
    assert_eq!(
        run_primary(&["1", "2", "3", "4", "5", "6"], &["2", "3"]),
        vec!["14", "25", "36"]
    );
}

#[test]
fn specified_depth_multiple_pages() {
    // Page size C*D = 4. Two full pages, then a partial page.
    assert_eq!(
        run_primary(&["a", "b", "c", "d", "e", "f", "g"], &["2", "2"]),
        vec!["ac", "bd", "eg", "f"]
    );
}

#[test]
fn last_partial_page_keeps_specified_depth() {
    // C=2, D=3, N=5: n stays 3, so first line is records 1 and 4.
    assert_eq!(
        run_primary(&["1", "2", "3", "4", "5"], &["2", "3"]),
        vec!["14", "25", "3"]
    );
}

#[test]
fn last_column_incomplete_when_depth_omitted() {
    // 7 records / 3 columns → depth 3; last column has one record.
    assert_eq!(
        run_primary(&["a", "b", "c", "d", "e", "f", "g"], &["3"]),
        vec!["adg", "be", "cf"]
    );
}

#[test]
fn neither_pads_nor_truncates() {
    // p.491 Input Record Format. Walkthrough uses `pad 9` first.
    assert_eq!(
        run_primary(&["aa", "b", "ccc", "d"], &["2"]),
        vec!["aaccc", "bd"]
    );
}

#[test]
fn pad_then_snake_walkthrough_standin() {
    // pp.16–17: `pad 9 | snake 6` — portable names, pad 3, 3 columns.
    assert_eq!(
        run_spec_collect("literal AA BB CC DD EE FF | split | pad 3 | snake 3"),
        vec!["AA CC EE ", "BB DD FF "]
    );
}

#[test]
fn one_column_is_identity() {
    assert_eq!(run_primary(&["a", "b", "c"], &["1"]), vec!["a", "b", "c"]);
    assert_eq!(
        run_primary(&["a", "b", "c", "d"], &["1", "2"]),
        vec!["a", "b", "c", "d"]
    );
}

#[test]
fn depth_one_joins_across() {
    // D=1: each page is one row of C records. See Also: join (p.492).
    assert_eq!(
        run_primary(&["a", "b", "c", "d", "e"], &["3", "1"]),
        vec!["abc", "de"]
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert!(run_primary(&[], &["2"]).is_empty());
}

#[test]
fn null_records_are_empty_cells() {
    assert_eq!(run_primary(&["a", "", "c"], &["3"]), vec!["ac"]);
    assert_eq!(run_primary(&[""], &["2"]), vec![""]);
}

#[test]
fn raw_bytes_are_concatenated() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        emit_bytes(vec![vec![0xff], vec![0x00], vec![0x01], vec![0x02]]),
    );
    let stage = b.add_stage("SNAKE", reg.create("SNAKE", vec!["2".into()]).unwrap());
    let snk = b.add_stage("sink", sink_bytes(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec![vec![0xff, 0x01], vec![0x00, 0x02]]);
}

#[test]
fn missing_columns_is_an_error() {
    let err = err_of("literal a | snake | hole");
    assert!(err.contains("columns is required"), "{err}");
}

#[test]
fn zero_and_invented_operands_are_rejected() {
    let err = err_of("literal a | snake 0 | hole");
    assert!(err.contains("must be positive"), "{err}");
    let err = err_of("literal a | snake 2 0 | hole");
    assert!(err.contains("must be positive"), "{err}");
    let err = err_of("literal a | snake columns 2 | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | snake 2 pad | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | snake 2 separator /-/ | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | snake 2 3 4 | hole");
    assert!(err.contains("unexpected operand"), "{err}");
    let err = err_of("literal a | snake * | hole");
    assert!(err.contains("unexpected operand"), "{err}");
}

#[test]
fn verb_is_exact() {
    let err = err_of("literal a | snak 2 | hole");
    assert!(
        err.contains("unknown stage") && err.to_ascii_uppercase().contains("SNAK"),
        "{err}"
    );
}

#[test]
fn extra_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage("SNAKE", reg.create("SNAKE", vec!["2".into()]).unwrap());
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
            for rec in ["one", "two", "three", "four"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("SNAKE", reg.create("SNAKE", vec!["2".into()]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "SNAKE should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn secondary_output_is_unused() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", "c", "d"]));
    let stage = b.add_stage("SNAKE", reg.create("SNAKE", vec!["2".into()]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["ac".to_string(), "bd".to_string()]);
    assert!(secondary.borrow().is_empty());
}

#[test]
fn delays_a_page_before_writing() {
    // p.491 Record Delay: SNAKE can delay all records that make up a page.
    // With a specified depth, the first output record is not written until
    // the page is full (or EOF).
    let saw_first_write_after = Rc::new(Cell::new(0usize));
    let produced = Rc::new(Cell::new(0usize));
    let first_write = Rc::clone(&saw_first_write_after);
    let count = Rc::clone(&produced);

    let reg = StageRegistry::default();
    let collected = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["a", "b", "c", "d"] {
                count.set(count.get() + 1);
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
            }
        }),
    );
    let stage = b.add_stage(
        "SNAKE",
        reg.create("SNAKE", vec!["2".into(), "2".into()]).unwrap(),
    );
    let snk = b.add_stage(
        "sink",
        Box::new({
            let first_write = Rc::clone(&first_write);
            let produced = Rc::clone(&produced);
            let collected = Rc::clone(&collected);
            move |ctx: &mut StageCtx| {
                if let Ok(Some(r)) = ctx.read() {
                    first_write.set(produced.get());
                    collected.borrow_mut().push(r.to_string_lossy());
                    while let Ok(Some(r)) = ctx.read() {
                        collected.borrow_mut().push(r.to_string_lossy());
                    }
                }
            }
        }),
    );
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    assert_eq!(
        *collected.borrow(),
        vec!["ac".to_string(), "bd".to_string()]
    );
    assert!(
        saw_first_write_after.get() >= 4,
        "first output should wait for a full page, saw write after {} records",
        saw_first_write_after.get()
    );
}
