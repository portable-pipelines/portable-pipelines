//! Variable pool stages without a live Regina program.

use pipe_stages::run_spec;

#[test]
fn stem_outside_rexx_pool_fails() {
    let err = run_spec("stem FOO. | hole").expect_err("STEM needs a REXX pool");
    let s = err.to_string();
    assert!(
        s.to_ascii_lowercase().contains("variable pool")
            || s.contains("must be called from a REXX program"),
        "{s}"
    );
}

#[test]
fn var_outside_rexx_pool_fails() {
    let err = run_spec("var FOO | hole").expect_err("VAR needs a REXX pool");
    let s = err.to_string();
    assert!(
        s.to_ascii_lowercase().contains("variable pool")
            || s.contains("must be called from a REXX program"),
        "{s}"
    );
}

#[test]
fn varfetch_outside_rexx_pool_fails() {
    let err = run_spec("varfetch FOO | hole").expect_err("VARFETCH needs a REXX pool");
    let s = err.to_string();
    assert!(
        s.to_ascii_lowercase().contains("variable pool")
            || s.contains("must be called from a REXX program"),
        "{s}"
    );
}

#[test]
fn rexxvars_outside_rexx_pool_fails() {
    let err = run_spec("rexxvars | hole").expect_err("REXXVARS needs a REXX pool");
    let s = err.to_string();
    assert!(
        s.to_ascii_lowercase().contains("variable pool")
            || s.contains("must be called from a REXX program"),
        "{s}"
    );
}

#[test]
fn vardrop_outside_rexx_pool_fails() {
    let err = run_spec("vardrop FOO | hole").expect_err("VARDROP needs a REXX pool");
    let s = err.to_string();
    assert!(
        s.to_ascii_lowercase().contains("variable pool")
            || s.contains("must be called from a REXX program"),
        "{s}"
    );
}

#[test]
fn vardrop_without_name_fails() {
    let err = run_spec("vardrop | hole").expect_err("VARDROP needs a name");
    let s = err.to_string();
    assert!(
        s.contains("variable name required") || s.to_ascii_lowercase().contains("variable pool"),
        "{s}"
    );
}
