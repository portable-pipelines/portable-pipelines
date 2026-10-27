//! UPDATE — Author's Edition 1.1.11 pp.576–577
//! (tutorial Figure 176 p.86; SEQ8 control cards as for CMS UPDATE).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

fn emit(records: impl IntoIterator<Item = impl AsRef<str>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    let records: Vec<String> = records.into_iter().map(|s| s.as_ref().to_string()).collect();
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

fn hole() -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx: &mut StageCtx| {
        while ctx.read().ok().flatten().is_some() {}
    })
}

struct Out {
    master: Vec<String>,
    log: Vec<String>,
    rc: i32,
}

fn run_update(
    master: impl IntoIterator<Item = impl AsRef<str>>,
    update: impl IntoIterator<Item = impl AsRef<str>>,
    args: &[&str],
) -> Out {
    let reg = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let s = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src_p = b.add_stage("master", emit(master));
    let src_s = b.add_stage("update", emit(update));
    let stage = b.add_stage(
        "UPDATE",
        reg.create(
            "UPDATE",
            args.iter().map(|a| (*a).to_string()).collect(),
        )
        .expect("UPDATE"),
    );
    let sink_p = b.add_stage("sink_p", sink(Rc::clone(&p)));
    let sink_s = b.add_stage("sink_s", sink(Rc::clone(&s)));
    b.connect(src_p, StreamId::PRIMARY, stage, StreamId::PRIMARY);
    b.connect(src_s, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    let rc = b.run_with_return_code().unwrap();
    let master = p.borrow().clone();
    let log = s.borrow().clone();
    Out { master, log, rc }
}

fn rec(seq: u64, body: &str) -> String {
    format!("{seq:08}{body}")
}

fn card(text: &str, seq: u64) -> String {
    let mut payload: String = text.chars().take(72).collect();
    while payload.len() < 72 {
        payload.push(' ');
    }
    payload.push_str(&format!("{seq:08}"));
    payload
}

#[test]
fn insert_delete_replace_seq8_style() {
    // Portable ASCII: sequence in columns 1-8 (book default is 73-80).
    let master = [
        rec(10000, "alpha"),
        rec(20000, "beta"),
        rec(30000, "gamma"),
        rec(40000, "delta"),
    ];
    let a = rec(10000, "alpha");
    let b = rec(20000, "beta");
    let g = rec(30000, "gamma");
    let d = rec(40000, "delta");
    let ins = rec(15000, "inserted");
    let neu = rec(40000, "delta-new");
    let update = [
        "./ * portable SEQ8-style update",
        "./ I 00010000",
        ins.as_str(),
        "./ D 00030000",
        "./ R 00040000",
        neu.as_str(),
    ];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(
        out.master,
        vec![a.clone(), ins.clone(), b.clone(), neu.clone()]
    );
    assert!(out.log.iter().any(|l| l.starts_with("./ I")), "{:?}", out.log);
    assert!(out.log.iter().any(|l| l.starts_with("./ D")), "{:?}", out.log);
    assert!(out.log.iter().any(|l| l.starts_with("./ R")), "{:?}", out.log);
    assert!(out.log.contains(&g), "deleted gamma in log {:?}", out.log);
    assert!(out.log.contains(&ins), "inserted record in log {:?}", out.log);
    let _ = (a, b, d);
}

#[test]
fn insert_at_start_seq_zero() {
    // CMS UPDATE SEQ8: `./ I 0` inserts before the first master record.
    let master = [rec(10000, "alpha"), rec(20000, "beta")];
    let head = rec(50, "head");
    let update = ["./ I 00000000", head.as_str()];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(
        out.master,
        vec![head, rec(10000, "alpha"), rec(20000, "beta")]
    );
}

#[test]
fn insert_after_with_dollar_numbering() {
    // `./ I 00010000 $ 00011000 1000` numbers the inserted record.
    let master = [rec(10000, "alpha"), rec(20000, "beta")];
    let update = [
        "./ I 00010000 $ 00011000 1000",
        "        extra",
    ];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(
        out.master,
        vec![rec(10000, "alpha"), rec(11000, "extra"), rec(20000, "beta")]
    );
}

#[test]
fn dollar_default_increment_from_seqno() {
    // CMS: `./ I 240000 $` → incr 100, start 240100.
    let master = [rec(240000, "keep"), rec(250000, "later")];
    let update = ["./ I 00240000 $", "        add"];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(
        out.master,
        vec![
            rec(240000, "keep"),
            rec(240100, "add"),
            rec(250000, "later")
        ]
    );
}

#[test]
fn delete_range() {
    let master = [
        rec(10000, "a"),
        rec(20000, "b"),
        rec(30000, "c"),
        rec(40000, "d"),
    ];
    let out = run_update(&master, &["./ D 00020000 00030000"], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.master, vec![rec(10000, "a"), rec(40000, "d")]);
    assert!(out.log.contains(&rec(20000, "b")));
    assert!(out.log.contains(&rec(30000, "c")));
}

#[test]
fn replace_range() {
    let master = [
        rec(10000, "a"),
        rec(20000, "b"),
        rec(30000, "c"),
        rec(40000, "d"),
    ];
    let n1 = rec(20000, "b1");
    let n2 = rec(25000, "b2");
    let update = ["./ R 00020000 00030000", n1.as_str(), n2.as_str()];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(
        out.master,
        vec![rec(10000, "a"), n1.clone(), n2.clone(), rec(40000, "d")]
    );
    assert!(out.log.contains(&rec(20000, "b")));
    assert!(out.log.contains(&rec(30000, "c")));
}

#[test]
fn delete_dollar_is_accepted() {
    let master = [rec(10000, "a"), rec(20000, "b")];
    let out = run_update(&master, &["./ D 00010000 $"], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.master, vec![rec(20000, "b")]);
}

#[test]
fn replace_one_with_several() {
    let master = [rec(10000, "a"), rec(20000, "b"), rec(30000, "c")];
    let n1 = rec(20000, "b1");
    let n2 = rec(20100, "b2");
    let update = ["./ R 00020000", n1.as_str(), n2.as_str()];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(
        out.master,
        vec![rec(10000, "a"), n1.clone(), n2.clone(), rec(30000, "c")]
    );
}

#[test]
fn empty_update_copies_master() {
    let master = [rec(10000, "a"), rec(20000, "b")];
    let out = run_update(&master, &[] as &[&str], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0);
    assert_eq!(out.master, vec![rec(10000, "a"), rec(20000, "b")]);
    assert!(out.log.is_empty(), "log {:?}", out.log);
}

#[test]
fn comment_copied_to_log() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./ * hello there"], &["1-8"]);
    assert_eq!(out.rc, 0);
    assert_eq!(out.master, vec![rec(10000, "a")]);
    assert_eq!(out.log, vec!["./ * hello there"]);
}

