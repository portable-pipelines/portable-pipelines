//! TOKENISE / TOKENIZE — Author's Edition 1.1.11 p.558.
//!
//! Portable ASCII: blank is X'20' (the book cites EBCDIC X'40'). Hex/binary
//! dstrings are raw bytes; the book's EBCDIC hex examples are not used here.

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
        "TOKENISE",
        reg.create("TOKENISE", args.iter().map(|a| (*a).to_string()).collect())
            .expect("TOKENISE"),
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
fn book_example_apples() {
    // p.558. Space before `| tokenise` is a LITERAL trailing blank (p.412);
    // TOKENISE discards blanks, so the printed tokens match.
    assert_eq!(
        run_spec_collect("literal apples = bananas(cherries+dates) | tokenise /()=+/"),
        vec!["apples", "=", "bananas", "(", "cherries", "+", "dates", ")"]
    );
    assert_eq!(
        run_spec_collect("literal apples = bananas(cherries+dates)| tokenise /()=+/"),
        vec!["apples", "=", "bananas", "(", "cherries", "+", "dates", ")"]
    );
}

#[test]
fn tokenize_synonym() {
    assert_eq!(
        run_spec_collect("literal a=b| tokenize /=/"),
        vec!["a", "=", "b"]
    );
    let reg = StageRegistry::default();
    assert!(reg.contains("TOKENISE"));
    assert!(reg.contains("TOKENIZE"));
    assert!(reg.create("TOKENIZE", vec!["/=/".into()]).is_some());
}

#[test]
fn blanks_discarded_singles_written() {
    assert_eq!(
        run_primary(&["  apples = bananas  "], &["/()=+/"]),
        vec!["apples", "=", "bananas"]
    );
    assert_eq!(run_primary(&["a()b"], &["/()/"]), vec!["a", "(", ")", "b"]);
    assert_eq!(run_primary(&["=="], &["/=/"]), vec!["=", "="]);
}

#[test]
fn null_input_record() {
    // Null input: no tokens. Second dstring is still written after the line.
    assert_eq!(run_primary(&[""], &["/()=+/"]), Vec::<String>::new());
    assert_eq!(run_primary(&[""], &["/()=+/", "/ /"]), vec![" "]);
    assert_eq!(run_primary(&[""], &["/()=+/", "//"]), vec![""]);
    assert_eq!(run_primary(&["   "], &["/()=+/"]), Vec::<String>::new());
}

#[test]
fn null_first_dstring_blanks_only() {
    assert_eq!(
        run_primary(&["apples=bananas"], &["//"]),
        vec!["apples=bananas"]
    );
    assert_eq!(
        run_primary(&["apples=bananas"], &[",,"]),
        vec!["apples=bananas"]
    );
    assert_eq!(run_primary(&["a = b"], &["//"]), vec!["a", "=", "b"]);
    assert_eq!(run_primary(&["a=b"], &["/=/"]), vec!["a", "=", "b"]);
}

#[test]
fn blank_in_first_dstring_is_not_emitted() {
    assert_eq!(run_primary(&["a b"], &["/ /"]), vec!["a", "b"]);
}

#[test]
fn second_dstring_after_each_line() {
    // CMS-style: `tokenise /()/ / /` writes a blank record after each line.
    assert_eq!(
        run_primary(&["a (b)", "c"], &["/()/", "/ /"]),
        vec!["a", "(", "b", ")", " ", "c", " "]
    );
    assert_eq!(
        run_spec_collect("literal a b| tokenise // /:/"),
        vec!["a", "b", ":"]
    );
}

#[test]
fn hex_dstring_ascii() {
    // Portable ASCII x3d is `=` (book EBCDIC hex would differ).
    assert_eq!(run_primary(&["a=b"], &["x3d"]), vec!["a", "=", "b"]);
}

#[test]
fn each_input_record_separately() {
    assert_eq!(
        run_primary(&["a = b", "c(d)"], &["/()=+/"]),
        vec!["a", "=", "b", "c", "(", "d", ")"]
    );
}

#[test]
fn missing_and_invented_operands_rejected() {
    let err = err_of("literal a | tokenise | hole");
    assert!(err.contains("delimitedString is required"), "{err}");
    for w in ["BLANK", "SPACE", "ANYCASE", "STRING", "ANYOF", "PAD"] {
        let err = err_of(&format!("literal a | tokenise {w} | hole"));
        assert!(
            err.contains("unexpected operand") || err.contains("not a delimited string"),
            "{w}: {err}"
        );
    }
    let err = err_of("literal a | tokenise /(/ extra | hole");
    assert!(
        err.contains("not a delimited string") || err.contains("unexpected"),
        "{err}"
    );
    let err = err_of("literal a | tokenise /(/ / / /x/ | hole");
    assert!(err.contains("unexpected operand"), "{err}");
}

#[test]
fn verb_is_exact() {
    for w in ["token", "toke", "tokeni", "tokeniz", "tokenis"] {
        let err = err_of(&format!("literal a | {w} /(/ | hole"));
        assert!(err.contains("unknown stage"), "{w}: {err}");
    }
}

#[test]
fn extra_input_is_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["b"]));
    let stage = b.add_stage(
        "TOKENISE",
        reg.create("TOKENISE", vec!["//".into()]).unwrap(),
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
            for rec in ["one two", "three"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage(
        "TOKENISE",
        reg.create("TOKENISE", vec!["//".into()]).unwrap(),
    );
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "TOKENISE should stop when no output is connected, wrote {}",
        written.get()
    );
}

#[test]
fn secondary_output_is_unused() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a b"]));
    let stage = b.add_stage(
        "TOKENISE",
        reg.create("TOKENISE", vec!["//".into()]).unwrap(),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["a".to_string(), "b".to_string()]);
    assert!(secondary.borrow().is_empty());
}
