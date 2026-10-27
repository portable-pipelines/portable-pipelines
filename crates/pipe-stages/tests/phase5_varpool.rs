//! Integration tests for REXX variable pool stages (Phase 5).
//!
//! These tests verify that the stages are properly registered and provide
//! appropriate error messages when called without Regina/REXX context.
//! 
//! Full functional tests with Regina must be run through the pipe CLI.
//! See examples/varpool_demo.rexx for working demonstrations.

use pipe_stages::run_spec;

#[test]
fn stem_registered() {
    // Verify STEM is in the registry (will fail without REXX context)
    let err = run_spec("literal test | stem TEST. | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("REXX variable pool") || msg.contains("Regina"),
        "Expected varpool error, got: {msg}"
    );
}

#[test]
fn var_registered() {
    let err = run_spec("literal test | var TEST | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("REXX variable pool") || msg.contains("Regina"),
        "Expected varpool error, got: {msg}"
    );
}

#[test]
fn varset_registered() {
    let err = run_spec("literal test | varset TEST /x/ | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("REXX variable pool") || msg.contains("Regina"),
        "Expected varpool error, got: {msg}"
    );
}

#[test]
fn varfetch_registered() {
    let err = run_spec("varfetch TEST | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("REXX variable pool") || msg.contains("Regina"),
        "Expected varpool error, got: {msg}"
    );
}

#[test]
fn rexxvars_registered() {
    let err = run_spec("rexxvars | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("REXX variable pool") || msg.contains("Regina"),
        "Expected varpool error, got: {msg}"
    );
}

#[test]
fn varload_registered() {
    let err = run_spec("literal test | varload | hole").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("REXX variable pool") || msg.contains("Regina"),
        "Expected varpool error, got: {msg}"
    );
}