#[test]
fn long_comment_is_not_unsupported() {
    // `./ *` comments are freeform (CMS UPDATE columns 4–50); there is no
    // token-count cap in the Pipelines inventory.
    let master = [rec(10000, "a")];
    let card = "./ * portable SEQ8-style update (sequence in columns 1-8)";
    let out = run_update(&master, &[card], &["1-8"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.log, vec![card]);
}

#[test]
fn resequence_omitted_defaults() {
    // CMS UPDATE SEQ8: `./ S` with no operands starts at 1000; omitted
    // increment equals the start.
    let master = [rec(10000, "a"), rec(20000, "b")];
    let out = run_update(&master, &["./ S"], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.master, vec![rec(1000, "a"), rec(2000, "b")]);
}

#[test]
fn resequence_first_in_update() {
    let master = [rec(10000, "a"), rec(20000, "b")];
    let out = run_update(&master, &["./ S 1000 1000"], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.master, vec![rec(1000, "a"), rec(2000, "b")]);
}

#[test]
fn resequence_not_first_rc4() {
    let master = [rec(10000, "a"), rec(20000, "b")];
    let update = ["./ * note", "./ S 1000 1000"];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 4, "log {:?}", out.log);
    // Resequence suppressed: original numbers remain.
    assert_eq!(out.master, vec![rec(10000, "a"), rec(20000, "b")]);
    assert!(
        out.log.iter().any(|l| l.contains("resequence request that is not first")),
        "{:?}",
        out.log
    );
}

#[test]
fn resequence_start_not_numeric_rc8() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./ S abc 10"], &["1-8"]);
    assert_eq!(out.rc, 8, "log {:?}", out.log);
    assert!(
        out.log.iter().any(|l| l.contains("resequence start is not numeric")),
        "{:?}",
        out.log
    );
}

#[test]
fn first_flags_input_sequence_error() {
    let master = [rec(20000, "b"), rec(10000, "a")];
    // LAST skips the output-sequence check so the input error (RC=4) is the
    // highest code (without LAST, the same disorder is also RC=8 on output).
    let out = run_update(&master, &[] as &[&str], &["1-8", "FIRST", "LAST"]);
    assert_eq!(out.rc, 4, "log {:?}", out.log);
    assert!(
        out.log.iter().any(|l| l.contains("sequence error in the input master file")),
        "{:?}",
        out.log
    );
}

