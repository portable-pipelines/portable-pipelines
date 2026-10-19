//! RUNPIPE — Author's Edition 1.1.11 pp.481–483.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, run_spec_with_return_code, StageRegistry};

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
    let (out, _) = run_streams(records, args, false);
    out
}

fn run_streams(records: &[&str], args: &[&str], secondary: bool) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary_out = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "RUNPIPE",
        reg.create("RUNPIPE", args.iter().map(|a| (*a).to_string()).collect())
            .unwrap(),
    );
    let snk_p = b.add_stage("sink_p", sink(Rc::clone(&primary)));
    b.connect_primary(src, stage);
    b.connect_primary(stage, snk_p);
    if secondary {
        let snk_s = b.add_stage("sink_s", sink(Rc::clone(&secondary_out)));
        b.connect(stage, StreamId::SECONDARY, snk_s, StreamId::PRIMARY);
    }
    b.run().unwrap();
    let primary = primary.borrow().clone();
    let secondary_out = secondary_out.borrow().clone();
    (primary, secondary_out)
}

#[test]
fn book_success_writes_no_messages() {
    // Nested PIPE data does not flow through RUNPIPE; only messages do (p.482).
    assert!(run_spec_collect("literal hole | runpipe").is_empty());
    assert!(run_spec_collect("literal literal hello | runpipe").is_empty());
}

#[test]
fn book_null_and_blank_records_are_discarded() {
    // p.453 Streams Used: null and blank input records are discarded.
    assert!(run_primary(&[""], &[]).is_empty());
    assert!(run_primary(&["   "], &[]).is_empty());
    assert!(run_primary(&["", "hole", "  "], &[]).is_empty());
}

#[test]
fn book_unknown_stage_is_intercepted() {
    // p.481: messages are written to RUNPIPE's primary output, not the terminal.
    let out = run_spec_collect("literal nosuch | runpipe");
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].starts_with("PIPES0001E "), "{out:?}");
    assert!(out[0].contains("unknown stage"), "{out:?}");
    let word = out[0].split_whitespace().next().unwrap();
    assert!(
        word.len() == 10 || word.len() == 11,
        "msgid length {}",
        word.len()
    );
}

#[test]
fn book_nested_error_does_not_fail_parent() {
    run_spec("literal nosuch | runpipe | hole").unwrap();
}

#[test]
fn book_availability_rc() {
    // p.483: `PIPE literal deblock Linend Eof | runpipe` then RC=0 if it ran.
    assert_eq!(
        run_spec_with_return_code("literal hole | runpipe").unwrap(),
        0
    );
    let rc = run_spec_with_return_code("literal nosuch | runpipe").unwrap();
    assert_ne!(rc, 0, "unknown nested stage must be a nonzero aggregate RC");
}

#[test]
fn book_aggregate_return_codes() {
    // p.483: min if any negative, else max. Nested AGGRC as last stage sets RC.
    assert_eq!(
        run_spec_with_return_code("literal literal 9||literal 3||aggrc | runpipe").unwrap(),
        9
    );
    assert_eq!(
        run_spec_with_return_code(
            "literal literal 9||literal 3||aggrc|literal literal -3||literal 6||aggrc | runpipe"
        )
        .unwrap(),
        -3
    );
}

#[test]
fn book_self_escape_nested_spec() {
    // Input record is a full PIPE spec; `||` self-escapes `|` in the outer scan.
    assert!(run_spec_collect("literal literal hello||hole | runpipe").is_empty());
}

#[test]
fn book_trace_operand_forces_trace() {
    // p.481: TRACE forces the nested pipeline set to run with TRACE.
    let out = run_spec_collect("literal hole | runpipe trace");
    assert!(
        out.iter()
            .any(|r| r.starts_with("PIPES0002I ") && r.contains("dispatcher start")),
        "{out:?}"
    );
}

#[test]
fn book_nested_trace_option_is_intercepted() {
    // p.483 example: `(trace)` on the nested spec is captured by RUNPIPE.
    let out = run_spec_collect("literal (trace) hole | runpipe");
    assert!(
        out.iter()
            .any(|r| r.starts_with("PIPES0002I ") && r.contains("dispatcher start")),
        "{out:?}"
    );
}

#[test]
fn book_message_command_is_intercepted() {
    // p.482: messages issued with MESSAGE are written to RUNPIPE's output.
    let out = run_spec_collect("literal literal message hello|| pipcmd | runpipe");
    assert_eq!(out, vec!["PIPES0003I hello".to_string()]);
}

#[test]
fn book_writes_before_consume() {
    // Record delay (p.482): output for a spec is written before the next spec runs.
    let out = run_spec_collect("literal nosuchb|literal nosucha | runpipe");
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(out[0].contains("NOSUCHA"), "{out:?}");
    assert!(out[1].contains("NOSUCHB"), "{out:?}");
}

#[test]
fn book_secondary_output_unused() {
    let (primary, secondary) = run_streams(&["hole"], &[], true);
    assert!(primary.is_empty());
    assert!(secondary.is_empty());
}

#[test]
fn book_must_not_be_first() {
    let err = run_spec("runpipe | hole").unwrap_err();
    assert!(
        err.to_string().contains("must not be a first stage"),
        "{err}"
    );
}

#[test]
fn book_last_stage_ignores_write() {
    // p.483 example is last-stage RUNPIPE; write errors are ignored (p.482).
    run_spec("literal nosuch | runpipe").unwrap();
}

#[test]
fn book_msglevel_accepted() {
    run_spec("literal hole | runpipe msglevel 4").unwrap();
    run_spec("literal hole | runpipe msgl xff").unwrap();
    run_spec("literal hole | runpipe msglevel 1 trace | hole").unwrap();
}

#[test]
fn rejects_invented_operands() {
    let err = run_spec("literal a | runpipe forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
    let err = run_spec("literal a | runpipe trace msglevel 1 | hole").unwrap_err();
    assert!(
        err.to_string()
            .to_ascii_uppercase()
            .contains("UNRECOGNIZED OPERAND MSGLEVEL"),
        "{err}"
    );
    let err = run_spec("literal a | runpipe anyeof | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
    let err = run_spec("literal a | runpipe mask 0 | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized operand mask"),
        "{err}"
    );
}

#[test]
fn events_is_not_implemented() {
    let err = run_spec("literal a | runpipe events | hole").unwrap_err();
    assert!(
        err.to_string().contains("EVENTS is not implemented"),
        "{err}"
    );
    let err = run_spec("literal a | runpipe events mask 00ff | hole").unwrap_err();
    assert!(
        err.to_string().contains("EVENTS is not implemented"),
        "{err}"
    );
}

#[test]
fn verb_is_all_caps() {
    assert!(StageRegistry::default().contains("RUNPIPE"));
    assert!(!StageRegistry::default().contains("RUNPIP"));
    let err = run_spec("literal a | runpip | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

#[test]
fn extra_input_rejected() {
    let err = run_spec("(ENDCHAR ?) literal a | r: runpipe | hole ? literal b | r:").unwrap_err();
    assert!(err.to_string().contains("only the primary input"), "{err}");
}
