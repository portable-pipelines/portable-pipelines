//! PIPCMD — Author's Edition 1.1.11 pp.457–458
//! (pipeline commands Ch.21 pp.638–660).

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
        "PIPCMD",
        reg.create("PIPCMD", args.iter().map(|a| (*a).to_string()).collect())
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
fn book_output_writes_argument() {
    // p.648: OUTPUT writes the argument string; PIPCMD does not copy the command.
    assert_eq!(
        run_spec_collect("literal output hello| pipcmd"),
        vec!["hello"]
    );
}

#[test]
fn book_output_null_when_omitted() {
    assert_eq!(run_spec_collect("literal output | pipcmd"), vec![""]);
}

#[test]
fn book_output_keeps_blanks_after_verb() {
    // p.648: the record begins after the blank ending the verb.
    assert_eq!(run_primary(&["output  hi"], &[]), vec![" hi"]);
}

#[test]
fn does_not_copy_the_command_record() {
    assert_eq!(
        run_spec_collect("literal output hello| pipcmd"),
        vec!["hello"]
    );
    assert!(run_spec_collect("literal output hello| pipcmd")
        .iter()
        .all(|s| s != "output hello"));
}

#[test]
fn multiple_output_commands() {
    // LITERAL cascade is reverse order (p.413).
    assert_eq!(
        run_spec_collect("literal output two|literal output one| pipcmd"),
        vec!["one", "two"]
    );
}

#[test]
fn book_callpipe_drains_to_selected_output() {
    // p.457: CALLPIPE is the most useful command. `||` self-escapes `|` (p.114).
    assert_eq!(
        run_spec_collect("literal callpipe literal hello||*:| pipcmd"),
        vec!["hello"]
    );
}

#[test]
fn addpipe_is_buffering_callpipe() {
    assert_eq!(
        run_spec_collect("literal addpipe literal world||*:| pipcmd"),
        vec!["world"]
    );
}

#[test]
fn book_resolve_missing_program_is_rc_zero() {
    // p.458: RC 0 means the name is not a built-in. The book's sample name
    // was INSERT, then OVERLAY, then FILLUP; those are now registered.
    // JEREMY is out of scope and is never registered.
    assert_eq!(
        run_spec_with_return_code("literal resolve jeremy | pipcmd").unwrap(),
        0
    );
}

#[test]
fn book_resolve_builtin_is_nonzero() {
    // p.458 / p.651: a positive RC means the name resolved.
    let rc = run_spec_with_return_code("literal resolve copy | pipcmd").unwrap();
    assert!(rc > 0, "expected a positive stand-in entry point, got {rc}");
    let rc = run_spec_with_return_code("literal resolve insert | pipcmd").unwrap();
    assert!(
        rc > 0,
        "INSERT is a built-in; expected nonzero RC, got {rc}"
    );
}

#[test]
fn book_resolve_missing_word() {
    // p.651: -42 when the argument is missing.
    assert_eq!(
        run_spec_with_return_code("literal resolve | pipcmd").unwrap(),
        -42
    );
}

#[test]
fn unknown_command_is_rc_minus_7() {
    // p.639: RC -7 means the command processor cannot resolve the command.
    assert_eq!(
        run_spec_with_return_code("literal nosuchcmd | pipcmd").unwrap(),
        -7
    );
}

#[test]
fn negative_command_rc_stops_further_commands() {
    // p.458: terminates as soon as a negative return code is received.
    assert_eq!(
        run_spec_with_return_code("literal output later|literal nosuch | pipcmd").unwrap(),
        -7
    );
}

#[test]
fn max_of_nonnegative_return_codes() {
    // p.458: when nonnegative, RC is the maximum of the command RCs.
    // OUTPUT to a connected stream is 0; OUTPUT with no output is 12.
    assert_eq!(
        run_spec_with_return_code("literal output hello | pipcmd").unwrap(),
        12
    );
}

#[test]
fn output_rc_zero_when_consumed() {
    assert_eq!(
        run_spec_with_return_code("literal output hello | pipcmd | hole").unwrap(),
        0
    );
}

