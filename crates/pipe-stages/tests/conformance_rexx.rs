//! REXX — Author's Edition 1.1.11 p.475 (Phase 5 subset).
//!
//! The verb is resolved by `run.rs` (`.rexx` / `.rex` path, or `REXX`), not the
//! built-in registry. Regina is preferred; a subset interpreter is the fallback.

use pipe_stages::run_spec;

#[test]
fn rexx_path_is_a_stage_verb() {
    // No hook is installed in this crate, so the stub runs and reports that.
    let err = run_spec("literal hello|examples/upper.rexx|hole").unwrap_err();
    assert!(
        err.to_string().contains("REXX") || err.to_string().contains("runtime"),
        "{err}"
    );
}

#[test]
fn rexx_keyword_is_accepted() {
    let err = run_spec("rexx | hole").unwrap_err();
    assert!(
        err.to_string().contains("REXX") || err.to_string().contains("runtime"),
        "{err}"
    );
}
