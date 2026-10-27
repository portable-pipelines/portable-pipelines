//! Regina-backed tests for STEM, VAR, VARSET, VARFETCH, REXXVARS, VARLOAD.
//!
//! Each test is a separate process, so Regina RexxStart is not reused across
//! `#[test]` functions. Skip when Regina is not installed; fail if
//! `PIPE_REQUIRE_REGINA=1` and Regina is missing.

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

fn run_rexx(script: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!(
        "pipe-varpool-{}-{}",
        std::process::id(),
        SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("temp dir");
    let script_path: PathBuf = dir.join("t.rexx");
    fs::write(&script_path, script).expect("write script");
    let output = pipe_bin()
        .current_dir(&dir)
        .env("PIPE_REQUIRE_REGINA", "1")
        .arg("t.rexx | console")
        .output()
        .expect("run pipe");
    let _ = fs::remove_dir_all(&dir);
    output
}

fn assert_ok(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "exit={:?} stdout={stdout} stderr={stderr}",
        output.status.code()
    );
    stdout
}

#[test]
fn stem_as_source() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
DATA.0 = 3
DATA.1 = 'alpha'
DATA.2 = 'beta'
DATA.3 = 'gamma'
'CALLPIPE stem DATA. | console'
"#,
    ));
    assert!(stdout.contains("alpha"), "{stdout}");
    assert!(stdout.contains("beta"), "{stdout}");
    assert!(stdout.contains("gamma"), "{stdout}");
}

#[test]
fn stem_as_sink() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
'CALLPIPE literal one | literal two | stem RESULT.'
SAY 'COUNT:' RESULT.0
SAY 'ITEM1:' RESULT.1
SAY 'ITEM2:' RESULT.2
"#,
    ));
    assert!(stdout.contains("COUNT: 2"), "{stdout}");
    assert!(
        stdout.contains("ITEM1: two") || stdout.contains("ITEM1: one"),
        "{stdout}"
    );
}

#[test]
fn stem_empty_and_tail() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
EMPTY.0 = 0
'CALLPIPE stem EMPTY. | stem COPY.'
SAY 'EMPTYCOUNT:' COPY.0
'CALLPIPE stem NEVERSET. | stem UNINIT.'
SAY 'UNINITCOUNT:' UNINIT.0
'CALLPIPE literal x | stem FOO.BAR.'
SAY 'TAILCOUNT:' FOO.BAR.0
SAY 'TAIL1:' FOO.BAR.1
"#,
    ));
    assert!(stdout.contains("EMPTYCOUNT: 0"), "{stdout}");
    assert!(stdout.contains("UNINITCOUNT: 0"), "{stdout}");
    assert!(stdout.contains("TAILCOUNT: 1"), "{stdout}");
    assert!(stdout.contains("TAIL1: x"), "{stdout}");
}

#[test]
fn var_as_source() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
CONTENT = 'line_one' || '0A'x || 'line_two'
'CALLPIPE var CONTENT | console'
"#,
    ));
    assert!(stdout.contains("line_one"), "{stdout}");
    assert!(stdout.contains("line_two"), "{stdout}");
}

#[test]
fn var_as_sink() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
'CALLPIPE literal first | literal second | var RESULT'
SAY 'LENGTH:' LENGTH(RESULT)
"#,
    ));
    let has_content = stdout
        .lines()
        .any(|line| line.contains("LENGTH:") && !line.contains("LENGTH: 0"));
    assert!(has_content, "VAR sink didn't set variable: {stdout}");
}

#[test]
fn varset_and_varfetch() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
'CALLPIPE literal test | varset STATUS /passed/ | hole'
SAY 'STATUS:' STATUS
'CALLPIPE varfetch STATUS | console'
"#,
    ));
    assert!(stdout.contains("STATUS: passed"), "{stdout}");
}

#[test]
fn varload() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
call lineout 'varload_test_data.txt', 'KEY1 value_one'
call lineout 'varload_test_data.txt', 'KEY2 value two'
call lineout 'varload_test_data.txt'
'CALLPIPE < varload_test_data.txt | varload'
SAY 'KEY1:' KEY1
SAY 'KEY2:' KEY2
"#,
    ));
    assert!(stdout.contains("KEY1: value_one"), "{stdout}");
    assert!(stdout.contains("KEY2: value two"), "{stdout}");
}

#[test]
fn rexxvars() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
TESTVAR = 'sample'
'CALLPIPE rexxvars | locate /TESTVAR/ | console'
SAY 'OK'
"#,
    ));
    assert!(stdout.contains("TESTVAR"), "{stdout}");
    assert!(stdout.contains("OK"), "{stdout}");
}

#[test]
fn lookup_unmatched_details_into_stem() {
    if skip_without_regina() {
        return;
    }
    let stdout = assert_ok(&run_rexx(
        r#"
PHOTOS.0 = 3
PHOTOS.1 = 'apple.jpg'
PHOTOS.2 = 'apricot.jpg'
PHOTOS.3 = 'banana.jpg'
DONE.0 = 2
DONE.1 = 'apple.jpg'
DONE.2 = 'banana.jpg'
'CALLPIPE (end ?) stem PHOTOS. | l: lookup | hole ? stem DONE. | l: | stem TODO.'
SAY 'RC=' RC
SAY 'TODO.0=' TODO.0
DO I = 1 TO TODO.0
  SAY 'TODO.' || I || '=' TODO.I
END
"#,
    ));
    assert!(
        stdout.contains("RC= 0") || stdout.contains("RC=0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("TODO.0= 1") || stdout.contains("TODO.0=1"),
        "{stdout}"
    );
    assert!(stdout.contains("apricot.jpg"), "{stdout}");
}

#[test]
fn photo_workflow_example() {
    if skip_without_regina() {
        return;
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = pipe_bin()
        .current_dir(&repo)
        .env("PIPE_REQUIRE_REGINA", "1")
        .arg("examples/photo_workflow.rexx | console")
        .output()
        .expect("run photo workflow");
    let stdout = assert_ok(&output);
    assert!(stdout.contains("Total photos: 5"), "{stdout}");
    assert!(stdout.contains("Already processed: 2"), "{stdout}");
    assert!(stdout.contains("Leftover photos: 3"), "{stdout}");
    assert!(stdout.contains("vacation/img002.jpg"), "{stdout}");
    assert!(stdout.contains("vacation/img003.jpg"), "{stdout}");
    assert!(stdout.contains("beach/img010.jpg"), "{stdout}");
    assert!(
        !stdout.contains("exiftool -overwrite_original -Comment=reviewed already_done.jpg"),
        "{stdout}"
    );
    assert!(stdout.contains("exiftool"), "{stdout}");
    assert!(stdout.contains("Workflow complete"), "{stdout}");
}
