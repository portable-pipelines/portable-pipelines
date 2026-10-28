//! Dedicated ADDRESS PIPELINE command tests on the buffered host.
//!
//! These do not call RexxStart, so each command can be its own `#[test]`.
//! Regina coverage lives in `lib.rs` (`prefix_rexx_keeps_leading_and_trailing_blanks`).

use pipe_core::{with_pipeline_message_sink, EofReport};
use pipe_stages::pipeline_cmd::CmdTarget;
use std::cell::RefCell;
use std::rc::Rc;

use crate::RexxPipeHost;

fn host(input: &[&str]) -> RexxPipeHost<'static> {
    RexxPipeHost::buffered(input.iter().map(|s| (*s).to_string()).collect())
}

fn rc(host: &mut RexxPipeHost<'_>, cmd: &str) -> i32 {
    host.dispatch(cmd).unwrap().rc
}

#[test]
fn peekto_assigns_without_consuming() {
    let mut h = host(&["first", "second"]);
    let r = h.dispatch("PEEKTO LINE").unwrap();
    assert_eq!(r.rc, 0);
    assert_eq!(r.assigns, vec![("LINE".into(), "first".into())]);
    let again = h.peekto().unwrap();
    assert_eq!(again.as_deref(), Some("first"));
    let r2 = h.dispatch("PEEKTO LINE").unwrap();
    assert_eq!(r2.assigns, vec![("LINE".into(), "first".into())]);
}

#[test]
fn peekto_then_readto_consumes() {
    let mut h = host(&["keep", "next"]);
    assert_eq!(rc(&mut h, "PEEKTO LINE"), 0);
    let r = h.dispatch("READTO LINE").unwrap();
    assert_eq!(r.rc, 0);
    assert_eq!(r.assigns, vec![("LINE".into(), "keep".into())]);
    let r = h.dispatch("PEEKTO LINE").unwrap();
    assert_eq!(r.assigns, vec![("LINE".into(), "next".into())]);
}

#[test]
fn peekto_repeated_returns_same_record() {
    let mut h = host(&["same"]);
    assert_eq!(h.peekto().unwrap().as_deref(), Some("same"));
    assert_eq!(h.peekto().unwrap().as_deref(), Some("same"));
    assert_eq!(h.peekto().unwrap().as_deref(), Some("same"));
    assert_eq!(h.dispatch("READTO X").unwrap().assigns[0].1, "same");
    assert_eq!(h.peekto().unwrap(), None);
}

#[test]
fn peekto_at_eof_is_rc_12() {
    let mut h = host(&[]);
    let r = h.dispatch("PEEKTO LINE").unwrap();
    assert_eq!(r.rc, 12);
    assert!(r.assigns.is_empty());
    assert_eq!(h.peekto().unwrap(), None);
}

#[test]
fn peekto_then_callpipe_star_sees_unread_records() {
    let mut h = host(&["hello", "world"]);
    assert_eq!(rc(&mut h, "PEEKTO LINE"), 0);
    h.callpipe("*: | xlate 1-* upper | *:").unwrap();
    assert_eq!(h.peekto().unwrap(), None);
    assert_eq!(
        h.into_output(),
        vec!["HELLO".to_string(), "WORLD".to_string()]
    );
}

#[test]
fn peekto_across_multiple_records() {
    let mut h = host(&["a", "b", "c"]);
    assert_eq!(h.dispatch("PEEKTO X").unwrap().assigns[0].1, "a");
    assert_eq!(h.dispatch("READTO X").unwrap().assigns[0].1, "a");
    assert_eq!(h.dispatch("PEEKTO X").unwrap().assigns[0].1, "b");
    assert_eq!(h.dispatch("READTO X").unwrap().assigns[0].1, "b");
    assert_eq!(h.dispatch("PEEKTO X").unwrap().assigns[0].1, "c");
    assert_eq!(h.dispatch("READTO X").unwrap().assigns[0].1, "c");
    assert_eq!(rc(&mut h, "PEEKTO X"), 12);
}

#[test]
fn peekto_rejects_extra_operand() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "PEEKTO LINE EXTRA"), -112);
}

#[test]
fn peekto_without_varname_still_sets_last_record() {
    let mut h = host(&["payload"]);
    let r = h.dispatch("PEEKTO").unwrap();
    assert_eq!(r.rc, 0);
    assert!(r.assigns.is_empty());
    let get = h.dispatch("GETRANGE 1-* PIECE").unwrap();
    assert_eq!(get.rc, 0);
    assert_eq!(get.assigns, vec![("PIECE".into(), "payload".into())]);
}

#[test]
fn readto_consumes_and_callpipe_skips_it() {
    let mut h = host(&["gone", "kept"]);
    assert_eq!(rc(&mut h, "READTO LINE"), 0);
    h.callpipe("*: | xlate 1-* upper | *:").unwrap();
    assert_eq!(h.into_output(), vec!["KEPT".to_string()]);
}

