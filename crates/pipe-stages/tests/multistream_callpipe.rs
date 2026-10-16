use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{CallPipeConnectors, PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{callpipe_from_spec, run_spec, StageRegistry};

#[test]
fn labeled_locate_splits_streams() {
    // Matching records → primary console path collected; non-matching → secondary.
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);

    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            ctx.write(Record::from("apple")).unwrap();
            ctx.write(Record::from("banana")).unwrap();
            ctx.write(Record::from("apricot")).unwrap();
        }),
    );
    let loc = b.add_stage(
        "locate",
        reg.create("LOCATE", vec!["/ap/".into()]).unwrap(),
    );
    let sink_p = b.add_stage(
        "sink_p",
        Box::new(move |ctx| {
            while let Ok(Some(r)) = ctx.read() {
                p.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    let sink_s = b.add_stage(
        "sink_s",
        Box::new(move |ctx| {
            while let Ok(Some(r)) = ctx.read() {
                s.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, loc);
    b.connect(loc, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(loc, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();

    assert_eq!(
        *primary.borrow(),
        vec!["apple".to_string(), "apricot".to_string()]
    );
    assert_eq!(*secondary.borrow(), vec!["banana".to_string()]);
}

#[test]
fn endchar_spec_runs() {
    // Smoke: multi-pipeline spec parses and executes without stalling.
    run_spec("(ENDCHAR ?) literal apple|literal banana | L: locate /ap/ | hole ? L: | hole")
        .unwrap();
}

#[test]
fn callpipe_with_connectors() {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);

    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            ctx.write(Record::from("one")).unwrap();
            ctx.write(Record::from("two")).unwrap();
        }),
    );
    let nested = b.add_stage(
        "callpipe",
        Box::new(|ctx| {
            callpipe_from_spec(
                ctx,
                "*: | change /one/ /1/ | change /two/ /2/ | *:",
                &StageRegistry::default(),
            )
            .unwrap();
        }),
    );
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx| {
            while let Ok(Some(r)) = ctx.read() {
                out.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, nested);
    b.connect_primary(nested, sink);
    b.run().unwrap();
    assert_eq!(*collected.borrow(), vec!["1".to_string(), "2".to_string()]);
}

#[test]
fn stage_ctx_callpipe_direct() {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let mut b = PipelineBuilder::new();
    let a = b.add_stage(
        "outer",
        Box::new(|ctx| {
            ctx.write(Record::from("z")).unwrap();
            let _ = ctx; // write then callpipe reading...
        }),
    );
    // Simpler: nested without connectors
    let n = b.add_stage(
        "inner",
        Box::new(|ctx| {
            ctx.callpipe(
                |pb| {
                    let x = pb.add_stage(
                        "lit",
                        Box::new(|c| {
                            c.write(Record::from("nested")).unwrap();
                        }),
                    );
                    let y = pb.add_stage(
                        "copy",
                        Box::new(|c| {
                            while let Ok(Some(r)) = c.read() {
                                let _ = c.write(r);
                            }
                        }),
                    );
                    pb.connect_primary(x, y);
                    let _ = CallPipeConnectors::default();
                },
                CallPipeConnectors::default(),
            )
            .unwrap();
            // Also forward any remaining input
            while let Ok(Some(r)) = ctx.read() {
                let _ = ctx.write(r);
            }
        }),
    );
    let s = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                out.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(a, n);
    b.connect_primary(n, s);
    b.run().unwrap();
    // Nested without connectors does not automatically emit to parent output;
    // parent still forwards its input "z".
    assert!(collected.borrow().iter().any(|x| x == "z"));
}