#[test]
fn without_first_input_sequence_is_not_checked() {
    let master = [rec(20000, "b"), rec(10000, "a")];
    let out = run_update(&master, &[] as &[&str], &["1-8", "LAST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.master, vec![rec(20000, "b"), rec(10000, "a")]);
}

#[test]
fn last_skips_output_sequence_check() {
    // An inserted record whose sequence is not after the previous output
    // number: LAST skips that check (p.576).
    let master = [rec(10000, "a"), rec(20000, "b")];
    let inserted = rec(5000, "x");
    let update = ["./ I 00010000", inserted.as_str()];
    let with_last = run_update(&master, &update, &["1-8", "FIRST", "LAST"]);
    assert_eq!(with_last.rc, 0, "log {:?}", with_last.log);
    assert_eq!(
        with_last.master,
        vec![rec(10000, "a"), rec(5000, "x"), rec(20000, "b")]
    );
    let without = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(without.rc, 8, "log {:?}", without.log);
    assert!(
        without
            .log
            .iter()
            .any(|l| l.contains("sequence error in the output master file")),
        "{:?}",
        without.log
    );
}

#[test]
fn sequence_number_not_found_rc12() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./ D 00090000"], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 12, "log {:?}", out.log);
    assert!(
        out.log.iter().any(|l| l.contains("sequence number 90000 not found")),
        "{:?}",
        out.log
    );
    assert_eq!(out.master, vec![rec(10000, "a")]);
}

#[test]
fn insert_dollar_sign_missing_rc12() {
    // Insert allows only one seqno before `$`.
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./ I 00010000 00011000"], &["1-8"]);
    assert_eq!(out.rc, 12, "log {:?}", out.log);
    assert!(
        out.log.iter().any(|l| l.contains("dollar sign is missing")),
        "{:?}",
        out.log
    );
}

#[test]
fn sequence_number_too_long_rc12() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./ I 123456789"], &["1-8"]);
    assert_eq!(out.rc, 12, "log {:?}", out.log);
    assert!(
        out.log
            .iter()
            .any(|l| l.contains("longer than the sequence field width")),
        "{:?}",
        out.log
    );
}

#[test]
fn unsupported_control_card_rc32() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./ Q 00010000"], &["1-8"]);
    assert_eq!(out.rc, 32, "log {:?}", out.log);
    assert!(
        out.log
            .iter()
            .any(|l| l.contains("unsupported or missing control card")),
        "{:?}",
        out.log
    );
}

#[test]
fn missing_control_card_rc32() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["not a control card"], &["1-8"]);
    assert_eq!(out.rc, 32, "log {:?}", out.log);
    assert!(
        out.log.iter().any(|l| l.contains("missing control card")),
        "{:?}",
        out.log
    );
}

#[test]
fn highest_return_code_wins() {
    let master = [rec(20000, "b"), rec(10000, "a")];
    let update = ["./ Q 1"];
    let out = run_update(&master, &update, &["1-8", "FIRST"]);
    assert_eq!(out.rc, 32, "log {:?}", out.log);
}

#[test]
fn comment_identifies_stage_in_log() {
    let master = [rec(10000, "a")];
    let out = run_update(
        &master,
        &["./ D 00090000"],
        &["1-8", "FIRST", "*", "MYUPD"],
    );
    assert_eq!(out.rc, 12);
    assert!(
        out.log.iter().any(|l| l.contains("MYUPD:") && l.contains("not found")),
        "{:?}",
        out.log
    );
}