#[test]
fn rexx_only_commands_are_rejected() {
    // p.457 / p.639: not available to PIPCMD.
    for cmd in [
        "readto line",
        "peekto",
        "begoutput",
        "nocommit",
        "getrange a variable b",
        "scanrange a b c",
        "scanstring a b c",
        "streamstate all foo",
    ] {
        let err = run_spec(&format!("literal {cmd} | pipcmd | hole")).unwrap_err();
        assert!(
            err.to_string().contains("not available to PIPCMD"),
            "{cmd}: {err}"
        );
    }
}

#[test]
fn addstream_eofreport_issuemsg_setrc_are_honoured() {
    assert_eq!(
        run_spec_with_return_code("literal addstream | pipcmd | hole").unwrap(),
        1,
        "ADDSTREAM with no operand assigns stream 1"
    );
    assert_eq!(
        run_spec_with_return_code("literal eofreport current | pipcmd | hole").unwrap(),
        0
    );
    assert_eq!(
        run_spec_with_return_code("literal issuemsg 112 MODULE | pipcmd | hole").unwrap(),
        0
    );
    assert_eq!(
        run_spec_with_return_code("literal setrc 0 | pipcmd | hole").unwrap(),
        0
    );
    assert_eq!(
        run_spec_with_return_code("literal setrc 7 | pipcmd | hole").unwrap(),
        7,
        "SETRC 7 is the PIPCMD stage return code"
    );
    assert_eq!(
        run_spec_with_return_code("literal output hello|literal setrc 7 | pipcmd | hole").unwrap(),
        7,
        "SETRC 7 then a later OUTPUT RC 0 stays 7"
    );
}

