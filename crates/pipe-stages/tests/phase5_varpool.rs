//! Integration tests for REXX variable pool stages (Phase 5).
//!
//! These tests use temporary REXX scripts to verify variable pool behavior.
//! Set PIPE_TEST_REGINA=1 to enable (skips by default since Regina may not be installed).

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn tests_enabled() -> bool {
    env::var("PIPE_TEST_REGINA").as_deref() == Ok("1")
}

fn get_pipe_binary() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent()?.parent()?.to_path_buf();
    
    // Try release first, then debug
    for mode in &["release", "debug"] {
        let binary = workspace_root.join(format!("target/{}/pipe", mode));
        if binary.exists() {
            return Some(binary);
        }
    }
    None
}

fn run_rexx_script(script: &str, pipeline_prefix: &str) -> Result<std::process::Output, String> {
    let pipe = get_pipe_binary().ok_or("pipe binary not found")?;
    
    // Create script in current directory to avoid path parsing issues
    // Use thread ID to avoid collisions in concurrent tests
    let script_name = format!("varpool_test_{}_{:?}.rexx", 
        std::process::id(),
        std::thread::current().id());
    let script_path = PathBuf::from(&script_name);
    
    let mut file = fs::File::create(&script_path)
        .map_err(|e| format!("failed to create temp script: {}", e))?;
    file.write_all(script.as_bytes())
        .map_err(|e| format!("failed to write temp script: {}", e))?;
    drop(file);
    
    let spec = format!("{} {} | console", pipeline_prefix, script_name);
    let output = Command::new(&pipe)
        .arg(&spec)
        .env("PIPE_REQUIRE_REGINA", "1")
        .output()
        .map_err(|e| format!("failed to execute: {}", e))?;
    
    // Log stderr if command failed
    if !output.status.success() {
        eprintln!("Command failed with spec: {}", spec);
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    fs::remove_file(&script_path).ok();
    Ok(output)
}

#[test]
fn stem_as_source() {
    if !tests_enabled() {
        eprintln!("Skipping stem_as_source (set PIPE_TEST_REGINA=1 to enable)");
        return;
    }

    let script = r#"
DATA.0 = 3
DATA.1 = 'alpha'
DATA.2 = 'beta'
DATA.3 = 'gamma'
'CALLPIPE stem DATA. | console'
"#;

    let output = run_rexx_script(script, "literal x |")
        .expect("failed to run script");
    assert!(output.status.success(), "exit code: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("alpha"), "missing alpha");
    assert!(stdout.contains("beta"), "missing beta");
    assert!(stdout.contains("gamma"), "missing gamma");
}

#[test]
fn stem_as_sink() {
    if !tests_enabled() {
        eprintln!("Skipping stem_as_sink (set PIPE_TEST_REGINA=1 to enable)");
        return;
    }

    let script = r#"
'CALLPIPE literal one | literal two | stem RESULT.'
SAY 'COUNT:' RESULT.0
SAY 'ITEM1:' RESULT.1
SAY 'ITEM2:' RESULT.2
"#;

    let output = run_rexx_script(script, "literal x |")
        .expect("failed to run script");
    assert!(output.status.success(), "exit code: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("COUNT: 2"), "wrong count in: {}", stdout);
}

#[test]
fn var_as_source() {
    if !tests_enabled() {
        eprintln!("Skipping var_as_source (set PIPE_TEST_REGINA=1 to enable)");
        return;
    }

    let script = r#"
CONTENT = 'line_one' || '0A'x || 'line_two'
'CALLPIPE var CONTENT | console'
"#;

    let output = run_rexx_script(script, "literal x |")
        .expect("failed to run script");
    assert!(output.status.success(), "exit code: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("line_one"), "missing line_one");
    assert!(stdout.contains("line_two"), "missing line_two");
}

#[test]
fn var_as_sink() {
    if !tests_enabled() {
        eprintln!("Skipping var_as_sink (set PIPE_TEST_REGINA=1 to enable)");
        return;
    }

    let script = r#"
'CALLPIPE literal first | literal second | var RESULT'
SAY 'LENGTH:' LENGTH(RESULT)
"#;

    let output = run_rexx_script(script, "literal x |")
        .expect("failed to run script");
    assert!(output.status.success(), "exit code: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("LENGTH:"), "missing length output");
    let has_content = stdout.lines().any(|line| {
        line.contains("LENGTH:") && !line.contains("LENGTH: 0")
    });
    assert!(has_content, "VAR sink didn't set variable");
}

#[test]
fn varset_and_varfetch() {
    if !tests_enabled() {
        eprintln!("Skipping varset_and_varfetch (set PIPE_TEST_REGINA=1 to enable)");
        return;
    }

    let script = r#"
'CALLPIPE literal test | varset STATUS /passed/ | hole'
SAY 'STATUS:' STATUS
'CALLPIPE varfetch STATUS | console'
"#;

    let output = run_rexx_script(script, "literal x |")
        .expect("failed to run script");
    assert!(output.status.success(), "exit code: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("STATUS: passed"), "VARSET failed");
}

#[test]
fn varload() {
    if !tests_enabled() {
        eprintln!("Skipping varload (set PIPE_TEST_REGINA=1 to enable)");
        return;
    }

    // Create a temp data file
    let data_path = PathBuf::from("varload_test_data.txt");
    fs::write(&data_path, "KEY1 value_one\nKEY2 value_two\n")
        .expect("failed to write test data");

    let script = r#"
'CALLPIPE < varload_test_data.txt | varload | hole'
SAY 'KEY1:' KEY1
SAY 'KEY2:' KEY2
"#;

    let output = run_rexx_script(script, "literal x |")
        .expect("failed to run script");
    
    fs::remove_file(&data_path).ok();
    
    assert!(output.status.success(), "exit code: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("KEY1: value_one"), "KEY1 not set correctly");
    assert!(stdout.contains("KEY2: value_two"), "KEY2 not set correctly");
}

#[test]
fn rexxvars() {
    if !tests_enabled() {
        eprintln!("Skipping rexxvars (set PIPE_TEST_REGINA=1 to enable)");
        return;
    }

    let script = r#"
TESTVAR = 'sample'
'CALLPIPE rexxvars | take first 1 | console'
SAY 'OK'
"#;

    let output = run_rexx_script(script, "literal x |")
        .expect("failed to run script");
    assert!(output.status.success(), "exit code: {}", output.status);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("OK"), "REXXVARS didn't complete");
}
