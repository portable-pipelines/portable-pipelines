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

#[test]
fn rexx_upper_example() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = root.join("examples/upper.rex");
    assert!(script.exists(), "missing {}", script.display());
    // Use forward slashes so the PIPE scanner does not treat `C:` as a label.
    let script_url = script.to_string_lossy().replace('\\', "/");
    let spec = format!("literal hello | {script_url} | console");
    let out = pipe_bin()
        .current_dir(&root)
        .arg(&spec)
        .output()
        .expect("run pipe");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("HELLO"));
}
