//! DISK — portable stand-in for diskfast, Author's Edition 1.1.11 p.328.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, StageCtx};
use pipe_stages::{run_spec, StageRegistry};

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cms-pipelines-disk-{name}"))
}

fn run_read(path: &str) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let mut b = PipelineBuilder::new();
    let disk = b.add_stage("DISK", reg.create("DISK", vec![path.into()]).unwrap());
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(disk, sink);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn missing_file_is_a_null_file() {
    // diskfast p.328: a file that does not exist is a null file.
    let path = tmp("missing-no-such.txt");
    let _ = std::fs::remove_file(&path);
    assert!(run_read(path.to_str().unwrap()).is_empty());
}

#[test]
fn reads_existing_lines() {
    let path = tmp("read-lines.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    assert_eq!(
        run_read(path.to_str().unwrap()),
        vec!["one".to_string(), "two".to_string()]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn invented_output_append_are_rejected() {
    let path = tmp("invented-output.txt");
    let err = run_spec(&format!("literal hi|disk output {}|hole", path.display())).unwrap_err();
    assert!(
        err.to_string().contains("OUTPUT") || err.to_string().contains(">"),
        "{err}"
    );
    let err = run_spec(&format!("literal hi|disk append {}|hole", path.display())).unwrap_err();
    assert!(
        err.to_string().contains("APPEND") || err.to_string().contains(">>"),
        "{err}"
    );
}

#[test]
fn not_first_appends_like_diskfast() {
    // diskfast p.328: when not first, records are appended (file created if needed).
    let path = tmp("append-not-first.txt");
    std::fs::write(&path, "old\n").unwrap();
    let spec = format!("literal new|disk {}|hole", path.display());
    run_spec(&spec).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    assert_eq!(body, "old\nnew\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn path_is_required() {
    let err = run_spec("disk | hole").unwrap_err();
    assert!(
        err.to_string().contains("path") || err.to_string().contains("DISK"),
        "{err}"
    );
}

fn non_utf8_sample() -> Vec<u8> {
    let mut data = vec![0xFF, 0xFE];
    data.extend_from_slice(&[b'P', 0x00, b'l', 0x00, b'a', 0x00, b'n', 0x00]);
    data.extend_from_slice(b"\r\n");
    data.extend_from_slice(&[b'x', 0x00, b'm', 0x00, b'l', 0x00]);
    data.extend_from_slice(b"\r\n");
    data
}

#[test]
fn disk_reads_and_writes_non_utf8_bytes() {
    let data = non_utf8_sample();
    let src = tmp("non-utf8-disk-in.bin");
    let counted = tmp("non-utf8-disk-count.txt");
    let out = tmp("non-utf8-disk-out.bin");
    std::fs::write(&src, &data).unwrap();

    run_spec(&format!(
        "disk {} | count lines | > {}",
        src.display(),
        counted.display()
    ))
    .unwrap();
    assert_eq!(std::fs::read(&counted).unwrap(), b"2\n");

    let _ = std::fs::remove_file(&out);
    run_spec(&format!(
        "< {} | disk {} | hole",
        src.display(),
        out.display()
    ))
    .unwrap();
    // DISK not-first appends; writers emit record + LF (CR before LF stripped).
    let expected = {
        let mut v = vec![
            0xFF, 0xFE, b'P', 0x00, b'l', 0x00, b'a', 0x00, b'n', 0x00, b'\n',
        ];
        v.extend_from_slice(&[b'x', 0x00, b'm', 0x00, b'l', 0x00, b'\n']);
        v
    };
    assert_eq!(std::fs::read(&out).unwrap(), expected);

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&counted);
    let _ = std::fs::remove_file(&out);
}
