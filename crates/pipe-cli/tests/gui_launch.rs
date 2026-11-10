use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn pipe_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pipe"))
}

#[test]
fn gui_flag_without_sibling_explains_same_folder() {
    let src = PathBuf::from(env!("CARGO_BIN_EXE_pipe"));
    static N: AtomicU64 = AtomicU64::new(0);
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "pipe-gui-missing-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let dest = if cfg!(windows) {
        dir.join("pipe.exe")
    } else {
        dir.join("pipe")
    };
    fs::copy(&src, &dest).unwrap();
    let out = Command::new(&dest)
        .arg("--gui")
        .arg("my file.pipe")
        .output()
        .expect("run copied pipe --gui");
    let _ = fs::remove_file(&dest);
    let _ = fs::remove_dir(&dir);
    assert!(
        !out.status.success(),
        "missing sibling must fail: {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("same folder as `pipe`") && stderr.contains("does not search PATH"),
        "stderr={stderr}"
    );
    assert!(stderr.contains("pipe-gui"), "stderr={stderr}");
}

#[test]
fn gui_help_resolves_when_sibling_exists() {
    let src = PathBuf::from(env!("CARGO_BIN_EXE_pipe"));
    static N: AtomicU64 = AtomicU64::new(0);
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "pipe-gui-help-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let dest = if cfg!(windows) {
        dir.join("pipe.exe")
    } else {
        dir.join("pipe")
    };
    let gui = if cfg!(windows) {
        dir.join("pipe-gui.exe")
    } else {
        dir.join("pipe-gui")
    };
    fs::copy(&src, &dest).unwrap();
    fs::write(&gui, b"").unwrap();
    let out = Command::new(&dest)
        .arg("--gui")
        .arg("--help")
        .output()
        .expect("run copied pipe --gui --help");
    let _ = fs::remove_file(&dest);
    let _ = fs::remove_file(&gui);
    let _ = fs::remove_dir(&dir);
    assert!(
        out.status.success(),
        "sibling --help must succeed: {:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("desktop GUI is") && stdout.contains("does not search PATH"),
        "stdout={stdout}"
    );
}

#[test]
fn gui_help_still_requires_sibling() {
    let src = PathBuf::from(env!("CARGO_BIN_EXE_pipe"));
    static N: AtomicU64 = AtomicU64::new(0);
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "pipe-gui-help-missing-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let dest = if cfg!(windows) {
        dir.join("pipe.exe")
    } else {
        dir.join("pipe")
    };
    fs::copy(&src, &dest).unwrap();
    let out = Command::new(&dest)
        .args(["--gui", "--help"])
        .output()
        .expect("run copied pipe --gui --help without sibling");
    let _ = fs::remove_file(&dest);
    let _ = fs::remove_dir(&dir);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("was not found next to"), "stderr={stderr}");
}

#[test]
fn version_still_beats_gui() {
    let out = pipe_bin()
        .args(["--gui", "--version"])
        .output()
        .expect("pipe --gui --version");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("pipe "));
}
