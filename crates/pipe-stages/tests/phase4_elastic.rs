//! ELASTIC — Author's Edition 1.1.11 p.335 (tutorial Ch.5 pp.88–89; FANIN note 2 p.338).

use std::cell::RefCell;
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

fn run_primary(records: &[&str]) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage("ELASTIC", reg.create("ELASTIC", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

fn fanout_fanin(
    records: &[&str],
    elastic_on_secondary: bool,
) -> pipe_core::PipeResult<Vec<String>> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let fan = b.add_stage("FANOUT", reg.create("FANOUT", vec![]).unwrap());
    let join = b.add_stage("FANIN", reg.create("FANIN", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, join, StreamId::PRIMARY);
    if elastic_on_secondary {
        let elas = b.add_stage("ELASTIC", reg.create("ELASTIC", vec![]).unwrap());
        b.connect(fan, StreamId::SECONDARY, elas, StreamId::PRIMARY);
        b.connect(elas, StreamId::PRIMARY, join, StreamId::SECONDARY);
    } else {
        b.connect(fan, StreamId::SECONDARY, join, StreamId::SECONDARY);
    }
    b.connect_primary(join, snk);
    b.run()?;
    let result = out.borrow().clone();
    Ok(result)
}

#[test]
fn primary_only_copies_records() {
    assert_eq!(run_primary(&["a", "b", "c"]), vec!["a", "b", "c"]);
}

#[test]
fn primary_only_null_record() {
    assert_eq!(run_primary(&[""]), vec![""]);
}

#[test]
fn primary_only_empty_file() {
    assert_eq!(run_primary(&[]), Vec::<String>::new());
}

#[test]
fn primary_only_run_spec() {
    let err = run_spec("literal a|literal b | elastic | hole");
    assert!(err.is_ok(), "{err:?}");
}

#[test]
fn no_operands() {
    let err = run_spec("literal a | elastic 10 | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn invented_keyword_rejected() {
    let err = run_spec("literal a | elastic forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_is_not_abbreviable() {
    let reg = StageRegistry::default();
    assert!(reg.contains("ELASTIC"));
    assert!(!reg.contains("ELASTI"));
    assert!(!reg.contains("ELAS"));
    let err = run_spec("literal a | elast | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_ascii_uppercase().contains("UNKNOWN STAGE")
            && msg.to_ascii_uppercase().contains("ELAST"),
        "{err}"
    );
}

#[test]
fn secondary_output_rejected() {
    let err = run_spec("(ENDCHAR ?) literal a | e: elastic | hole ? e: | hole").unwrap_err();
    assert!(err.to_string().contains("secondary output"), "{err}");
}

#[test]
fn tertiary_input_rejected() {
    let err =
        run_spec("(ENDCHAR ?) literal a | e: elastic | hole ? literal b | e: ? literal c | e:")
            .unwrap_err();
    assert!(err.to_string().contains("primary and secondary"), "{err}");
}

#[test]
fn premature_termination_without_output() {
    run_spec("literal a | elastic").unwrap();
}

#[test]
fn fanout_fanin_stalls_without_elastic() {
    // Tutorial p.88 / FANIN note 2 p.338: two paths from one producer to FANIN
    // stall unless the non-primary path buffers.
    let err = fanout_fanin(&["a", "b"], false).unwrap_err();
    assert!(err.to_string().contains("stalled"), "{err}");
}

#[test]
fn fanout_fanin_completes_with_elastic() {
    let out = fanout_fanin(&["a", "b"], true).unwrap();
    // FANIN: all primary (the direct fanout copies), then secondary (elastic).
    assert_eq!(out, vec!["a", "b", "a", "b"]);
}

#[test]
fn copy_on_secondary_still_stalls() {
    // Note 1: COPY is a one-record delay. The connection slots plus that
    // hold absorb a few records; a longer file still stalls, while ELASTIC
    // does not.
    let recs = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&recs));
    let fan = b.add_stage("FANOUT", reg.create("FANOUT", vec![]).unwrap());
    let copy = b.add_stage("COPY", reg.create("COPY", vec![]).unwrap());
    let join = b.add_stage("FANIN", reg.create("FANIN", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, fan);
    b.connect(fan, StreamId::PRIMARY, join, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, copy, StreamId::PRIMARY);
    b.connect(copy, StreamId::PRIMARY, join, StreamId::SECONDARY);
    b.connect_primary(join, snk);
    let err = b.run().unwrap_err();
    assert!(err.to_string().contains("stalled"), "{err}");
    assert_eq!(fanout_fanin(&recs, true).unwrap().len(), recs.len() * 2);
}

#[test]
fn secondary_after_primary() {
    // Streams Used: primary file first, then the buffered secondary.
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let prim = b.add_stage("prim", emit(&["A", "B"]));
    let sec = b.add_stage("sec", emit(&["X", "Y"]));
    let elas = b.add_stage("ELASTIC", reg.create("ELASTIC", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(prim, StreamId::PRIMARY, elas, StreamId::PRIMARY);
    b.connect(sec, StreamId::PRIMARY, elas, StreamId::SECONDARY);
    b.connect_primary(elas, snk);
    b.run().unwrap();
    assert_eq!(
        *out.borrow(),
        vec![
            "A".to_string(),
            "B".to_string(),
            "X".to_string(),
            "Y".to_string()
        ]
    );
}

#[test]
fn feedback_todo_list() {
    // p.335 example shape: secondary is feedback. A seed on primary is
    // rewritten once; the child does not match, so the list empties.
    //
    //   seed → e: elastic → f: fanout → sink
    //                      ↘ locate /seed/ → spec /child/ → e:
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["seed"]));
    let elas = b.add_stage("ELASTIC", reg.create("ELASTIC", vec![]).unwrap());
    let fan = b.add_stage("FANOUT", reg.create("FANOUT", vec![]).unwrap());
    let loc = b.add_stage(
        "LOCATE",
        reg.create("LOCATE", vec!["/seed/".into()]).unwrap(),
    );
    let spec = b.add_stage(
        "SPEC",
        reg.create("SPEC", vec!["/child/".into(), "1".into()])
            .unwrap(),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, elas);
    b.connect_primary(elas, fan);
    b.connect(fan, StreamId::PRIMARY, snk, StreamId::PRIMARY);
    b.connect(fan, StreamId::SECONDARY, loc, StreamId::PRIMARY);
    b.connect_primary(loc, spec);
    b.connect(spec, StreamId::PRIMARY, elas, StreamId::SECONDARY);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["seed".to_string(), "child".to_string()]);
}

#[test]
fn feedback_nested_imbeds() {
    // p.335 SCRIPT-imbed shape: each output name can enqueue another.
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["MASTER"]));
    let elas = b.add_stage(
        "ELASTIC",
        StageRegistry::default().create("ELASTIC", vec![]).unwrap(),
    );
    let expand = b.add_stage(
        "expand",
        Box::new(|ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                let name = r.to_string_lossy();
                if ctx.write(r.clone()).is_err() {
                    return;
                }
                let next = match name.as_str() {
                    "MASTER" => Some("child"),
                    "child" => Some("grand"),
                    _ => None,
                };
                if let Some(n) = next {
                    ctx.select_output(StreamId::SECONDARY);
                    if ctx.write(Record::from(n)).is_err() {
                        return;
                    }
                    ctx.select_output(StreamId::PRIMARY);
                }
            }
        }),
    );
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect_primary(src, elas);
    b.connect_primary(elas, expand);
    b.connect(expand, StreamId::PRIMARY, snk, StreamId::PRIMARY);
    b.connect(expand, StreamId::SECONDARY, elas, StreamId::SECONDARY);
    b.run().unwrap();
    assert_eq!(
        *out.borrow(),
        vec![
            "MASTER".to_string(),
            "child".to_string(),
            "grand".to_string()
        ]
    );
}

#[test]
fn secondary_eof_flushes_buffer() {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let prim = b.add_stage("prim", emit(&["p"]));
    let sec = b.add_stage("sec", emit(&["s1", "s2"]));
    let elas = b.add_stage("ELASTIC", reg.create("ELASTIC", vec![]).unwrap());
    let snk = b.add_stage("sink", sink(Rc::clone(&out)));
    b.connect(prim, StreamId::PRIMARY, elas, StreamId::PRIMARY);
    b.connect(sec, StreamId::PRIMARY, elas, StreamId::SECONDARY);
    b.connect_primary(elas, snk);
    b.run().unwrap();
    assert_eq!(
        *out.borrow(),
        vec!["p".to_string(), "s1".to_string(), "s2".to_string()]
    );
}

#[test]
fn labeled_spec_fanout_fanin_with_elastic() {
    run_spec("(ENDCHAR ?) literal a|literal b | f: fanout | i: fanin | hole ? f: | elastic | i:")
        .unwrap();
}
