use std::process::Command;

fn pipe_bin() -> Command {
    // Run via cargo when integrating; for unit-style, call library.
    Command::new(env!("CARGO_BIN_EXE_pipe"))
}

#[test]
fn literal_console_exits_zero() {
    let out = pipe_bin()
        .arg("literal hello | console")
        .output()
        .expect("run pipe");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello"), "stdout={stdout}");
}

#[test]
fn trace_flag_emits_stderr() {
    let out = pipe_bin()
        .args(["--trace", "literal hello | console"])
        .output()
        .expect("run pipe");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("PIPE TRACE:"), "stderr={stderr}");
    assert!(stderr.contains("WRITE"), "stderr={stderr}");
    assert!(stderr.contains("dispatcher end"), "stderr={stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello"));
}

#[test]
fn trace_option_in_spec() {
    let out = pipe_bin()
        .arg("(TRACE) literal hi | console")
        .output()
        .expect("run pipe");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("PIPE TRACE: stage["), "stderr={stderr}");
}

#[test]
fn change_stage() {
    let out = pipe_bin()
        .arg("literal xx | change /xx/ /yy/ | console")
        .output()
        .expect("run pipe");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("yy"));
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_pipe_in_repo(spec: &str) -> std::process::Output {
    pipe_bin()
        .current_dir(repo_root())
        .arg(spec)
        .output()
        .expect("run pipe")
}

#[test]
fn rexx_upper_example() {
    let out = run_pipe_in_repo("literal hello | examples/upper.rexx | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("HELLO"));
}

#[test]
fn rexx_filter_and_take_examples() {
    let out = run_pipe_in_repo(
        "examples/emit.rexx apple banana apricot | examples/filter.rexx ap | console",
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("apple"), "stdout={stdout}");
    assert!(stdout.contains("apricot"), "stdout={stdout}");
    assert!(!stdout.contains("banana"), "stdout={stdout}");

    let out = run_pipe_in_repo("examples/emit.rexx a b c d | examples/take.rexx 2 | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains('a'), "stdout={stdout}");
    assert!(stdout.contains('b'), "stdout={stdout}");
    assert!(!stdout.contains('c'), "stdout={stdout}");
}

#[test]
fn aggrc_last_stage_exits_with_aggregate() {
    // Author's Edition 1.1.11 p.259: last-stage AGGRC sets PIPE RC 6.
    let out = pipe_bin()
        .arg("literal 6 | aggrc")
        .output()
        .expect("run pipe");
    assert_eq!(
        out.status.code(),
        Some(6),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout.is_empty(),
        "stdout={:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn aggrc_not_last_exits_zero() {
    let out = pipe_bin()
        .arg("literal 6 | aggrc | hole")
        .output()
        .expect("run pipe");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn rexx_fields_chop_wrap() {
    // No space before `|`: a blank there is a trailing blank in LITERAL's argument
    // (Author's Edition 1.1.11 p.412).
    let out = run_pipe_in_repo("literal red green blue|examples/fields.rexx | console");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("green blue|red"));

    let out = run_pipe_in_repo("literal abcdef|examples/chop.rexx 2 3 | console");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("6:bcd"));

    let out = run_pipe_in_repo("literal hi|examples/wrap.rexx | console");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("[HI]"));
}
