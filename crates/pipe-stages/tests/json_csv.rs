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

#[test]
fn spec_array_index_dot_and_brackets() {
    let rec = r#"{"items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}],"tags":["a","b"]}"#;
    let sku = run_spec_collect(&format!("literal {rec} | spec $items.0.sku 1"));
    assert_eq!(sku, vec!["NUT"]);
    let sku_b = run_spec_collect(&format!("literal {rec} | spec $items[1].sku 1"));
    assert_eq!(sku_b, vec!["BOLT"]);
    let last = run_spec_collect(&format!("literal {rec} | spec $tags.-1 1"));
    assert_eq!(last, vec!["b"]);
    let last_b = run_spec_collect(&format!("literal {rec} | spec $tags[-1] 1"));
    assert_eq!(last_b, vec!["b"]);
    let missing = run_spec_collect(&format!("literal {rec} | spec $items.9.sku 1"));
    assert_eq!(missing, vec![""]);
    let tags = run_spec_collect(&format!("literal {rec} | spec $tags 1"));
    assert_eq!(tags, vec![r#"["a","b"]"#]);
}

#[test]
fn pick_array_index_number_and_missing() {
    let rec = r#"{"items":[{"qty":2},{"qty":10}]}"#;
    let (p, s) = run_both(&[rec], &["$items.0.qty", ">", "/5/"]);
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(s, vec![rec]);
    let (p, s) = run_both(&[rec], &["$items.1.qty", ">", "/5/"]);
    assert_eq!(p, vec![rec]);
    assert!(s.is_empty());
    let (p, s) = run_both(&[rec], &["$items.9.qty", ">", "/0/"]);
    assert!(p.is_empty(), "{p:?}");
    assert_eq!(s, vec![rec]);
}

#[test]
fn json_get_and_add_use_array_index() {
    let rec = r#"{"items":[{"sku":"NUT","qty":2}]}"#;
    let sku = run_spec_collect(&format!("literal {rec} | json get items.0.sku"));
    assert_eq!(sku, vec!["NUT"]);
    let bump = run_spec_collect(&format!(
        "literal {rec} | json set items.0.qty = $items.0.qty + 1 | json get items.0.qty"
    ));
    assert_eq!(bump, vec!["3"]);
}

#[test]
fn json_add_slash_string_and_quoted_string() {
    let slash = run_spec_collect(
        r#"strliteral /{"bleh":"hi"}/ | json add shout = $bleh + /!/ | json get shout"#,
    );
    assert_eq!(slash, vec!["hi!"]);
    let quoted = run_spec_collect(
        r#"strliteral /{"bleh":"hi"}/ | json add shout = $bleh + "!" | json get shout"#,
    );
    assert_eq!(quoted, vec!["hi!"]);
    let div = run_spec_collect(
        r#"literal {"qty":8,"price":2} | json add n = $qty / $price | json get n"#,
    );
    assert_eq!(div, vec!["4"]);
}

#[test]
fn json_split_objects_and_keep() {
    let rec = r#"{"order":"A-1","items":[{"sku":"NUT","qty":2},{"sku":"BOLT","qty":10}]}"#;
    let skus = run_spec_collect(&format!("literal {rec} | json split items | spec $sku 1"));
    assert_eq!(skus, vec!["NUT", "BOLT"]);
    let kept = run_spec_collect(&format!(
        "literal {rec} | json split items keep | spec $order 1 / / next $sku next"
    ));
    assert_eq!(kept, vec!["A-1 NUT", "A-1 BOLT"]);
}

#[test]
fn json_split_scalars_empty_missing_and_non_array() {
    let scalars =
        run_spec_collect(r#"literal {"tags":["a","b"]} | json split tags | spec $value 1"#);
    assert_eq!(scalars, vec!["a", "b"]);
    let empty = run_spec_collect(r#"literal {"tags":[]} | json split tags"#);
    assert!(empty.is_empty(), "{empty:?}");
    let missing = err_of(r#"literal {"n":1} | json split tags | hole"#);
    assert!(missing.contains("missing"), "{missing}");
    let not_arr = err_of(r#"literal {"n":1} | json split n | hole"#);
    assert!(not_arr.contains("not a JSON array"), "{not_arr}");
}

#[test]
fn json_split_mixed_and_nested_arrays() {
    let mixed =
        run_spec_collect(r#"literal {"xs":[1,"café",{"k":true},null,[2,3]]} | json split xs"#);
    assert_eq!(mixed.len(), 5);
    let v0: serde_json::Value = serde_json::from_str(&mixed[0]).unwrap();
    assert_eq!(v0["value"], 1);
    let v1: serde_json::Value = serde_json::from_str(&mixed[1]).unwrap();
    assert_eq!(v1["value"], "café");
    let v2: serde_json::Value = serde_json::from_str(&mixed[2]).unwrap();
    assert_eq!(v2["k"], true);
    let v3: serde_json::Value = serde_json::from_str(&mixed[3]).unwrap();
    assert!(v3["value"].is_null());
    let v4: serde_json::Value = serde_json::from_str(&mixed[4]).unwrap();
    assert_eq!(v4["value"], serde_json::json!([2, 3]));
}

#[test]
fn json_collect_round_trip_with_split() {
    let rec = r#"{"items":[{"sku":"NUT"},{"sku":"BOLT"}]}"#;
    let out = run_spec_collect(&format!(
        "literal {rec} | json split items | json collect items | json get items"
    ));
    let v: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    assert_eq!(v, serde_json::json!([{"sku":"NUT"},{"sku":"BOLT"}]));
    let empty = run_spec_collect("literal {\"tags\":[]} | json split tags | json collect tags");
    let v: serde_json::Value = serde_json::from_str(&empty[0]).unwrap();
    assert_eq!(v, serde_json::json!({"tags":[]}));
}

#[test]
fn json_array_compact_and_pretty_and_scalars() {
    let compact =
        run_spec_collect(r#"literal [{"name":"Ann"},{"name":"Cam"}] | json array | spec $name 1"#);
    assert_eq!(compact, vec!["Ann", "Cam"]);
    let scalars = run_spec_collect(r#"literal [1,2,3] | json array | spec $value 1"#);
    assert_eq!(scalars, vec!["1", "2", "3"]);
    let nested = run_spec_collect(r#"literal [[1],[2,3]] | json array"#);
    let v0: serde_json::Value = serde_json::from_str(&nested[0]).unwrap();
    assert_eq!(v0["value"], serde_json::json!([1]));
    let empty = run_spec_collect("literal [] | json array");
    assert!(empty.is_empty(), "{empty:?}");
    let not_arr = err_of(r#"literal {"a":1} | json array | hole"#);
    assert!(not_arr.contains("not a JSON array"), "{not_arr}");
    let bad = err_of("literal [1,} | json array | hole");
    assert!(
        bad.contains("invalid JSON") || bad.contains("not a JSON"),
        "{bad}"
    );
}

#[test]
fn json_array_pretty_printed_records() {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        emit(&[
            "[",
            r#"  {"name":"Ann","city":"Oslo"},"#,
            r#"  {"name":"Cam","city":"Leeds"}"#,
            "]",
        ]),
    );
    let arr = b.add_stage(
        "JSON",
        StageRegistry::default()
            .create("JSON", vec!["ARRAY".into()])
            .expect("JSON"),
    );
    let spec = b.add_stage(
        "SPEC",
        StageRegistry::default()
            .create(
                "SPEC",
                vec![
                    "$name".into(),
                    "1".into(),
                    "/ /".into(),
                    "next".into(),
                    "$city".into(),
                    "next".into(),
                ],
            )
            .expect("SPEC"),
    );
    let sink_p = b.add_stage("sink", sink(Rc::clone(&collected)));
    b.connect_primary(src, arr);
    b.connect_primary(arr, spec);
    b.connect_primary(spec, sink_p);
    b.run().unwrap();
    assert_eq!(
        collected.borrow().clone(),
        vec!["Ann Oslo".to_string(), "Cam Leeds".to_string()]
    );
}

#[test]
fn json_array_unclosed_and_unicode() {
    let collected = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&["[", r#"  {"name":"Année"}"#]));
    let arr = b.add_stage(
        "JSON",
        StageRegistry::default()
            .create("JSON", vec!["ARRAY".into()])
            .expect("JSON"),
    );
    let sink_p = b.add_stage("sink", sink(Rc::clone(&collected)));
    b.connect_primary(src, arr);
    b.connect_primary(arr, sink_p);
    let err = b.run().unwrap_err().to_string();
    assert!(err.contains("unclosed"), "{err}");

    let uni = run_spec_collect(r#"literal [{"name":"Année"}] | json array | spec $name 1"#);
    assert_eq!(uni, vec!["Année"]);
}

#[test]
fn json_array_deep_nested_does_not_panic() {
    let deep = "[[[[]]]]";
    let out = run_spec_collect(&format!("literal {deep} | json array"));
    let v: serde_json::Value = serde_json::from_str(&out[0]).unwrap();
    assert!(v["value"].is_array());
}

#[test]
fn json_split_and_array_huge() {
    let n = 100_000;
    let items: Vec<serde_json::Value> = (0..n).map(|i| serde_json::json!({"n": i})).collect();
    let rec = serde_json::json!({"items": items.clone()});
    let rec_s = serde_json::to_string(&rec).unwrap();
    let collected = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&[&rec_s]));
    let split = b.add_stage(
        "JSON",
        StageRegistry::default()
            .create("JSON", vec!["SPLIT".into(), "items".into()])
            .expect("JSON"),
    );
    let sink_p = b.add_stage("sink", sink(Rc::clone(&collected)));
    b.connect_primary(src, split);
    b.connect_primary(split, sink_p);
    b.run().unwrap();
    assert_eq!(collected.borrow().len(), n);

    let arr_s = serde_json::to_string(&serde_json::Value::Array(items)).unwrap();
    let collected = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new();
    let src = b.add_stage("src", emit(&[&arr_s]));
    let arr = b.add_stage(
        "JSON",
        StageRegistry::default()
            .create("JSON", vec!["ARRAY".into()])
            .expect("JSON"),
    );
    let sink_p = b.add_stage("sink", sink(Rc::clone(&collected)));
    b.connect_primary(src, arr);
    b.connect_primary(arr, sink_p);
    b.run().unwrap();
    assert_eq!(collected.borrow().len(), n);
}

#[test]
fn json_non_object_still_errors_except_array_verb() {
    let err = err_of(r#"literal [1,2,3] | json get value | hole"#);
    assert!(err.contains("not a JSON object"), "{err}");
    let err = err_of(r#"literal hello | spec $name 1 | hole"#);
    assert!(err.contains("not a JSON object"), "{err}");
}

#[test]
fn json_index_preserves_large_int_and_unicode() {
    let rec = r#"{"items":[{"n":9007199254740993,"name":"café"}]}"#;
    let n = run_spec_collect(&format!("literal {rec} | json get items.0.n"));
    assert_eq!(n, vec!["9007199254740993"]);
    let name = run_spec_collect(&format!("literal {rec} | spec $items.0.name 1"));
    assert_eq!(name, vec!["café"]);
}
