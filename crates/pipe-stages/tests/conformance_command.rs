//! COMMAND / CMD — Author's Edition 1.1.11 pp. 300–301 (portable subset).

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx};
use pipe_stages::{run_spec, StageRegistry};

#[test]
fn cmd_is_a_documented_synonym() {
    let reg = StageRegistry::default();
    assert!(reg.contains("COMMAND"));
    assert!(reg.contains("CMD"));
}

#[test]
fn argument_string_is_issued() {
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let cmd = b.add_stage(
        "COMMAND",
        reg.create("COMMAND", vec!["echo".into(), "hi".into()])
            .unwrap(),
    );
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(cmd, sink);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["hi".to_string()]);
}

#[test]
fn blank_input_records_are_discarded() {
    // p.300: null and blank input records are discarded.
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx: &mut StageCtx| {
            let _ = ctx.write(Record::from(""));
            let _ = ctx.write(Record::from("   "));
            let _ = ctx.write(Record::from("echo one"));
        }),
    );
    let cmd = b.add_stage("COMMAND", reg.create("COMMAND", vec![]).unwrap());
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, cmd);
    b.connect_primary(cmd, sink);
    b.run().unwrap();
    assert_eq!(*out.borrow(), vec!["one".to_string()]);
}

#[test]
fn host_alias_was_removed() {
    let err = run_spec("host echo hi | hole").unwrap_err();
    assert!(
        err.to_string().contains("unknown") || err.to_string().contains("HOST"),
        "{err}"
    );
}

/// Host command that writes `bytes` to stdout, plus a guard that keeps any
/// temp files alive until after COMMAND runs.
fn command_that_emits(bytes: &[u8]) -> (String, Option<TempPayload>) {
    if cfg!(windows) {
        // Nested `powershell -Command "..."` quotes do not survive `cmd /C`
        // (Rust argv quoting, then cmd quote-stripping). A -File script that
        // writes a temp payload is the same proof without that quoting maze.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "cms-pipelines-cmd-bytes-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("payload.bin");
        let ps1 = dir.join("dump.ps1");
        std::fs::write(&bin, bytes).unwrap();
        std::fs::write(
            &ps1,
            concat!(
                "$ErrorActionPreference = 'Stop'\r\n",
                "$p = Join-Path $PSScriptRoot 'payload.bin'\r\n",
                "$b = [System.IO.File]::ReadAllBytes($p)\r\n",
                "$o = [Console]::OpenStandardOutput()\r\n",
                "[void]$o.Write($b, 0, $b.Length)\r\n",
            ),
        )
        .unwrap();
        let cmd = format!("powershell -NoProfile -File \"{}\"", ps1.display());
        (cmd, Some(TempPayload { dir }))
    } else {
        let mut cmd = String::from("printf '");
        for &b in bytes {
            cmd.push_str(&format!("\\{b:03o}"));
        }
        cmd.push('\'');
        (cmd, None)
    }
}

struct TempPayload {
    dir: std::path::PathBuf,
}

impl Drop for TempPayload {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn command_stdout_need_not_be_utf8() {
    // COMMAND splits host stdout on LF (optional CR before LF); no UTF-8 check.
    let mut payload = vec![0xFF, 0xFE];
    payload.extend_from_slice(b"hi\r\n");
    payload.push(0x80);
    payload.push(b'\n');

    let (shell, _keep) = command_that_emits(&payload);
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let cmd = b.add_stage("COMMAND", reg.create("COMMAND", vec![shell]).unwrap());
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.as_bytes().to_vec());
            }
        }),
    );
    b.connect_primary(cmd, sink);
    b.run().unwrap();
    assert_eq!(
        *out.borrow(),
        vec![vec![0xFF, 0xFE, b'h', b'i'], vec![0x80]]
    );
}
