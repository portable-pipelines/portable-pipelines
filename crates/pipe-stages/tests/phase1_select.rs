use std::cell::Cell;
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

fn run_primary(records: &[&str], verb: &str, args: &[&str]) -> Vec<String> {
    run_split(records, verb, args).0
}

fn run_split(records: &[&str], verb: &str, args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let s = Rc::clone(&secondary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("missing {verb}")),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    let sink_s = b.add_stage("sink_s", sink(s));
    b.connect_primary(src, stage);
    b.connect(stage, StreamId::PRIMARY, sink_p, StreamId::PRIMARY);
    b.connect(stage, StreamId::SECONDARY, sink_s, StreamId::PRIMARY);
    b.run().unwrap();
    let primary = primary.borrow().clone();
    let secondary = secondary.borrow().clone();
    (primary, secondary)
}

#[test]
fn take_first_default_and_count() {
    let (p, s) = run_split(&["a", "b", "c", "d"], "TAKE", &[]);
    assert_eq!(p, vec!["a".to_string()]);
    assert_eq!(s, vec!["b".to_string(), "c".to_string(), "d".to_string()]);

    let (p, s) = run_split(&["a", "b", "c", "d"], "TAKE", &["2"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(s, vec!["c".to_string(), "d".to_string()]);

    let (p, s) = run_split(&["a", "b", "c"], "TAKE", &["FIRST", "2"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(s, vec!["c".to_string()]);

    let (p, _) = run_split(&["a", "b"], "TAKE", &["2", "FIRST"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn take_last_star_and_zero() {
    let (p, s) = run_split(&["a", "b", "c", "d"], "TAKE", &["LAST", "2"]);
    assert_eq!(p, vec!["c".to_string(), "d".to_string()]);
    assert_eq!(s, vec!["a".to_string(), "b".to_string()]);

    let (p, s) = run_split(&["a", "b", "c"], "TAKE", &["2", "LAST"]);
    assert_eq!(p, vec!["b".to_string(), "c".to_string()]);
    assert_eq!(s, vec!["a".to_string()]);

    let (p, s) = run_split(&["a", "b", "c"], "TAKE", &["*"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    assert_eq!(s, Vec::<String>::new());

    let (p, s) = run_split(&["a", "b"], "TAKE", &["0"]);
    assert_eq!(p, Vec::<String>::new());
    assert_eq!(s, vec!["a".to_string(), "b".to_string()]);

    let (p, _) = run_split(&["a", "b", "c"], "TAKE", &["LAST", "10"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
}

#[test]
fn drop_first_last_star_and_zero() {
    let (p, s) = run_split(&["a", "b", "c", "d"], "DROP", &["2"]);
    assert_eq!(p, vec!["c".to_string(), "d".to_string()]);
    assert_eq!(s, vec!["a".to_string(), "b".to_string()]);

    let (p, s) = run_split(&["a", "b", "c"], "DROP", &["LAST", "1"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(s, vec!["c".to_string()]);

    let (p, s) = run_split(&["a", "b"], "DROP", &["0"]);
    assert_eq!(p, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(s, Vec::<String>::new());

    let (p, s) = run_split(&["a", "b", "c"], "DROP", &["*"]);
    assert_eq!(p, Vec::<String>::new());
    assert_eq!(s, vec!["a".to_string(), "b".to_string(), "c".to_string()]);

    let (p, _) = run_split(&["a", "b", "c"], "DROP", &["FIRST"]);
    assert_eq!(p, vec!["b".to_string(), "c".to_string()]);
}

#[test]
fn take_first_severs_input_when_secondary_is_absent() {
    let reg = StageRegistry::default();
    let written = Rc::new(Cell::new(0u32));
    let count = Rc::clone(&written);
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let records = ["a", "b", "c", "d", "e"];
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in records {
                if ctx.write(Record::from(rec)).is_err() {
                    break;
                }
                count.set(count.get() + 1);
            }
        }),
    );
    let stage = b.add_stage("TAKE", reg.create("TAKE", vec!["1".into()]).unwrap());
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    assert_eq!(*primary.borrow(), vec!["a".to_string()]);
    assert_eq!(written.get(), 1);
}

#[test]
fn nlocate_inverse_and_null_string() {
    let (p, s) = run_split(&["ax", "b", "x", ""], "NLOCATE", &["/x/"]);
    assert_eq!(p, vec!["b".to_string(), "".to_string()]);
    assert_eq!(s, vec!["ax".to_string(), "x".to_string()]);

    let (p, s) = run_split(&["", "a", ""], "NLOCATE", &[]);
    assert_eq!(p, vec!["".to_string(), "".to_string()]);
    assert_eq!(s, vec!["a".to_string()]);

    let (p, _) = run_split(&["ax", "b"], "NOTLOCATE", &["/x/"]);
    assert_eq!(p, vec!["b".to_string()]);
}

#[test]
fn strip_sides_count_not_and_anycase() {
    assert_eq!(
        run_primary(&["  aa  "], "STRIP", &[]),
        vec!["aa".to_string()]
    );
    assert_eq!(
        run_primary(&["  aa  "], "STRIP", &["LEADING"]),
        vec!["aa  ".to_string()]
    );
    assert_eq!(
        run_primary(&["  aa  "], "STRIP", &["TRAILING"]),
        vec!["  aa".to_string()]
    );
    assert_eq!(
        run_primary(&["  aa  "], "STRIP", &["1"]),
        vec![" aa ".to_string()]
    );
    assert_eq!(
        run_primary(&["xxyzz"], "STRIP", &["ANYOF", "/xy/"]),
        vec!["zz".to_string()]
    );
    assert_eq!(
        run_primary(&["aaa"], "STRIP", &["LEADING", "ANYOF", "/a/", "2"]),
        vec!["a".to_string()]
    );
    assert_eq!(
        run_primary(&["AaA"], "STRIP", &["ANYCASE", "ANYOF", "/a/"]),
        vec!["".to_string()]
    );
    assert_eq!(
        run_primary(&["a b"], "STRIP", &["NOT"]),
        vec![" ".to_string()]
    );
    assert_eq!(
        run_primary(&["a.b."], "STRIP", &["TRAILING", "."]),
        vec!["a.b".to_string()]
    );
}

#[test]
fn bytes_and_string_are_rejected() {
    let err = run_spec("literal a | take bytes 2 | hole").unwrap_err();
    assert!(
        err.to_string().contains("BYTES is not implemented"),
        "{err}"
    );
    let err = run_spec("literal a | drop last bytes | hole").unwrap_err();
    assert!(
        err.to_string().contains("BYTES is not implemented"),
        "{err}"
    );
    run_spec("literal a | strip string /ab/ | hole").unwrap();
    run_spec("literal a | strip 0-9 | hole").unwrap();
}

#[test]
fn exit_pipe_take_nlocate_strip() {
    let mut records = Vec::new();
    for i in 0..25 {
        if i % 2 == 0 {
            records.push(format!("  e{i}  "));
        } else {
            records.push(format!("x{i}"));
        }
    }
    let refs: Vec<&str> = records.iter().map(String::as_str).collect();
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&refs));
    let take = b.add_stage("TAKE", reg.create("TAKE", vec!["20".into()]).unwrap());
    let nloc = b.add_stage(
        "NLOCATE",
        reg.create("NLOCATE", vec!["/x/".into()]).unwrap(),
    );
    let strip = b.add_stage("STRIP", reg.create("STRIP", vec![]).unwrap());
    let sink_p = b.add_stage("sink", sink(p));
    b.connect_primary(src, take);
    b.connect_primary(take, nloc);
    b.connect_primary(nloc, strip);
    b.connect_primary(strip, sink_p);
    b.run().unwrap();
    let expected: Vec<String> = (0..20)
        .filter(|i| i % 2 == 0)
        .map(|i| format!("e{i}"))
        .collect();
    assert_eq!(*primary.borrow(), expected);

    let path = std::env::temp_dir().join("cms-pipelines-phase1-exit.txt");
    let body = records.join("\n") + "\n";
    std::fs::write(&path, body).unwrap();
    let spec = format!(
        "disk {} | take 20 | nlocate /x/ | strip | hole",
        path.display()
    );
    run_spec(&spec).unwrap();
    let _ = std::fs::remove_file(&path);
}

#[test]
fn between_basic_two_patterns() {
    let (p, s) = run_split(
        &["a", "start", "b", "c", "end", "d"],
        "BETWEEN",
        &["/start/", "/end/"],
    );
    assert_eq!(
        p,
        vec![
            "start".to_string(),
            "b".to_string(),
            "c".to_string(),
            "end".to_string()
        ]
    );
    assert_eq!(s, vec!["a".to_string(), "d".to_string()]);
}

#[test]
fn between_repeated_ranges() {
    let (p, s) = run_split(
        &["a", "x", "1", "x", "b", "x", "2", "x", "c"],
        "BETWEEN",
        &["/x/", "/x/"],
    );
    assert_eq!(
        p,
        vec![
            "x".to_string(),
            "1".to_string(),
            "x".to_string(),
            "x".to_string(),
            "2".to_string(),
            "x".to_string()
        ]
    );
    assert_eq!(s, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
}

#[test]
fn between_never_closes() {
    let (p, s) = run_split(
        &["a", "start", "b", "c", "d"],
        "BETWEEN",
        &["/start/", "/end/"],
    );
    assert_eq!(
        p,
        vec![
            "start".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string()
        ]
    );
    assert_eq!(s, vec!["a".to_string()]);
}

#[test]
fn between_numeric_count() {
    let (p, s) = run_split(
        &["a", "start", "b", "c", "d", "e"],
        "BETWEEN",
        &["/start/", "3"],
    );
    assert_eq!(
        p,
        vec!["start".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(
        s,
        vec!["a".to_string(), "d".to_string(), "e".to_string()]
    );
}

#[test]
fn between_count_two_and_three() {
    // Author's Edition 1.1.11 p.272: numeric count must be 2 or larger.
    let (p, s) = run_split(
        &["a", "start", "b", "start", "c"],
        "BETWEEN",
        &["/start/", "2"],
    );
    assert_eq!(
        p,
        vec![
            "start".to_string(),
            "b".to_string(),
            "start".to_string(),
            "c".to_string()
        ]
    );
    assert_eq!(s, vec!["a".to_string()]);

    let (p, s) = run_split(
        &["a", "start", "b", "c", "d"],
        "BETWEEN",
        &["/start/", "3"],
    );
    assert_eq!(
        p,
        vec!["start".to_string(), "b".to_string(), "c".to_string()]
    );
    assert_eq!(s, vec!["a".to_string(), "d".to_string()]);
}

#[test]
fn between_anycase() {
    let (p, s) = run_split(
        &["a", "START", "b", "END", "c"],
        "BETWEEN",
        &["ANYCASE", "/start/", "/end/"],
    );
    assert_eq!(
        p,
        vec!["START".to_string(), "b".to_string(), "END".to_string()]
    );
    assert_eq!(s, vec!["a".to_string(), "c".to_string()]);
}

#[test]
fn between_secondary_unconnected() {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["a", "x", "b", "x", "c"]));
    let stage = b.add_stage(
        "BETWEEN",
        reg.create("BETWEEN", vec!["/x/".into(), "/x/".into()]).unwrap(),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    assert_eq!(
        *primary.borrow(),
        vec![
            "x".to_string(),
            "b".to_string(),
            "x".to_string(),
        ]
    );
}

#[test]
fn between_multistream_parsed() {
    let spec = "(ENDCHAR ?) literal a|literal start|literal b|literal c|literal end|literal d | fanin | B: between /start/ /end/ | console ? B: | console";
    let result = run_spec(spec);
    assert!(result.is_ok(), "Multistream BETWEEN spec should parse and run: {result:?}");
}

#[test]
fn between_error_no_operands() {
    let err = run_spec("literal a | between | hole").unwrap_err();
    assert!(
        err.to_string().contains("requires two operands"),
        "{err}"
    );
}

#[test]
fn between_error_one_operand() {
    let err = run_spec("literal a | between /x/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("requires two operands"),
        "{err}"
    );
}

#[test]
fn between_error_count_less_than_two() {
    // Author's Edition 1.1.11 p.272: "The number must be 2 or larger."
    let err = run_spec("literal a | between /x/ 0 | hole").unwrap_err();
    assert!(
        err.to_string().contains("count must be at least 2"),
        "{err}"
    );

    let err = run_spec("literal a | between /x/ 1 | hole").unwrap_err();
    assert!(
        err.to_string().contains("count must be at least 2"),
        "{err}"
    );
}