#[test]
fn readto_at_eof_is_rc_12() {
    let mut h = host(&["one"]);
    assert_eq!(rc(&mut h, "READTO X"), 0);
    assert_eq!(rc(&mut h, "READTO X"), 12);
}

#[test]
fn readto_rejects_extra_operand() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "READTO LINE EXTRA"), -112);
}

#[test]
fn addstream_assigns_next_number() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "ADDSTREAM"), 1);
    assert_eq!(rc(&mut h, "ADDSTREAM"), 2);
    assert_eq!(rc(&mut h, "ADDSTREAM OUTPUT"), 3);
    assert_eq!(rc(&mut h, "ADDSTREAM INPUT 9"), 9);
    assert_eq!(rc(&mut h, "STREAMNUM INPUT 9"), 9);
    assert_eq!(rc(&mut h, "STREAMSTATE OUTPUT 3"), 12);
}

#[test]
fn addstream_rejects_bad_operand() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "ADDSTREAM BOTH"), -164);
    assert_eq!(rc(&mut h, "ADDSTREAM foo"), -178);
    assert_eq!(rc(&mut h, "ADDSTREAM INPUT 1 extra"), -112);
    assert_eq!(rc(&mut h, "ADDSTREAM 1 extra"), -112);
}

#[test]
fn addstream_abbreviation_min_eight() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "ADDSTREA"), 1);
    assert_eq!(rc(&mut h, "ADDSTRE"), -7);
}

#[test]
fn begoutput_sets_implied_output() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "BEGOUTPUT"), 0);
    assert!(h.implied_output());
    assert_eq!(rc(&mut h, "BEGOUTPUT extra"), -112);
}

#[test]
fn eofreport_modes_and_bad_operand() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "EOFREPORT"), 0);
    assert!(matches!(h.eof_report(), EofReport::Current));
    assert_eq!(rc(&mut h, "EOFREPORT ANY"), 0);
    assert!(matches!(h.eof_report(), EofReport::Any));
    assert_eq!(rc(&mut h, "EOFREPORT ALL"), 0);
    assert!(matches!(h.eof_report(), EofReport::All));
    assert_eq!(rc(&mut h, "EOFREPORT CURRENT"), 0);
    assert_eq!(rc(&mut h, "EOFREPORT FOO"), -4);
    assert_eq!(rc(&mut h, "EOFREPORT CURRENT extra"), -112);
}

#[test]
fn eofreport_then_sever_input_readto_is_rc_8() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "EOFREPORT CURRENT"), 0);
    assert_eq!(rc(&mut h, "SEVER INPUT"), 0);
    assert_eq!(rc(&mut h, "READTO X"), 8);
    assert_eq!(rc(&mut h, "PEEKTO X"), 8);
}

#[test]
fn eofreport_off_sever_input_readto_is_rc_12() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SEVER INPUT"), 0);
    assert_eq!(rc(&mut h, "READTO X"), 12);
}

#[test]
fn getrange_from_named_scan_and_from_last_record() {
    let mut h = host(&["abcdef"]);
    let scan = h.dispatch("SCANRANGE RNG REST 2-4 leftover").unwrap();
    assert_eq!(scan.rc, 0);
    let get = h.dispatch("GETRANGE RNG PIECE abcdef").unwrap();
    assert_eq!(get.rc, 0);
    assert_eq!(get.assigns, vec![("PIECE".into(), "bcd".into())]);

    assert_eq!(rc(&mut h, "PEEKTO"), 0);
    let from_last = h.dispatch("GETRANGE 1-2 PIECE").unwrap();
    assert_eq!(from_last.rc, 0);
    assert_eq!(from_last.assigns, vec![("PIECE".into(), "ab".into())]);
}

#[test]
fn getrange_errors() {
    let mut h = host(&[]);
    assert_eq!(rc(&mut h, "GETRANGE"), -42);
    assert_eq!(rc(&mut h, "GETRANGE ONLY"), -42);
    assert_eq!(rc(&mut h, "GETRANGE NOSUCH PIECE abc"), 8);
    assert_eq!(rc(&mut h, "GETRANGE 1-* PIECE"), 12);
}

#[test]
fn issuemsg_writes_tokens_and_rejects_missing_number() {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&sink);
    with_pipeline_message_sink(sink, || {
        let mut h = host(&["x"]);
        assert_eq!(rc(&mut h, "ISSUEMSG 112 MODULE token"), 0);
        assert_eq!(rc(&mut h, "ISSUEMSG"), -42);
        assert_eq!(rc(&mut h, "ISSUEMSG notanumber"), -42);
    });
    let lines = captured.borrow().clone();
    assert!(
        lines
            .iter()
            .any(|s| s.contains("112") && s.contains("MODULE")),
        "{lines:?}"
    );
}

