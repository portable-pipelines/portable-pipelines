//! Python stages are resolved by `run.rs` (`.py` path), not the built-in registry.

use pipe_stages::run_spec;

#[test]
fn py_path_is_a_stage_verb() {
    let err = run_spec("literal hello|examples/upper.py|hole").unwrap_err();
    assert!(
        err.to_string().contains("Python") || err.to_string().contains("runtime"),
        "{err}"
    );
}
