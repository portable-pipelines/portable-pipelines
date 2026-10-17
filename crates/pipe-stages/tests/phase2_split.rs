use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
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
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let p = Rc::clone(&primary);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        verb,
        reg.create(verb, args.iter().map(|a| (*a).to_string()).collect())
            .unwrap_or_else(|| panic!("missing {verb}")),
    );
    let sink_p = b.add_stage("sink_p", sink(p));
    b.connect_primary(src, stage);
    b.connect_primary(stage, sink_p);
    b.run().unwrap();
    let result = primary.borrow().clone();
    result
}

#[test]
fn split_default_blanks() {
    let out = run_primary(&["one two three"], "SPLIT", &[]);
    assert_eq!(out, vec!["one", "two", "three"]);
    
    let out = run_primary(&["a  b   c"], "SPLIT", &[]);
    assert_eq!(out, vec!["a", "", "b", "", "", "c"]);
    
    let out = run_primary(&[" leading"], "SPLIT", &[]);
    assert_eq!(out, vec!["", "leading"]);
    
    let out = run_primary(&["trailing "], "SPLIT", &[]);
    assert_eq!(out, vec!["trailing", ""]);
}

#[test]
fn split_at_character() {
    let out = run_primary(&["a,b,c"], "SPLIT", &[","]);
    assert_eq!(out, vec!["a", "b", "c"]);
    
    let out = run_primary(&["x:y:z"], "SPLIT", &[":"]);
    assert_eq!(out, vec!["x", "y", "z"]);
    
    let out = run_primary(&["a.b"], "SPLIT", &["."]);
    assert_eq!(out, vec!["a", "b"]);
}

#[test]
fn split_at_delimited_string() {
    let out = run_primary(&["one::two::three"], "SPLIT", &["/::/", "AT"]);
    assert_eq!(out, vec!["one", "two", "three"]);
    
    let out = run_primary(&["a--b--c"], "SPLIT", &["/--/"]);
    assert_eq!(out, vec!["a", "b", "c"]);
    
    let out = run_primary(&["hello world"], "SPLIT", &["/ /"]);
    assert_eq!(out, vec!["hello", "world"]);
}

#[test]
fn split_anyof_set() {
    let out = run_primary(&["a,b;c:d"], "SPLIT", &["ANYOF", "/,:;/"]);
    assert_eq!(out, vec!["a", "b", "c", "d"]);
    
    let out = run_primary(&["one two|three"], "SPLIT", &["ANYOF", "/ |/"]);
    assert_eq!(out, vec!["one", "two", "three"]);
    
    let out = run_primary(&["a1b2c3"], "SPLIT", &["ANYOF", "/123/"]);
    assert_eq!(out, vec!["a", "b", "c", ""]);
}

#[test]
fn split_before_placement() {
    let out = run_primary(&["a,b,c"], "SPLIT", &[",", "BEFORE"]);
    assert_eq!(out, vec!["a", ",b", ",c"]);
    
    let out = run_primary(&["x::y::z"], "SPLIT", &["/::/", "BEFORE"]);
    assert_eq!(out, vec!["x", "::y", "::z"]);
    
    let out = run_primary(&["a b c"], "SPLIT", &["BEFORE"]);
    assert_eq!(out, vec!["a", " b", " c"]);
}

#[test]
fn split_after_placement() {
    let out = run_primary(&["a,b,c"], "SPLIT", &[",", "AFTER"]);
    assert_eq!(out, vec!["a,", "b,", "c"]);
    
    let out = run_primary(&["x::y::z"], "SPLIT", &["/::/", "AFTER"]);
    assert_eq!(out, vec!["x::", "y::", "z"]);
    
    let out = run_primary(&["a b c"], "SPLIT", &["AFTER"]);
    assert_eq!(out, vec!["a ", "b ", "c"]);
}

#[test]
fn split_with_count() {
    let out = run_primary(&["a b c d e"], "SPLIT", &["2"]);
    assert_eq!(out, vec!["a", "b", "c d e"]);
    
    let out = run_primary(&["a,b,c,d"], "SPLIT", &[",", "1"]);
    assert_eq!(out, vec!["a", "b,c,d"]);
    
    let out = run_primary(&["one two three four"], "SPLIT", &["3"]);
    assert_eq!(out, vec!["one", "two", "three", "four"]);
    
    let out = run_primary(&["x:y:z"], "SPLIT", &[":", "2"]);
    assert_eq!(out, vec!["x", "y", "z"]);
}

#[test]
fn split_anycase() {
    let out = run_primary(&["AaBbCc"], "SPLIT", &["ANYCASE", "ANYOF", "/b/"]);
    assert_eq!(out, vec!["Aa", "", "Cc"]);
    
    let out = run_primary(&["oneTWOthree"], "SPLIT", &["ANYCASE", "/TWO/"]);
    assert_eq!(out, vec!["one", "three"]);
    
    let out = run_primary(&["XxYyZz"], "SPLIT", &["ANYCASE", "/x/"]);
    assert_eq!(out, vec!["", "", "YyZz"]);
}

#[test]
fn split_empty_pieces() {
    let out = run_primary(&[",,"], "SPLIT", &[","]);
    assert_eq!(out, vec!["", "", ""]);
    
    let out = run_primary(&[",a,"], "SPLIT", &[","]);
    assert_eq!(out, vec!["", "a", ""]);
    
    let out = run_primary(&[""], "SPLIT", &[","]);
    assert_eq!(out, vec![""]);
}

#[test]
fn split_multiple_records() {
    let out = run_primary(&["a b", "c d", "e f"], "SPLIT", &[]);
    assert_eq!(out, vec!["a", "b", "c", "d", "e", "f"]);
    
    let out = run_primary(&["1,2", "3,4"], "SPLIT", &[","]);
    assert_eq!(out, vec!["1", "2", "3", "4"]);
}

#[test]
fn split_operand_errors() {
    let err = run_spec("literal a | split 0 | hole").unwrap_err();
    assert!(
        err.to_string().contains("count must be at least 1"),
        "{err}"
    );
    
    let err = run_spec("literal a | split /,/ /,/ | hole").unwrap_err();
    assert!(
        err.to_string().contains("duplicate delimiter"),
        "{err}"
    );
    
    let err = run_spec("literal a | split anycase anycase | hole").unwrap_err();
    assert!(
        err.to_string().contains("duplicate ANYCASE"),
        "{err}"
    );
    
    let err = run_spec("literal a | split before after | hole").unwrap_err();
    assert!(
        err.to_string().contains("duplicate placement"),
        "{err}"
    );
}

#[test]
fn split_end_to_end_through_parser() {
    let spec = "literal one,two,three | split , | hole";
    run_spec(spec).unwrap();
    
    let spec = "literal a::b::c | split /::/ | hole";
    run_spec(spec).unwrap();
    
    let spec = "literal a b c | split before | hole";
    run_spec(spec).unwrap();
}

#[test]
fn split_complex_combinations() {
    let out = run_primary(&["a,B,c"], "SPLIT", &[",", "BEFORE", "ANYCASE"]);
    assert_eq!(out, vec!["a", ",B", ",c"]);
    
    let out = run_primary(&["x y z w"], "SPLIT", &["2", "AFTER"]);
    assert_eq!(out, vec!["x ", "y ", "z w"]);
    
    let out = run_primary(&["a:b;c:d"], "SPLIT", &["ANYOF", "/:;/", "1"]);
    assert_eq!(out, vec!["a", "b;c:d"]);
}
