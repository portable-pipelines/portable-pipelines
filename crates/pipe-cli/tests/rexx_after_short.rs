//! Regina-backed: TAKE (and DROP LAST) SHORT must look like EOF to a REXX
//! stage, not RC 8 "stream severed". Skip when Regina is not installed; fail
//! if `PIPE_REQUIRE_REGINA=1` and Regina is missing.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn skip_without_regina() -> bool {
    if pipe_rexx::regina_available() {
        return false;
    }
    if std::env::var("PIPE_REQUIRE_REGINA").as_deref() == Ok("1") {
        panic!(
            "PIPE_REQUIRE_REGINA=1 but Regina was not discovered/loaded ({})",
            pipe_rexx::discovery_hint()
        );
    }
    eprintln!("Skipping (Regina not installed)");
    true
}

fn pipe_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pipe"))
}

static SCRIPT_SEQ: AtomicU64 = AtomicU64::new(0);

fn run_copy_after(spec_mid: &str, input_lines: &[&str]) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!(
        "pipe-rexx-short-{}-{}",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let script_path: PathBuf = dir.join("r1.rexx");
    fs::write(
        &script_path,
        "DO FOREVER\n\
         'READTO REC'\n\
         IF RC = 12 THEN LEAVE\n\
         'OUTPUT' REC\n\
         END\n",
    )
    .expect("write script");
    let nums: PathBuf = dir.join("nums.txt");
    fs::write(&nums, input_lines.join("\n") + "\n").expect("write nums");
    let spec = format!("< nums.txt | {spec_mid} | r1.rexx | console");
    let output = pipe_bin()
        .current_dir(&dir)
        .env("PIPE_REQUIRE_REGINA", "1")
        .arg(&spec)
        .output()
        .expect("run pipe");
    let _ = fs::remove_dir_all(&dir);
    output
}

fn assert_ok_lines(output: &std::process::Output, want: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "exit={:?} stdout={stdout} stderr={stderr}",
        output.status.code()
    );
    assert!(
        !stderr.contains("stream severed"),
        "stderr still reports stream severed: {stderr}"
    );
    assert_eq!(stdout.trim(), want, "stderr={stderr}");
}

#[test]
fn regina_take_then_readto_copies_then_eof() {
    if skip_without_regina() {
        return;
    }
    assert_eq!(pipe_rexx::preferred_engine(), pipe_rexx::RexxEngine::Regina);
    assert_ok_lines(&run_copy_after("take 3", &["40", "50", "30"]), "40\n50\n30");
    assert_ok_lines(&run_copy_after("take 1", &["40", "50", "30"]), "40");
    assert_ok_lines(&run_copy_after("take 2", &["40", "50", "30"]), "40\n50");
}

#[test]
fn regina_drop_last_then_readto_copies_then_eof() {
    if skip_without_regina() {
        return;
    }
    assert_ok_lines(
        &run_copy_after("drop last 1", &["40", "50", "30"]),
        "40\n50",
    );
}