#[test]
fn maxstream_primary_and_errors() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "MAXSTREAM OUTPUT"), 0);
    assert_eq!(rc(&mut h, "MAXSTREAM INPUT"), 0);
    assert_eq!(rc(&mut h, "MAXSTREAM"), -163);
    assert_eq!(rc(&mut h, "MAXSTREAM BOTH"), -164);
    assert_eq!(rc(&mut h, "MAXSTREAM OUTPUT extra"), -112);
    assert_eq!(rc(&mut h, "MAXSTREAM foo"), -164);
    assert_eq!(rc(&mut h, "ADDSTREAM OUTPUT"), 1);
    assert_eq!(rc(&mut h, "MAXSTREAM OUTPUT"), 0);
}

#[test]
fn message_is_rc_zero_including_empty() {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&sink);
    with_pipeline_message_sink(sink, || {
        let mut h = host(&["x"]);
        assert_eq!(rc(&mut h, "MESSAGE hello there"), 0);
        assert_eq!(rc(&mut h, "MESSAGE"), 0);
    });
    let lines = captured.borrow().clone();
    assert!(lines.iter().any(|s| s.contains("hello there")), "{lines:?}");
}

#[test]
fn nocommit_accepts_and_rejects_extra() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "NOCOMMIT"), 0);
    assert_eq!(rc(&mut h, "NOCOMMIT extra"), -112);
}

#[test]
fn resolve_builtin_missing_and_errors() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "RESOLVE COPY"), 1);
    assert_eq!(rc(&mut h, "RESOLVE JEREMY"), 0);
    assert_eq!(rc(&mut h, "RESOLVE"), -42);
    assert_eq!(rc(&mut h, "RESOLVE COPY extra"), -112);
}

#[test]
fn scanrange_parses_and_returns_remainder() {
    let mut h = host(&["x"]);
    let r = h.dispatch("SCANRANGE RNG REST 2-4 leftover words").unwrap();
    assert_eq!(r.rc, 0);
    assert!(r.assigns.iter().any(|(k, v)| k == "RNG" && v == "2-4"));
    assert!(r
        .assigns
        .iter()
        .any(|(k, v)| k == "REST" && v == "leftover words"));
}

#[test]
fn scanrange_errors() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SCANRANGE"), -42);
    assert_eq!(rc(&mut h, "SCANRANGE ONLY"), -42);
    assert_eq!(rc(&mut h, "SCANRANGE A B"), 8);
    assert_eq!(rc(&mut h, "SCANRANGE A B not-a-range leftover"), 8);
}

#[test]
fn scanstring_delimited_and_errors() {
    let mut h = host(&["x"]);
    let r = h.dispatch("SCANSTRING S R /hi/ there").unwrap();
    assert_eq!(r.rc, 0);
    assert!(r.assigns.iter().any(|(k, v)| k == "S" && v == "hi"));
    assert!(r.assigns.iter().any(|(k, v)| k == "R" && v == "there"));
    assert_eq!(rc(&mut h, "SCANSTRING"), -42);
    assert_eq!(rc(&mut h, "SCANSTRING ONLY"), -42);
    assert_eq!(rc(&mut h, "SCANSTRING S R undelimited"), 8);
}

#[test]
fn setrc_stores_stage_rc_and_rejects_bad_arg() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SETRC 9"), 0);
    assert_eq!(h.stage_rc, Some(9));
    assert_eq!(rc(&mut h, "SETRC 0"), 0);
    assert_eq!(h.stage_rc, Some(0));
    assert_eq!(rc(&mut h, "SETRC"), -2147483648);
    assert_eq!(rc(&mut h, "SETRC foo"), -2147483648);
    assert_eq!(rc(&mut h, "SETRC 1 extra"), -2147483648);
}

#[test]
fn sever_output_then_output_is_rc_12() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SEVER OUTPUT"), 0);
    assert_eq!(rc(&mut h, "OUTPUT after"), 12);
}

#[test]
fn sever_errors() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SEVER"), -163);
    assert_eq!(rc(&mut h, "SEVER BOTH"), -164);
    assert_eq!(rc(&mut h, "SEVER OUTPUT extra"), -112);
    assert_eq!(rc(&mut h, "SEVER foo"), -164);
}

#[test]
fn stagenum_is_one_and_rejects_operand() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "STAGENUM"), 1);
    assert_eq!(rc(&mut h, "STAGENUM extra"), -112);
}

