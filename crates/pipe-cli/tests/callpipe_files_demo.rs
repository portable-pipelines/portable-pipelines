//! CLI smoke for the streaming CALLPIPE file-set example. Output shape
//! only; early-stop and RSS live in pipe-stages `callpipe_files`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn pipe_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pipe"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "pipe-callpipe-files-demo-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write_set(root: &std::path::Path, nfiles: usize, nlines: usize) -> (PathBuf, PathBuf) {
    let files_dir = root.join("files");
    fs::create_dir_all(&files_dir).unwrap();
    let mut paths = Vec::new();
    let mut commands = Vec::new();
    for n in 1..=nfiles {
        let path = files_dir.join(format!("f{n:04}.txt"));
        let mut body = String::new();
        for ln in 1..=nlines {
            body.push_str(&format!("file={n:04} line={ln:04}\n"));
        }
        fs::write(&path, body).unwrap();
        let full = path.to_string_lossy().into_owned();
        paths.push(full.clone());
        commands.push(format!("callpipe < {full} | xlate 1-* upper | *:"));
    }
    let paths_file = root.join("paths.txt");
    let cmds_file = root.join("commands.txt");
    fs::write(&paths_file, paths.join("\n") + "\n").unwrap();
    fs::write(&cmds_file, commands.join("\n") + "\n").unwrap();
    (paths_file, cmds_file)
}

fn assert_ok(output: &std::process::Output, ctx: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{ctx} exit={:?} stdout={stdout} stderr={stderr}",
        output.status.code()
    );
}

#[test]
fn pipcmd_demo_uppercases_each_file() {
    let root = tmp_root();
    let (_paths, cmds) = write_set(&root, 3, 2);
    let spec = format!("< {} | pipcmd | console", cmds.display());
    let output = pipe_bin()
        .current_dir(repo_root())
        .arg(&spec)
        .output()
        .expect("run pipe");
    assert_ok(&output, "pipcmd demo");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "FILE=0001 LINE=0001",
            "FILE=0001 LINE=0002",
            "FILE=0002 LINE=0001",
            "FILE=0002 LINE=0002",
            "FILE=0003 LINE=0001",
            "FILE=0003 LINE=0002",
        ]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn pipcmd_take_10_stops_before_missing_files() {
    let root = tmp_root();
    let files_dir = root.join("files");
    fs::create_dir_all(&files_dir).unwrap();
    for n in 1..=2 {
        let path = files_dir.join(format!("f{n:04}.txt"));
        let mut body = String::new();
        for ln in 1..=5 {
            body.push_str(&format!("file={n:04} line={ln:04}\n"));
        }
        fs::write(&path, body).unwrap();
    }
    let mut commands = Vec::new();
    for n in 1..=8 {
        let path = files_dir.join(format!("f{n:04}.txt"));
        commands.push(format!(
            "callpipe < {} | xlate 1-* upper | *:",
            path.display()
        ));
    }
    let cmds = root.join("commands.txt");
    fs::write(&cmds, commands.join("\n") + "\n").unwrap();
    let spec = format!(
        "< {} | pipcmd stop anyeof | take 10 | console",
        cmds.display()
    );
    let output = pipe_bin()
        .current_dir(repo_root())
        .arg(&spec)
        .output()
        .expect("run pipe");
    assert_ok(&output, "pipcmd take 10");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 10, "{lines:?}");
    assert_eq!(lines[0], "FILE=0001 LINE=0001");
    assert_eq!(lines[9], "FILE=0002 LINE=0005");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn rexx_example_uppercases_each_file() {
    let root = tmp_root();
    let (paths, _cmds) = write_set(&root, 2, 2);
    let spec = format!(
        "< {} | examples/callpipe_files.rexx | console",
        paths.display()
    );
    let output = pipe_bin()
        .current_dir(repo_root())
        .arg(&spec)
        .output()
        .expect("run pipe");
    assert_ok(&output, "rexx example");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "FILE=0001 LINE=0001",
            "FILE=0001 LINE=0002",
            "FILE=0002 LINE=0001",
            "FILE=0002 LINE=0002",
        ]
    );
    let _ = fs::remove_dir_all(&root);
}
