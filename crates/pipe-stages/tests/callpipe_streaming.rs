//! Streaming CALLPIPE: loans, leftover label records, multi-stream connectors,
//! SHORT/EOF across the boundary, nested CALLPIPE, and stall.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{
    CallPipeAttachment, CallPipeConnectors, LoanSide, PipelineBuilder, Record, StageCtx, StreamId,
};
use pipe_stages::{callpipe_from_spec, StageRegistry};

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

fn leftover_after(spec: &str, records: &[&str]) -> (Vec<String>, Vec<String>) {
    let nested_out = Rc::new(RefCell::new(Vec::new()));
    let leftover = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&nested_out);
    let left = Rc::clone(&leftover);
    let spec = spec.to_string();

    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            callpipe_from_spec(ctx, &spec, &StageRegistry::default()).unwrap();
            while let Ok(Some(r)) = ctx.read() {
                left.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    let snk = b.add_stage("sink", sink(out));
    b.connect_primary(src, caller);
    b.connect_primary(caller, snk);
    b.run().unwrap();
    let nested = nested_out.borrow().clone();
    let left = leftover.borrow().clone();
    (nested, left)
}

#[test]
fn strtolabel_exclusive_leftover_is_the_match() {
    let (nested, leftover) =
        leftover_after("*: | strtolabel /.end/ | *:", &["a", "b", ".end", "c"]);
    assert_eq!(nested, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(leftover, vec![".end".to_string(), "c".to_string()]);
}

#[test]
fn strtolabel_inclusive_leftover_is_the_rest() {
    let (nested, leftover) =
        leftover_after("*: | strtolabel inclusive /.end/ | *:", &["a", ".end", "c"]);
    assert_eq!(nested, vec!["a".to_string(), ".end".to_string()]);
    assert_eq!(leftover, vec!["c".to_string()]);
}

#[test]
fn strwhilelabel_leftover_is_the_first_nonmatch() {
    let (nested, leftover) =
        leftover_after("*: | strwhilelabel /*/ | *:", &["*a", "*b", "c", "*d"]);
    assert_eq!(nested, vec!["*a".to_string(), "*b".to_string()]);
    assert_eq!(leftover, vec!["c".to_string(), "*d".to_string()]);
}

#[test]
fn secondary_connected_still_takes_the_leftover() {
    let primary = Rc::new(RefCell::new(Vec::new()));
    let leftover = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let left = Rc::clone(&leftover);

    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "b", ".end", "c"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            callpipe_from_spec(
                ctx,
                "(ENDCHAR ?) *: | t: tolabel .end| *: ? t: | hole",
                &StageRegistry::default(),
            )
            .unwrap();
            while let Ok(Some(r)) = ctx.read() {
                left.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    let sink_p = b.add_stage("p", sink(p));
    // Caller secondary is not used; nested secondary of TOLABEL goes to hole.
    b.connect_primary(src, caller);
    b.connect_primary(caller, sink_p);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["a".to_string(), "b".to_string()]);
    assert!(
        leftover.borrow().is_empty(),
        "secondary copy should consume leftover, got {:?}",
        leftover.borrow()
    );
}

#[test]
fn select_then_bare_star_uses_selected_stream() {
    let p_out = Rc::new(RefCell::new(Vec::new()));
    let s_out = Rc::new(RefCell::new(Vec::new()));
    let po = Rc::clone(&p_out);
    let so = Rc::clone(&s_out);

    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(&["keep"]));
    let src_s = b.add_stage("src_s", emit(&["one"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            ctx.select_input(StreamId::SECONDARY);
            ctx.select_output(StreamId::SECONDARY);
            callpipe_from_spec(ctx, "*: | change /one/ /1/ | *:", &StageRegistry::default())
                .unwrap();
            ctx.select_input(StreamId::PRIMARY);
            ctx.select_output(StreamId::PRIMARY);
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r).is_err() {
                    break;
                }
            }
        }),
    );
    let sink_p = b.add_stage("sink_p", sink(po));
    let sink_s = b.add_stage("sink_s", sink(so));
    b.connect(src_p, StreamId::PRIMARY, caller, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, caller, StreamId::SECONDARY);
    b.connect(caller, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(caller, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p_out.borrow(), vec!["keep".to_string()]);
    assert_eq!(*s_out.borrow(), vec!["1".to_string()]);
}

