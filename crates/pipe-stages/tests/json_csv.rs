//! JSON and CSV portable extensions, plus `$name` in SPEC and PICK.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{run_spec, StageRegistry};

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

fn err_of(spec: &str) -> String {
    run_spec(spec).unwrap_err().to_string()
}

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

fn run_both(records: &[&str], args: &[&str]) -> (Vec<String>, Vec<String>) {
    let reg = StageRegistry::default();
    let primary = Rc::new(RefCell::new(Vec::new()));
    let secondary = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(records));
    let stage = b.add_stage(
        "PICK",
        reg.create("PICK", args.iter().map(|a| (*a).to_string()).collect())
            .expect("PICK"),
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

#[test]
fn registry_has_json_and_csv() {
    let reg = StageRegistry::default();
    assert!(reg.contains("JSON"));
    assert!(reg.contains("CSV"));
    assert!(reg.contains("json"));
    assert!(reg.contains("csv"));
}

#[test]
fn json_add_then_spec_and_pick() {
    let out = run_spec_collect(
        r#"literal {"customer":"Ann","qty":3,"price":40} | json add total = $qty * $price | pick $total > /100/ | spec $customer 1 / spent / next $total next"#,
    );
    assert_eq!(out, vec!["Ann spent 120"]);
}

#[test]
fn json_set_overwrite_and_get() {
    let out = run_spec_collect(r#"literal {"n":1} | json set n = $n + 4 | json get n"#);
    assert_eq!(out, vec!["5"]);
}

#[test]
fn json_add_existing_is_an_error() {
    let err = err_of(r#"literal {"n":1} | json add n = 2 | hole"#);
    assert!(err.contains("already exists"), "{err}");
    assert!(err.contains("JSON:"), "{err}");
}

#[test]
fn json_delete_keep_and_missing_get() {
    let out = run_spec_collect(r#"literal {"a":1,"b":2,"c":3} | json delete b | json keep a c"#);
    let v: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    assert_eq!(v["a"], 1);
    assert_eq!(v["c"], 3);
    assert!(v.get("b").is_none());
    let err = err_of(r#"literal {"a":1} | json get missing | hole"#);
    assert!(err.contains("missing"), "{err}");
}

#[test]
fn json_string_concat_and_nested() {
    let out = run_spec_collect(
        r#"literal {"user":{"name":"Ann"},"city":"Oslo"} | json add greet = "hi " + $user.name | spec $greet 1 / in / next $city next"#,
    );
    assert_eq!(out, vec!["hi Ann in Oslo"]);
}

#[test]
fn json_preserves_nested_object_on_passthrough() {
    let out = run_spec_collect(
        r#"literal {"user":{"name":"Ann","tags":["a","b"]},"n":1} | copy | json get user"#,
    );
    let v: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    assert_eq!(v["name"], "Ann");
    assert_eq!(v["tags"][1], "b");
}

#[test]
fn json_unicode_and_empty_string() {
    let out = run_spec_collect(
        r#"literal {"name":"Année","note":""} | spec $name 1 /:/ next $note next"#,
    );
    assert_eq!(out, vec!["Année:"]);
}

#[test]
fn json_non_object_is_a_clear_error() {
    for spec in [
        r#"literal hello | json get name | hole"#,
        r#"literal [1,2] | json add n = 1 | hole"#,
        r#"literal hello | spec $name 1 | hole"#,
        r#"literal hello | pick $name == /x/ | hole"#,
    ] {
        let err = err_of(spec);
        assert!(
            err.contains("not a JSON object"),
            "expected object error for {spec}: {err}"
        );
        assert!(!err.to_ascii_lowercase().contains("panic"), "{err}");
    }
}

#[test]
fn json_large_integer_stays_integer() {
    let out = run_spec_collect(r#"literal {"n":9007199254740993} | json get n"#);
    assert_eq!(out, vec!["9007199254740993"]);
}

#[test]
fn json_integer_vs_float() {
    let out = run_spec_collect(r#"literal {"a":1,"b":1.5} | json add c = $a * $b | json get c"#);
    assert_eq!(out, vec!["1.5"]);
    let whole = run_spec_collect(r#"literal {"a":2,"b":5} | json add c = $a * $b | json get c"#);
    assert_eq!(whole, vec!["10"]);
}

#[test]
fn pick_json_number_does_not_need_text_parse() {
    let rec = r#"{"total":120}"#;
    let (p, s) = run_both(&[rec], &["$total", ">", "/100/"]);
    assert_eq!(p, vec![rec]);
    assert!(s.is_empty());
}

#[test]
fn pick_json_string_compare() {
    let east = r#"{"region":"East"}"#;
    let west = r#"{"region":"West"}"#;
    let (p, s) = run_both(&[east, west], &["$region", "==", "/East/"]);
    assert_eq!(p, vec![east]);
    assert_eq!(s, vec![west]);
}

#[test]
fn pick_missing_field_is_not_selected() {
    let rec = r#"{"region":"East"}"#;
    let (p, s) = run_both(&[rec], &["$amount", ">", "/0/"]);
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(s, vec![rec]);
}

#[test]
fn spec_missing_field_is_empty() {
    let out = run_spec_collect(r#"literal {"name":"Ann"} | spec $name 1 /:/ next $missing next"#);
    assert_eq!(out, vec!["Ann:"]);
}

#[test]
fn spec_plain_text_without_dollar_is_unchanged() {
    let out = run_spec_collect("literal hello| spec 1-* 1 /*/ next");
    assert_eq!(out, vec!["hello*"]);
}

#[test]
fn copy_passes_json_text_through() {
    let src = r#"{"a":1}"#;
    let out = run_spec_collect(&format!("literal {src}| copy"));
    assert_eq!(out, vec![src]);
}

#[test]
fn csv_reader_detects_numbers_and_strings() {
    let out = run_spec_collect(
        "literal Ann,3,40,East| literal name,qty,price,region| csv | json get qty",
    );
    assert_eq!(out, vec!["3"]);
    let region = run_spec_collect(
        "literal Ann,3,40,East| literal name,qty,price,region| csv | json get region",
    );
    assert_eq!(region, vec!["East"]);
}

#[test]
fn csv_strings_keeps_digits_as_text() {
    let out = run_spec_collect("literal 007,3| literal code,n| csv strings | json get code");
    assert_eq!(out, vec!["007"]);
}

#[test]
fn csv_quoted_comma_and_quote() {
    let out = run_spec_collect(
        r#"strliteral /Ann,"x,y","say ""hi"""/| strliteral /name,note,msg/ | csv | spec $note 1 / :: / next $msg next"#,
    );
    assert_eq!(out, vec![r#"x,y :: say "hi""#]);
}

#[test]
fn csv_empty_field_and_unicode() {
    let out = run_spec_collect(
        "literal Année,,Oslo| literal name,note,city| csv | spec $name 1 /:/ next $note next /:/ next $city next",
    );
    assert_eq!(out, vec!["Année::Oslo"]);
}

#[test]
fn csv_leading_zeros_stay_strings() {
    let out = run_spec_collect("literal 007| literal zip| csv | json get zip");
    assert_eq!(out, vec!["007"]);
}

#[test]
fn csv_write_round_trip_and_quoting() {
    let out = run_spec_collect(r#"literal Ann,"x,y",3| literal name,note,qty| csv | csv write"#);
    assert_eq!(out, vec!["name,note,qty", r#"Ann,"x,y",3"#]);
}

#[test]
fn csv_write_rejects_plain_text() {
    let err = err_of("literal hello | csv write | hole");
    assert!(err.contains("not a JSON object"), "{err}");
}

#[test]
fn csv_quoted_newline_joins_records() {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&[r#"name,note"#, r#"Ann,"hello"#, r#"world""#]));
    let csv = b.add_stage(
        "CSV",
        StageRegistry::default().create("CSV", vec![]).expect("CSV"),
    );
    let spec = b.add_stage(
        "SPEC",
        StageRegistry::default()
            .create("SPEC", vec!["$note".into(), "1".into()])
            .expect("SPEC"),
    );
    let sink_p = b.add_stage("sink", sink(Rc::clone(&collected)));
    b.connect_primary(src, csv);
    b.connect_primary(csv, spec);
    b.connect_primary(spec, sink_p);
    b.run().unwrap();
    let out = collected.borrow().clone();
    assert_eq!(out, vec!["hello\nworld"]);
}

#[test]
fn csv_then_pick_and_spec() {
    let out = run_spec_collect(
        "literal Cam,200,East| literal Ben,80,West| literal Ann,120,East| literal name,amount,region| csv | pick $region == /East/ | spec $name 1 / / next $amount next",
    );
    assert_eq!(out, vec!["Ann 120", "Cam 200"]);
}