#[test]
fn blank_sequence_copied_without_input_error() {
    let master = [
        rec(10000, "a"),
        "        blank-seq".to_string(),
        rec(20000, "b"),
    ];
    let out = run_update(&master, &[] as &[&str], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(
        out.master,
        vec![rec(10000, "a"), "        blank-seq".into(), rec(20000, "b")]
    );
}

#[test]
fn leading_zeros_match_numerically() {
    let master = [rec(10000, "a"), rec(20000, "b")];
    let out = run_update(&master, &["./ D 10000"], &["1-8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.master, vec![rec(20000, "b")]);
}

#[test]
fn default_columns_73_80() {
    let m1 = card("alpha", 10000);
    let m2 = card("beta", 20000);
    let ins = card("extra", 15000);
    let out = run_update(
        &[m1.as_str(), m2.as_str()],
        &["./ I 00010000", ins.as_str()],
        &["FIRST"],
    );
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert_eq!(out.master, vec![m1, ins, m2]);
}

#[test]
fn range_dot_length() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./ D 00010000"], &["1.8", "FIRST"]);
    assert_eq!(out.rc, 0, "log {:?}", out.log);
    assert!(out.master.is_empty());
}

#[test]
fn sequence_field_wider_than_15_rejected() {
    let err = run_spec("literal a | update 1-16 | hole").unwrap_err();
    assert!(
        err.to_string().contains("maximum length") && err.to_string().contains("15"),
        "{err}"
    );
}

#[test]
fn invented_operands_rejected() {
    for spec in [
        "literal a | update seq8 | hole",
        "literal a | update inc | hole",
        "literal a | update ctl | hole",
        "literal a | update noseq8 | hole",
        "literal a | update first 1-8 | hole",
        "literal a | update last first | hole",
        "literal a | update strict | hole",
        "literal a | update first last forever | hole",
    ] {
        let err = run_spec(spec).unwrap_err();
        assert!(
            err.to_string().contains("unrecognized operand")
                || err.to_string().contains("FIRST must precede LAST"),
            "{spec} → {err}"
        );
    }
}

#[test]
fn firs_is_not_an_abbreviation() {
    let err = run_spec("literal a | update firs | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn las_is_not_an_abbreviation() {
    let err = run_spec("literal a | update las | hole").unwrap_err();
    assert!(err.to_string().contains("unrecognized operand"), "{err}");
}

#[test]
fn verb_is_not_abbreviable() {
    let err = run_spec("literal a | updat | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown stage") && err.to_string().contains("UPDAT"),
        "{err}"
    );
}

#[test]
fn premature_termination_without_secondary_output() {
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["one", "two"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let upd = b.add_stage("upd", emit(&["./ D 1"]));
    let stage = b.add_stage("UPDATE", reg.create("UPDATE", vec!["1-8".into()]).unwrap());
    let h = b.add_stage("hole", hole());
    b.connect_primary(src, stage);
    b.connect(upd, StreamId::PRIMARY, stage, StreamId::SECONDARY);
    b.connect_primary(stage, h);
    b.run().unwrap();
    assert!(
        written.get() <= 1,
        "disconnected secondary output must not drain the producer, wrote {}",
        written.get()
    );
}

#[test]
fn premature_termination_without_any_output() {
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in ["one", "two"] {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("UPDATE", reg.create("UPDATE", vec![]).unwrap());
    b.connect_primary(src, stage);
    b.run().unwrap();
    assert_eq!(written.get(), 0);
}

#[test]
fn tertiary_stream_rejected() {
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a"]));
    let extra = b.add_stage("extra", emit(&["x"]));
    let stage = b.add_stage("UPDATE", reg.create("UPDATE", vec![]).unwrap());
    let h = b.add_stage("hole", hole());
    b.connect_primary(src, stage);
    b.connect(extra, StreamId::PRIMARY, stage, StreamId(2));
    b.connect_primary(stage, h);
    let err = b.run().unwrap_err();
    assert!(
        err.to_string().contains("primary and secondary"),
        "{err}"
    );
}

#[test]
fn cascade_first_then_last() {
    // Book example p.576: `|u1: update first |u2: update last`.
    let master = [rec(10000, "a"), rec(20000, "b"), rec(30000, "c")];
    let mid = rec(15000, "mid");
    let first_upd = ["./ I 00010000", mid.as_str()];
    let second_upd = ["./ D 00030000"];
    let spec = StageRegistry::default();
    let p = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("master", emit(&master));
    let u1src = b.add_stage("u1src", emit(&first_upd));
    let u2src = b.add_stage("u2src", emit(&second_upd));
    let u1 = b.add_stage(
        "U1",
        spec.create("UPDATE", vec!["1-8".into(), "FIRST".into()])
            .unwrap(),
    );
    let u2 = b.add_stage(
        "U2",
        spec.create("UPDATE", vec!["1-8".into(), "LAST".into()])
            .unwrap(),
    );
    let sink_p = b.add_stage("sink", sink(Rc::clone(&p)));
    let h1 = b.add_stage("h1", hole());
    let h2 = b.add_stage("h2", hole());
    b.connect(src, StreamId::PRIMARY, u1, StreamId::PRIMARY);
    b.connect(u1src, StreamId::PRIMARY, u1, StreamId::SECONDARY);
    b.connect(u1, StreamId::PRIMARY, u2, StreamId::PRIMARY);
    b.connect(u1, StreamId::SECONDARY, h1, StreamId::PRIMARY);
    b.connect(u2src, StreamId::PRIMARY, u2, StreamId::SECONDARY);
    b.connect(u2, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(u2, StreamId::SECONDARY, h2, StreamId::PRIMARY);
    b.run().unwrap();
    assert_eq!(
        *p.borrow(),
        vec![rec(10000, "a"), rec(15000, "mid"), rec(20000, "b")]
    );
}

#[test]
fn return_code_is_not_a_pipeline_error() {
    let rc = run_update(&[rec(10000, "a")], &["./ D 00090000"], &["1-8", "FIRST"]).rc;
    assert_eq!(rc, 12);
}

#[test]
fn control_without_blank_after_dotslash() {
    let master = [rec(10000, "a")];
    let out = run_update(&master, &["./I 00010000"], &["1-8"]);
    assert_eq!(out.rc, 32, "log {:?}", out.log);
}
