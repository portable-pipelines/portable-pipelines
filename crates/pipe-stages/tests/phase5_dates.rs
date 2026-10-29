//! DATECONVERT, GREG2SEC, SEC2GREG, TIMESTAMP.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, with_unix_now, StageRegistry};

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

fn run_records(verb: &str, records: &[&str], args: &[&str]) -> Vec<String> {
    let (p, _) = run_split(verb, records, args);
    p
}

fn run_split(verb: &str, records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("{verb}")),
    );
    let sink_p = b.add_stage("p", sink(Rc::clone(&primary)));
    let sink_s = b.add_stage("s", sink(Rc::clone(&secondary)));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let p = primary.borrow().clone();
    let s = secondary.borrow().clone();
    (p, s)
}

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

#[test]
fn registry_has_date_stages() {
    let reg = StageRegistry::default();
    assert!(reg.contains("DATECONVERT"));
    assert!(reg.contains("GREG2SEC"));
    assert!(reg.contains("SEC2GREG"));
    assert!(reg.contains("TIMESTAMP"));
}

#[test]
fn greg2sec_epoch_and_before() {
    assert_eq!(
        run_spec_collect("literal 19700101000000 | greg2sec"),
        vec!["0"]
    );
    assert_eq!(
        run_records("GREG2SEC", &["1969-12-31 23:59:59"], &[]),
        vec!["-1"]
    );
    assert_eq!(
        run_records("GREG2SEC", &["1970-01-01T00:00:01"], &[]),
        vec!["1"]
    );
}

#[test]
fn sec2greg_is_converse() {
    assert_eq!(
        run_spec_collect("literal 0 | sec2greg"),
        vec!["19700101000000"]
    );
    assert_eq!(
        run_records("SEC2GREG", &["-1"], &[]),
        vec!["19691231235959"]
    );
    assert_eq!(
        run_spec_collect("literal 19700101000000 | greg2sec | sec2greg"),
        vec!["19700101000000"]
    );
}

#[test]
fn greg_trims_whole_record() {
    assert_eq!(
        run_records("GREG2SEC", &["  19700101000000  "], &[]),
        vec!["0"]
    );
    assert_eq!(
        run_records("SEC2GREG", &["  0  "], &[]),
        vec!["19700101000000"]
    );
}

#[test]
fn greg_empty_input_is_eof() {
    assert_eq!(run_records("GREG2SEC", &[], &[]), Vec::<String>::new());
    assert_eq!(run_records("SEC2GREG", &[], &[]), Vec::<String>::new());
}

#[test]
fn greg_invalid_goes_to_secondary_or_fails() {
    let (p, s) = run_split("GREG2SEC", &["not-a-date", "19700101000000"], &[]);
    assert_eq!(p, vec!["0"]);
    assert_eq!(s, vec!["not-a-date"]);
    let err = err_of("literal bogus | greg2sec | hole");
    assert!(err.contains("GREG2SEC"), "{err}");
    let err = err_of("literal 1.5 | sec2greg | hole");
    assert!(err.contains("SEC2GREG"), "{err}");
}

#[test]
fn greg_must_not_be_first_and_rejects_operands() {
    let err = err_of("greg2sec | hole");
    assert!(err.to_ascii_lowercase().contains("first"), "{err}");
    let err = err_of("sec2greg UTC | hole");
    assert!(err.contains("unrecognized operand"), "{err}");
}

#[test]
fn dateconvert_iso_to_ymd_and_julian() {
    assert_eq!(
        run_spec_collect("literal 2026-10-29 | dateconvert from iso to ymd"),
        vec!["20261029"]
    );
    assert_eq!(
        run_records(
            "DATECONVERT",
            &["2026-10-29 rest"],
            &["FROM", "ISO", "TO", "JULIAN"]
        ),
        vec!["2026302 rest"]
    );
    assert_eq!(
        run_records(
            "DATECONVERT",
            &["29/10/2026"],
            &["FROM", "EUROPEAN", "TO", "ISO"]
        ),
        vec!["2026-10-29"]
    );
    assert_eq!(
        run_records(
            "DATECONVERT",
            &["10/29/2026"],
            &["FROM", "USA", "TO", "YMD"]
        ),
        vec!["20261029"]
    );
    assert_eq!(
        run_records(
            "DATECONVERT",
            &["2026-10-29"],
            &["FROM", "ISO", "TO", "GREG"]
        ),
        vec!["20261029000000"]
    );
}

#[test]
fn dateconvert_leap_day_and_invalid() {
    assert_eq!(
        run_records(
            "DATECONVERT",
            &["2020-02-29"],
            &["FROM", "ISO", "TO", "YMD"]
        ),
        vec!["20200229"]
    );
    let (p, s) = run_split(
        "DATECONVERT",
        &["2026-02-30", "2026-10-29"],
        &["FROM", "ISO", "TO", "YMD"],
    );
    assert_eq!(p, vec!["20261029"]);
    assert_eq!(s, vec!["2026-02-30"]);
    let err = err_of("literal 2026-13-01 | dateconvert from iso to ymd | hole");
    assert!(err.contains("DATECONVERT"), "{err}");
}

#[test]
fn dateconvert_errors() {
    let err = err_of("dateconvert | hole");
    assert!(err.to_ascii_lowercase().contains("first"), "{err}");
    let err = err_of("literal 2026-10-29 | dateconvert from | hole");
    assert!(err.contains("FROM requires"), "{err}");
    let err = err_of("literal 2026-10-29 | dateconvert from iso to moon | hole");
    assert!(err.contains("unrecognized date format"), "{err}");
}

#[test]
fn timestamp_prefixes_injected_utc_clock() {
    with_unix_now(0, || {
        assert_eq!(
            run_spec_collect("literal hi| timestamp"),
            vec!["1970-01-01 00:00:00 hi"]
        );
        assert_eq!(
            run_records("TIMESTAMP", &[""], &[]),
            vec!["1970-01-01 00:00:00 "]
        );
        assert_eq!(
            run_records("TIMESTAMP", &["a", "b"], &["/|/"]),
            vec!["1970-01-01 00:00:00|a", "1970-01-01 00:00:00|b"]
        );
    });
    with_unix_now(1_793_277_000, || {
        assert_eq!(
            run_records("TIMESTAMP", &["x"], &[]),
            vec!["2026-10-29 12:30:00 x"]
        );
    });
}

#[test]
fn timestamp_empty_input_and_errors() {
    with_unix_now(0, || {
        assert_eq!(run_records("TIMESTAMP", &[], &[]), Vec::<String>::new());
    });
    let err = err_of("literal x | timestamp /a/ /b/ | hole");
    assert!(
        err.contains("unrecognized operand") || err.contains("TIMESTAMP"),
        "{err}"
    );
}

#[test]
fn date_stages_eof_after_last_record() {
    assert_eq!(
        run_records(
            "DATECONVERT",
            &["2026-10-29"],
            &["FROM", "ISO", "TO", "ISO"]
        ),
        vec!["2026-10-29"]
    );
}