#[test]
fn numbered_connectors_loan_two_streams() {
    let p_out = Rc::new(RefCell::new(Vec::new()));
    let s_out = Rc::new(RefCell::new(Vec::new()));
    let po = Rc::clone(&p_out);
    let so = Rc::clone(&s_out);

    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("src_p", emit(&["a"]));
    let src_s = b.add_stage("src_s", emit(&["b"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            callpipe_from_spec(
                ctx,
                "(ENDCHAR ?) *.input: | change /a/ /A/ | *.output: ? *.input.1: | change /b/ /B/ | *.output.1:",
                &StageRegistry::default(),
            )
            .unwrap();
        }),
    );
    let sink_p = b.add_stage("sink_p", sink(po));
    let sink_s = b.add_stage("sink_s", sink(so));
    b.connect(src_p, StreamId::PRIMARY, caller, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, caller, StreamId::SECONDARY);
    b.connect(caller, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(caller, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*p_out.borrow(), vec!["A".to_string()]);
    assert_eq!(*s_out.borrow(), vec!["B".to_string()]);
}

#[test]
fn tertiary_connector_word_loans_stream_two() {
    let t_out = Rc::new(RefCell::new(Vec::new()));
    let to = Rc::clone(&t_out);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["x"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            callpipe_from_spec(
                ctx,
                "*.input.tertiary: | change /x/ /X/ | *.output.tertiary:",
                &StageRegistry::default(),
            )
            .unwrap();
        }),
    );
    let sink = b.add_stage("sink", sink(to));
    b.connect(src, StreamId::PRIMARY, caller, StreamId::TERTIARY);
    b.connect(caller, StreamId::TERTIARY, sink, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(*t_out.borrow(), vec!["X".to_string()]);
}

#[test]
fn read_then_callpipe_skips_the_consumed_record() {
    let nested_out = Rc::new(RefCell::new(Vec::new()));
    let leftover = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&nested_out);
    let left = Rc::clone(&leftover);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["gone", "kept"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx: &mut StageCtx| {
            let consumed = ctx.read().unwrap().unwrap();
            assert_eq!(consumed.to_string_lossy(), "gone");
            callpipe_from_spec(ctx, "*: | xlate 1-* upper | *:", &StageRegistry::default())
                .unwrap();
            while let Ok(Some(r)) = ctx.read() {
                left.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    let snk = b.add_stage("sink", sink(out));
    b.connect_primary(src, caller);
    b.connect_primary(caller, snk);
    b.run().unwrap();
    assert_eq!(*nested_out.borrow(), vec!["KEPT".to_string()]);
    assert!(leftover.borrow().is_empty(), "{:?}", leftover.borrow());
}

#[test]
fn nested_callpipe_inside_callpipe() {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["ab"]));
    let caller = b.add_stage(
        "caller",
        Box::new(|ctx| {
            ctx.callpipe(
                |pb| {
                    let _ = pb.add_stage(
                        "inner",
                        Box::new(|c| {
                            callpipe_from_spec(
                                c,
                                "*: | change /ab/ /AB/ | *:",
                                &StageRegistry::default(),
                            )
                            .unwrap();
                        }),
                    );
                },
                CallPipeConnectors {
                    attachments: vec![
                        CallPipeAttachment {
                            side: LoanSide::Input,
                            caller_stream: StreamId::PRIMARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        },
                        CallPipeAttachment {
                            side: LoanSide::Output,
                            caller_stream: StreamId::PRIMARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        },
                    ],
                    processing: None,
                },
            )
            .unwrap();
        }),
    );
    let snk = b.add_stage("sink", sink(out));
    b.connect_primary(src, caller);
    b.connect_primary(caller, snk);
    b.run().unwrap();
    assert_eq!(*collected.borrow(), vec!["AB".to_string()]);
}