#[test]
fn invented_stage_operands_are_rejected() {
    let err = run_spec("literal a | pipcmd forever | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
    let err = run_spec("literal a | pipcmd anyeof | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
    let err = run_spec("literal a | pipcmd stop forever | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized STOP operand"),
        "{err}"
    );
    let err = run_spec("literal a | pipcmd stop alleof extra | hole").unwrap_err();
    assert!(
        err.to_string().contains("unrecognized operand extra"),
        "{err}"
    );
}

#[test]
fn verb_is_not_abbreviated() {
    // p.457 diagram is PIPCMD all-caps.
    assert!(StageRegistry::default().contains("PIPCMD"));
    assert!(!StageRegistry::default().contains("PIPCM"));
    let err = run_spec("literal a | pipcm | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("PIPCM"),
        "{err}"
    );
}

#[test]
fn select_output_secondary() {
    let (primary, secondary) = run_streams(&["select output 1", "output sec"], &[], true);
    assert!(primary.is_empty(), "{primary:?}");
    assert_eq!(secondary, vec!["sec"]);
}

#[test]
fn streamstate_connected_output() {
    // p.658: 0 connected (wait-state 0/4/8 collapsed), 12 not connected, -4 not defined.
    assert_eq!(
        run_spec_with_return_code("literal streamstate output | pipcmd | hole").unwrap(),
        0
    );
    assert_eq!(
        run_spec_with_return_code("literal streamstate output 1 | pipcmd | hole").unwrap(),
        -4
    );
}

#[test]
fn maxstream_primary_only() {
    // p.646: 0 when the primary is the only stream.
    assert_eq!(
        run_spec_with_return_code("literal maxstream output | pipcmd | hole").unwrap(),
        0
    );
}

#[test]
fn stagenum_is_one_based() {
    // p.657: first stage is 1. `literal | pipcmd` → pipcmd is stage 2.
    assert_eq!(
        run_spec_with_return_code("literal stagenum | pipcmd").unwrap(),
        2
    );
}

#[test]
fn streamnum_selected_primary() {
    assert_eq!(
        run_spec_with_return_code("literal streamnum output | pipcmd | hole").unwrap(),
        0
    );
}

#[test]
fn commit_zero() {
    assert_eq!(
        run_spec_with_return_code("literal commit 0 | pipcmd | hole").unwrap(),
        0
    );
}

#[test]
fn suspend_is_accepted() {
    assert_eq!(
        run_spec_with_return_code("literal suspend | pipcmd | hole").unwrap(),
        0
    );
}

#[test]
fn message_is_rc_zero() {
    assert_eq!(
        run_spec_with_return_code("literal message pipcmd-test | pipcmd").unwrap(),
        0
    );
}

#[test]
fn sever_output_then_output_is_rc_12() {
    // After SEVER OUTPUT the next OUTPUT sees an unconnected stream (p.648 RC 12).
    assert_eq!(
        run_spec_with_return_code("literal output x|literal sever output | pipcmd | hole").unwrap(),
        12
    );
}

#[test]
fn stop_anyeof_after_take_one() {
    // p.457: STOP ANYEOF ends when any output is at EOF.
    assert_eq!(
        run_spec_collect(
            r#"literal output c|literal output b|literal output a| pipcmd stop anyeof | take 1"#
        ),
        vec!["a"]
    );
}

#[test]
fn stop_without_keyword_is_alleof() {
    assert_eq!(
        run_spec_collect("literal output hi| pipcmd stop"),
        vec!["hi"]
    );
}

#[test]
fn select_anyinput_sees_the_command_record() {
    // The command record is still in the primary slot, so ANYINPUT selects 0.
    assert_eq!(
        run_spec_with_return_code("literal select anyinput | pipcmd | hole").unwrap(),
        0
    );
}

#[test]
fn callpipe_unknown_stage_is_an_error() {
    let err = run_spec("literal callpipe nosuchstage | pipcmd | hole").unwrap_err();
    assert!(err.to_string().contains("unknown stage"), "{err}");
}

fn pipcmd_rc(command: &str) -> i32 {
    run_spec_with_return_code(&format!("literal {command} | pipcmd | hole")).unwrap()
}

#[test]
fn addstream_success_and_errors() {
    assert_eq!(pipcmd_rc("addstream"), 1);
    assert_eq!(pipcmd_rc("addstream output"), 1);
    assert_eq!(pipcmd_rc("addstream both"), -164);
    assert_eq!(pipcmd_rc("addstream foo"), -178);
    assert_eq!(pipcmd_rc("addstream input 1 extra"), -112);
}

#[test]
fn eofreport_success_and_errors() {
    assert_eq!(pipcmd_rc("eofreport"), 0);
    assert_eq!(pipcmd_rc("eofreport current"), 0);
    assert_eq!(pipcmd_rc("eofreport any"), 0);
    assert_eq!(pipcmd_rc("eofreport all"), 0);
    assert_eq!(pipcmd_rc("eofreport foo"), -4);
    assert_eq!(pipcmd_rc("eofreport current extra"), -112);
}

#[test]
fn issuemsg_success_and_errors() {
    assert_eq!(pipcmd_rc("issuemsg 1 PORTABLE token"), 0);
    assert_eq!(pipcmd_rc("issuemsg"), -42);
    assert_eq!(pipcmd_rc("issuemsg xyz"), -42);
}

#[test]
fn maxstream_success_and_errors() {
    assert_eq!(pipcmd_rc("maxstream output"), 0);
    assert_eq!(pipcmd_rc("maxstream input"), 0);
    assert_eq!(pipcmd_rc("maxstream"), -163);
    assert_eq!(pipcmd_rc("maxstream both"), -164);
    assert_eq!(pipcmd_rc("maxstream output extra"), -112);
}

#[test]
fn message_success() {
    assert_eq!(pipcmd_rc("message hello"), 0);
    assert_eq!(
        run_spec_with_return_code("literal message | pipcmd | hole").unwrap(),
        0
    );
}

#[test]
fn resolve_success_and_errors() {
    assert_eq!(pipcmd_rc("resolve copy"), 1);
    assert_eq!(pipcmd_rc("resolve jeremy"), 0);
    assert_eq!(pipcmd_rc("resolve"), -42);
    assert_eq!(pipcmd_rc("resolve copy extra"), -112);
}

#[test]
fn setrc_success_and_errors() {
    assert_eq!(pipcmd_rc("setrc 7"), 7);
    assert_eq!(pipcmd_rc("setrc 0"), 0);
    assert_eq!(pipcmd_rc("setrc"), -2147483648);
    assert_eq!(pipcmd_rc("setrc foo"), -2147483648);
    assert_eq!(pipcmd_rc("setrc 1 extra"), -2147483648);
}

#[test]
fn sever_success_and_errors() {
    assert_eq!(pipcmd_rc("sever output"), 0);
    assert_eq!(pipcmd_rc("sever"), -163);
    assert_eq!(pipcmd_rc("sever both"), -164);
    assert_eq!(pipcmd_rc("sever output extra"), -112);
    assert_eq!(
        run_spec_with_return_code("literal output x|literal sever output | pipcmd | hole").unwrap(),
        12
    );
}

#[test]
fn stagenum_success_and_errors() {
    assert_eq!(
        run_spec_with_return_code("literal stagenum | pipcmd").unwrap(),
        2
    );
    assert_eq!(pipcmd_rc("stagenum extra"), -112);
}

#[test]
fn streamnum_success_and_errors() {
    assert_eq!(pipcmd_rc("streamnum output"), 0);
    assert_eq!(pipcmd_rc("streamnum input *"), 0);
    assert_eq!(pipcmd_rc("streamnum input 9"), -102);
    assert_eq!(pipcmd_rc("streamnum"), -163);
    assert_eq!(pipcmd_rc("streamnum both"), -164);
    assert_eq!(pipcmd_rc("streamnum input foo"), -178);
    assert_eq!(pipcmd_rc("streamnum output extra extra"), -112);
}

#[test]
fn streamstate_success_and_errors() {
    assert_eq!(pipcmd_rc("streamstate output"), 0);
    assert_eq!(pipcmd_rc("streamstate output 1"), -4);
    assert_eq!(pipcmd_rc("streamstate summary"), 0);
    assert_eq!(pipcmd_rc("streamstate"), -163);
    assert_eq!(pipcmd_rc("streamstate both"), -164);
    assert_eq!(pipcmd_rc("streamstate input foo"), -4);
    assert_eq!(pipcmd_rc("streamstate summary extra"), -112);
    let err = run_spec("literal streamstate all foo | pipcmd | hole").unwrap_err();
    assert!(err.to_string().contains("not available to PIPCMD"), "{err}");
}

#[test]
fn suspend_success_and_errors() {
    assert_eq!(pipcmd_rc("suspend"), 0);
    assert_eq!(pipcmd_rc("suspend extra"), -112);
}

#[test]
fn select_success_and_errors() {
    assert_eq!(pipcmd_rc("select 0"), 0);
    assert_eq!(pipcmd_rc("select both 0"), 0);
    assert_eq!(pipcmd_rc("select input 0"), 0);
    assert_eq!(pipcmd_rc("select output 0"), 0);
    assert_eq!(pipcmd_rc("select anyinput"), 0);
    assert_eq!(pipcmd_rc("select"), -169);
    assert_eq!(pipcmd_rc("select input"), -169);
    assert_eq!(pipcmd_rc("select 99"), 4);
    assert_eq!(pipcmd_rc("select input 0 extra"), -112);
    assert_eq!(pipcmd_rc("select anyinput input"), -168);
    assert_eq!(pipcmd_rc("select in 0"), 0);
}

#[test]
fn commit_success_and_errors() {
    assert_eq!(pipcmd_rc("commit 0"), 0);
    assert_eq!(pipcmd_rc("commit"), -2147483648);
    assert_eq!(pipcmd_rc("commit foo"), -2147483648);
    assert_eq!(pipcmd_rc("commit 0 extra"), -2147483648);
}

#[test]
fn short_success_and_errors() {
    assert_eq!(pipcmd_rc("short extra"), -112);
    assert_eq!(pipcmd_rc("short"), 0);
}

#[test]
fn rexx_command_requires_a_path() {
    assert_eq!(pipcmd_rc("rexx"), -42);
}

#[test]
fn pipcmd_rejects_each_rexx_only_verb() {
    for cmd in [
        "peekto",
        "peekto line",
        "readto line",
        "begoutput",
        "nocommit",
        "getrange a variable b",
        "scanrange a b c",
        "scanstring a b c",
        "streamstate all foo",
    ] {
        let err = run_spec(&format!("literal {cmd} | pipcmd | hole")).unwrap_err();
        assert!(
            err.to_string().contains("not available to PIPCMD"),
            "{cmd}: {err}"
        );
    }
}
