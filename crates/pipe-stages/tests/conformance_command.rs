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

fn command_that_emits(bytes: &[u8]) -> String {
    if cfg!(windows) {
        let list: String = bytes
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "powershell -NoProfile -Command \"[Console]::OpenStandardOutput().Write([byte[]]({list}), 0, {})\"",
            bytes.len()
        )
    } else {
        let mut cmd = String::from("printf '");
        for &b in bytes {
            cmd.push_str(&format!("\\{b:03o}"));
        }
        cmd.push('\'');
        cmd
    }
}

#[test]
fn command_stdout_need_not_be_utf8() {
    // COMMAND splits host stdout on LF (optional CR before LF); no UTF-8 check.
    let mut payload = vec![0xFF, 0xFE];
    payload.extend_from_slice(b"hi\r\n");
    payload.push(0x80);
    payload.push(b'\n');

    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let reg = StageRegistry::default();
    let mut b = PipelineBuilder::new();
    let cmd = b.add_stage(
        "COMMAND",
        reg.create("COMMAND", vec![command_that_emits(&payload)])
            .unwrap(),
    );
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
