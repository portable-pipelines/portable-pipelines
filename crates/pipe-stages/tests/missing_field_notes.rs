//! Missing `$field` is still empty, and now names the stage that looked it up.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::with_pipeline_message_sink;
use pipe_stages::run_spec_with_return_code;

fn run_capturing(spec: &str) -> (Result<i32, String>, Vec<String>) {
    let sink = Rc::new(RefCell::new(Vec::new()));
    let result = with_pipeline_message_sink(sink.clone(), || {
        run_spec_with_return_code(spec).map_err(|e| e.to_string())
    });
    let notes = sink.borrow().clone();
    (result, notes)
}

#[test]
fn lookup_missing_owner_i_warns_and_still_matches_empty_keys() {
    let spec = r#"(ENDCHAR ?) literal {"owner_id":"O1","name":"Mara"} | v: lookup $owner_i | hole ? literal {"owner_id":"O1","name":"Ann"} | v:"#;
    let (result, notes) = run_capturing(spec);
    assert_eq!(result.unwrap(), 0);
    let joined = notes.join("\n");
    assert!(
        joined.contains("LOOKUP (stage 2): field $owner_i not found"),
        "{joined:?}"
    );
    assert!(joined.contains("owner_id, name"), "{joined:?}");
    assert!(joined.contains("did you mean owner_id?"), "{joined:?}");
}

#[test]
fn spec_missing_field_still_writes_empty_and_warns() {
    let spec = r#"literal {"owner_id":"O1"} | spec $owner_i 1 | hole"#;
    let (result, notes) = run_capturing(spec);
    assert_eq!(result.unwrap(), 0);
    let joined = notes.join("\n");
    assert!(
        joined.contains("SPEC (stage 2): field $owner_i not found"),
        "{joined:?}"
    );
}

#[test]
fn sort_missing_field_warns_without_failing() {
    let spec = r#"literal {"name":"b"} | literal {"name":"a"} | sort $missing | hole"#;
    let (result, notes) = run_capturing(spec);
    assert_eq!(result.unwrap(), 0);
    let joined = notes.join("\n");
    assert!(
        joined.contains("SORT") && joined.contains("field $missing not found"),
        "{joined:?}"
    );
}

#[test]
fn collate_missing_field_warns_without_failing() {
    let spec =
        r#"(endchar ?) literal {"id":"1"} | c: collate $nope | hole ? literal {"id":"1"} | c:"#;
    let (result, notes) = run_capturing(spec);
    assert_eq!(result.unwrap(), 0);
    let joined = notes.join("\n");
    assert!(
        joined.contains("COLLATE") && joined.contains("field $nope not found"),
        "{joined:?}"
    );
}

#[test]
fn present_field_does_not_warn() {
    let spec = r#"literal {"owner_id":"O1"} | spec $owner_id 1 | hole"#;
    let (result, notes) = run_capturing(spec);
    assert_eq!(result.unwrap(), 0);
    assert!(!notes.iter().any(|n| n.contains("not found")), "{notes:?}");
}

#[test]
fn json_get_still_errors_when_missing() {
    let spec = r#"literal {"owner_id":"O1"} | json get $owner_i | hole"#;
    let (result, notes) = run_capturing(spec);
    let err = result.expect_err("JSON GET must fail");
    assert!(err.contains("missing") || err.contains("JSON:"), "{err}");
    assert!(
        !notes.iter().any(|n| n.contains("did you mean")),
        "GET should error, not only warn: {notes:?}"
    );
}

#[test]
fn note_once_per_stage_not_once_per_record() {
    let spec = r#"literal {"a":1} | literal {"a":2} | spec $nope 1 | hole"#;
    let (result, notes) = run_capturing(spec);
    assert_eq!(result.unwrap(), 0);
    let hits: Vec<_> = notes
        .iter()
        .filter(|n| n.contains("field $nope not found"))
        .collect();
    assert_eq!(hits.len(), 1, "{notes:?}");
}
