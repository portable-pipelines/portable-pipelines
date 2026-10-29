use std::process::Command;

fn pipe_bin() -> Command {
    // Run via cargo when integrating; for unit-style, call library.
    Command::new(env!("CARGO_BIN_EXE_pipe"))
}

#[test]
fn version_flag_prints_version_and_exits_zero() {
    for flag in ["--version", "-v", "-V"] {
        let out = pipe_bin().arg(flag).output().expect("run pipe");
        assert!(
            out.status.success(),
            "flag={flag} stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.starts_with("pipe "), "flag={flag} stdout={stdout}");
        assert!(
            stdout.contains(env!("CARGO_PKG_VERSION")),
            "flag={flag} stdout={stdout}"
        );
    }
}

#[test]
fn literal_console_exits_zero() {
    let out = pipe_bin()
        .arg("literal hello | console")
        .output()
        .expect("run pipe");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hello"), "stdout={stdout}");
}

#[test]
fn trace_flag_emits_stderr() {
    let out = pipe_bin()
        .args(["--trace", "literal hello | console"])
        .output()
        .expect("run pipe");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
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
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
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
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("green blue|red"));

    let out = run_pipe_in_repo("literal abcdef|examples/chop.rexx 2 3 | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("6:bcd"));

    let out = run_pipe_in_repo("literal hi|examples/wrap.rexx | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("[HI]"));
}

fn python_available_or_skip() -> bool {
    if pipe_python::python_available() {
        return true;
    }
    if pipe_python::pipe_require_python() {
        panic!(
            "PIPE_REQUIRE_PYTHON=1 but Python 3.10+ was not discovered ({})",
            pipe_python::discovery_hint()
        );
    }
    false
}

#[test]
fn python_upper_example() {
    if !python_available_or_skip() {
        return;
    }
    let out = run_pipe_in_repo("literal hello | examples/upper.py | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("HELLO"));
}

#[test]
fn python_upper_finds_helper_without_pythonpath() {
    if !python_available_or_skip() {
        return;
    }
    let out = pipe_bin()
        .current_dir(repo_root())
        .env_remove("PYTHONPATH")
        .env_remove("PIPE_PYTHONPATH")
        .arg("literal hello | examples/upper.py | console")
        .output()
        .expect("run pipe");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("HELLO"));
}

#[test]
fn python_exit_matches_rexx_process_status() {
    if !python_available_or_skip() {
        return;
    }
    let py = run_pipe_in_repo("literal x | tests/pipelines/python/exit7.py | hole");
    let rexx = run_pipe_in_repo("literal x | tests/pipelines/python/exit7.rexx | hole");
    assert_eq!(
        py.status.code(),
        rexx.status.code(),
        "python stderr={} rexx stderr={}",
        String::from_utf8_lossy(&py.stderr),
        String::from_utf8_lossy(&rexx.stderr)
    );
    assert_eq!(py.status.code(), Some(7));
}

#[test]
fn stage_error_prints_cms_context_for_labeled_pick() {
    // John's multi-pipeline PICK topology: secondary input of `b:` is connected
    // by `a: | b:`. CMS numbers the definition as stage 3 of pipeline 1.
    let dir = std::env::temp_dir().join("cms-pipelines-pick-context");
    let _ = std::fs::create_dir_all(&dir);
    let data = dir.join("data1.txt");
    let matched = dir.join("matched.txt");
    let others = dir.join("others.txt");
    std::fs::write(&data, "A1\nB1\nC1\n").unwrap();
    let spec = format!(
        "(End ?) < {} | a: PICK 1.1 == /A/ | b: PICK 1.1 == /B/ | > {} ? a: | b:? b: | > {}",
        data.display(),
        matched.display(),
        others.display()
    );
    let out = pipe_bin().arg(&spec).output().expect("run pipe");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("PIPE: PICK: secondary input stream must not be connected"),
        "stderr={stderr}"
    );
    assert!(
        stderr.contains("... Issued from stage 3 of pipeline 1"),
        "stderr={stderr}"
    );
    assert!(
        stderr.contains("... Running \"PICK 1.1 == /B/\""),
        "stderr={stderr}"
    );
    assert!(
        !stderr.contains("... Processing \""),
        "top-level PIPE must not emit message 2; stderr={stderr}"
    );
}

#[test]
fn stall_is_plain_words_and_exit_8() {
    let dir = tempfile_or_tmp();
    let file = dir.join("three.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let spec = format!(
        "(end ?) < {} | a: fanout | f: fanin | console ? a: | f:",
        file.display()
    );
    let out = pipe_bin()
        .current_dir(repo_root())
        .arg(&spec)
        .output()
        .expect("run pipe");
    assert_eq!(
        out.status.code(),
        Some(8),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("pipeline stalled (no stage can make progress)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("waiting to write") || stderr.contains("waiting to read"),
        "{stderr}"
    );
    assert!(stderr.contains("Hint:"), "{stderr}");
    assert!(
        !stderr.contains("read=None") && !stderr.contains("write=Some"),
        "raw dump leaked: {stderr}"
    );

    let traced = pipe_bin()
        .current_dir(repo_root())
        .args(["--trace", &spec])
        .output()
        .expect("run pipe --trace");
    assert_eq!(traced.status.code(), Some(8));
    let tstderr = String::from_utf8_lossy(&traced.stderr);
    assert!(
        tstderr.contains("stall dump:") && tstderr.contains("read="),
        "expected raw dump under --trace: {tstderr}"
    );
}

#[test]
fn callpipe_stall_is_plain_words_and_exit_8() {
    // Nested FANOUT/FANIN through PIPCMD CALLPIPE must still stall, not hang,
    // and the process status stays 8.
    let spec = "literal callpipe (ENDCHAR ?) literal a||literal b|| a: fanout || f: fanin || hole ? a: || f: | pipcmd";
    let out = pipe_bin()
        .current_dir(repo_root())
        .arg(spec)
        .output()
        .expect("run pipe");
    assert_eq!(
        out.status.code(),
        Some(8),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("pipeline stalled (no stage can make progress)"),
        "{stderr}"
    );
    assert!(stderr.contains("waiting for CALLPIPE"), "{stderr}");
}

#[test]
fn take_then_rexx_copies_then_eof() {
    let out = run_pipe_in_repo("literal 40 50 30 | split | take 3 | examples/upper.rexx | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "40\n50\n30");

    let out = run_pipe_in_repo("literal 40 50 30 | split | take 1 | examples/upper.rexx | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "40");
}

#[test]
fn take_then_luhn_peekto() {
    let out = run_pipe_in_repo("literal 79927398713 | take 1 | examples/luhn.rexx | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}

#[test]
fn luhn_example_valid_and_invalid() {
    let out = run_pipe_in_repo("literal 79927398713 | examples/luhn.rexx | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");

    let out = run_pipe_in_repo("literal 79927398714 | examples/luhn.rexx | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}

#[test]
fn glued_labels_parse_and_run() {
    let out = run_pipe_in_repo("(end ?) literal a | o:fanout | console ? o: | console");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.matches("a").count(), 2, "stdout={stdout}");
}

fn tempfile_or_tmp() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("pipe-stall-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&p);
    p
}

#[test]
fn juxtapose_user_mdisk_example() {
    let out = run_pipe_in_repo(
        "(end ?) < examples/users.txt | a: strfind /USER / | spec 1-* 1 / / next | j: juxtapose | console ? a: | j:",
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("USER AAA MDISK 191"), "stdout={stdout}");
    assert!(stdout.contains("USER AAA MDISK 192"), "stdout={stdout}");
    assert!(stdout.contains("USER AAA MDISK 193"), "stdout={stdout}");
    assert!(stdout.contains("USER BBB MDISK 191"), "stdout={stdout}");
    assert!(stdout.contains("USER BBB MDISK 192"), "stdout={stdout}");
}