#[test]
fn nested_callpipe_then_caller_footer() {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["ab"]));
    let caller = b.add_stage(
        "caller",
        Box::new(|ctx| {
            ctx.callpipe(
                |pb| {
                    let _ = pb.add_stage(
                        "inner",
                        Box::new(|c| {
                            callpipe_from_spec(
                                c,
                                "*: | change /ab/ /AB/ | *:",
                                &StageRegistry::default(),
                            )
                            .unwrap();
                        }),
                    );
                },
                CallPipeConnectors {
                    attachments: vec![
                        CallPipeAttachment {
                            side: LoanSide::Input,
                            caller_stream: StreamId::PRIMARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        },
                        CallPipeAttachment {
                            side: LoanSide::Output,
                            caller_stream: StreamId::PRIMARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        },
                    ],
                    processing: None,
                },
            )
            .unwrap();
            ctx.write(Record::from("footer"))
                .expect("caller footer after nested CALLPIPE");
        }),
    );
    let snk = b.add_stage("sink", sink(out));
    b.connect_primary(src, caller);
    b.connect_primary(caller, snk);
    b.run().unwrap();
    assert_eq!(
        *collected.borrow(),
        vec!["AB".to_string(), "footer".to_string()]
    );
}

#[test]
fn nested_eof_on_loaned_input_is_eof_for_the_caller() {
    let leftover = Rc::new(RefCell::new(0usize));
    let n = Rc::clone(&leftover);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["only"]));
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx| {
            callpipe_from_spec(ctx, "*: | copy | *:", &StageRegistry::default()).unwrap();
            let mut count = 0;
            while let Ok(Some(_)) = ctx.read() {
                count += 1;
            }
            *n.borrow_mut() = count;
        }),
    );
    let snk = b.add_stage(
        "sink",
        Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {}),
    );
    b.connect_primary(src, caller);
    b.connect_primary(caller, snk);
    b.run().unwrap();
    assert_eq!(*leftover.borrow(), 0);
}

#[test]
fn nested_short_input_propagates_to_caller() {
    let saw = Rc::new(RefCell::new(false));
    let flag = Rc::clone(&saw);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            let _ = ctx.write(Record::from("a"));
            let _ = ctx.write(Record::from("b"));
        }),
    );
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx| {
            ctx.callpipe(
                |pb| {
                    let _ = pb.add_stage(
                        "short",
                        Box::new(|c| {
                            let _ = c.read();
                            c.short_input();
                        }),
                    );
                },
                CallPipeConnectors {
                    attachments: vec![CallPipeAttachment {
                        side: LoanSide::Input,
                        caller_stream: StreamId::PRIMARY,
                        nested_stage: 0,
                        nested_stream: StreamId::PRIMARY,
                    }],
                    processing: None,
                },
            )
            .unwrap();
            *flag.borrow_mut() = ctx.read().is_err();
        }),
    );
    b.connect_primary(src, caller);
    b.run().unwrap();
    assert!(
        *saw.borrow(),
        "nested SHORT on a loaned input must sever the caller stream"
    );
}

#[test]
fn nested_callpipe_stall_is_plain_words() {
    let mut b = PipelineBuilder::new();
    let caller = b.add_stage(
        "caller",
        Box::new(|ctx| {
            let _ = callpipe_from_spec(
                ctx,
                "(ENDCHAR ?) literal a|literal b| a: fanout | f: fanin | hole ? a: | f:",
                &StageRegistry::default(),
            );
        }),
    );
    let src = b.add_stage("src", emit(&["a", "b"]));
    b.connect_primary(src, caller);
    let err = b.run().expect_err("expected stall");
    let msg = err.to_string();
    assert!(
        msg.contains("pipeline stalled (no stage can make progress)"),
        "{msg}"
    );
    assert!(msg.contains("waiting for CALLPIPE"), "{msg}");
}