#[test]
fn streamnum_selected_defined_and_errors() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "STREAMNUM INPUT"), 0);
    assert_eq!(rc(&mut h, "STREAMNUM OUTPUT *"), 0);
    assert_eq!(rc(&mut h, "STREAMNUM INPUT 0"), 0);
    assert_eq!(rc(&mut h, "STREAMNUM INPUT 9"), -102);
    assert_eq!(rc(&mut h, "STREAMNUM"), -163);
    assert_eq!(rc(&mut h, "STREAMNUM BOTH"), -164);
    assert_eq!(rc(&mut h, "STREAMNUM INPUT extra extra"), -112);
    assert_eq!(rc(&mut h, "STREAMNUM INPUT foo"), -178);
    assert_eq!(rc(&mut h, "STREAMNUM foo"), -164);
}

#[test]
fn streamstate_connected_missing_and_summary() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "STREAMSTATE OUTPUT"), 0);
    assert_eq!(rc(&mut h, "STREAMSTATE INPUT *"), 0);
    assert_eq!(rc(&mut h, "STREAMSTATE OUTPUT 1"), -4);
    assert_eq!(rc(&mut h, "STREAMSTATE SUMMARY"), 0);
    assert_eq!(rc(&mut h, "STREAMSTATE"), -163);
    assert_eq!(rc(&mut h, "STREAMSTATE BOTH"), -164);
    assert_eq!(rc(&mut h, "STREAMSTATE OUTPUT extra extra"), -112);
    assert_eq!(rc(&mut h, "STREAMSTATE INPUT foo"), -4);
    assert_eq!(rc(&mut h, "STREAMSTATE SUMMARY extra"), -112);
}

#[test]
fn streamstate_all_fills_stem() {
    let mut h = host(&["x"]);
    let r = h.dispatch("STREAMSTATE ALL ST").unwrap();
    assert_eq!(r.rc, 0);
    assert!(
        r.assigns
            .iter()
            .any(|(k, v)| k == "ST.0" && v.parse::<i32>().unwrap() >= 1),
        "{r:?}"
    );
    assert!(
        r.assigns
            .iter()
            .any(|(k, v)| k == "ST.1" && v.contains("INPUT")),
        "{r:?}"
    );
    assert_eq!(rc(&mut h, "STREAMSTATE ALL"), -112);
    assert_eq!(rc(&mut h, "STREAMSTATE ALL ST extra"), -112);
}

#[test]
fn suspend_is_noop_in_buffered_host() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SUSPEND"), 0);
    assert_eq!(rc(&mut h, "SUSPEND extra"), -112);
}

#[test]
fn select_both_anyinput_and_errors() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SELECT BOTH 0"), 0);
    assert_eq!(rc(&mut h, "SELECT 0"), 0);
    assert_eq!(rc(&mut h, "SELECT INPUT 0"), 0);
    assert_eq!(rc(&mut h, "SELECT OUTPUT 0"), 0);
    assert_eq!(rc(&mut h, "SELECT ANYINPUT"), 0);
    assert_eq!(rc(&mut h, "SELECT"), -169);
    assert_eq!(rc(&mut h, "SELECT INPUT"), -169);
    assert_eq!(rc(&mut h, "SELECT 99"), 4);
    assert_eq!(rc(&mut h, "SELECT INPUT 0 extra"), -112);
    assert_eq!(rc(&mut h, "SELECT ANYINPUT INPUT"), -168);
    assert_eq!(rc(&mut h, "SELECT ANYINPUT foo"), -112);
    assert_eq!(rc(&mut h, "SELECT IN 0"), 0);
    assert_eq!(rc(&mut h, "SELECT OUT 0"), 0);
}

#[test]
fn select_anyinput_at_eof_is_12() {
    let mut h = host(&[]);
    assert_eq!(rc(&mut h, "SELECT ANYINPUT"), 12);
}

#[test]
fn commit_zero_and_bad_arg() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "COMMIT 0"), 0);
    assert_eq!(rc(&mut h, "COMMIT"), -2147483648);
    assert_eq!(rc(&mut h, "COMMIT foo"), -2147483648);
    assert_eq!(rc(&mut h, "COMMIT 0 extra"), -2147483648);
}

#[test]
fn short_rejects_operand() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "SHORT extra"), -112);
    assert_eq!(rc(&mut h, "SHORT"), 0);
    assert_eq!(rc(&mut h, "READTO X"), 12);
}

#[test]
fn unknown_command_is_minus_7() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "NOSUCH"), -7);
    assert_eq!(rc(&mut h, "PEEK"), -7);
}

#[test]
fn rexx_command_requires_a_path() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "REXX"), -42);
}

#[test]
fn eofreport_then_sever_output_is_rc_8() {
    let mut h = host(&["x"]);
    assert_eq!(rc(&mut h, "EOFREPORT CURRENT"), 0);
    assert_eq!(rc(&mut h, "SEVER OUTPUT"), 0);
    assert_eq!(rc(&mut h, "OUTPUT x"), 8);
}
