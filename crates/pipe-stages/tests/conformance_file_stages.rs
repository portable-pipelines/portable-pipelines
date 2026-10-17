//! `<` (p.230), `>` (p.237), `>>` (p.245) — portable host-path operands.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, StageCtx};
use pipe_stages::{run_spec, StageRegistry};

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cms-pipelines-file-{name}"))
}

fn run_first(verb: &str, path: &str) -> Vec<String> {
    let reg = StageRegistry::default();
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = Rc::clone(&out);
    let mut b = PipelineBuilder::new();
    let src = b.add_stage(verb, reg.create(verb, vec![path.into()]).unwrap());
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx: &mut StageCtx| {
            while let Ok(Some(r)) = ctx.read() {
                o.borrow_mut().push(r.to_string_lossy());
            }
        }),
    );
    b.connect_primary(src, sink);
    b.run().unwrap();
    let result = out.borrow().clone();
    result
}

#[test]
fn file_in_reads_existing_lines() {
    let path = tmp("read-lines.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    assert_eq!(
        run_first("<", path.to_str().unwrap()),
        vec!["one".to_string(), "two".to_string()]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_in_missing_file_is_an_error() {
    // `<` is not diskfast: a missing file is not a null file.
    let path = tmp("missing-no-such.txt");
    let _ = std::fs::remove_file(&path);
    let err = run_spec(&format!("< {} | hole", path.display())).unwrap_err();
    assert!(
        err.to_string().contains("file read") || err.to_string().contains("No such"),
        "{err}"
    );
}

#[test]
fn file_in_must_be_first() {
    let path = tmp("not-first-in.txt");
    std::fs::write(&path, "x\n").unwrap();
    let err = run_spec(&format!("literal z | < {} | hole", path.display())).unwrap_err();
    assert!(err.to_string().contains("first stage"), "{err}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_replace_creates_and_overwrites() {
    let path = tmp("replace.txt");
    std::fs::write(&path, "old\n").unwrap();
    run_spec(&format!("literal hi|> {}|hole", path.display())).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hi\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_append_creates_or_appends() {
    let path = tmp("append.txt");
    let _ = std::fs::remove_file(&path);
    run_spec(&format!("literal first|>> {}|hole", path.display())).unwrap();
    run_spec(&format!("literal second|>> {}|hole", path.display())).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_replace_and_append_must_not_be_first() {
    let path = tmp("first-out.txt");
    let err = run_spec(&format!("> {} | hole", path.display())).unwrap_err();
    assert!(err.to_string().contains("first stage"), "{err}");
    let err = run_spec(&format!(">> {} | hole", path.display())).unwrap_err();
    assert!(err.to_string().contains("first stage"), "{err}");
}

#[test]
fn file_writers_copy_through() {
    // > / >> do not delay the record: they write the file and copy to output.
    let a = tmp("copy-a.txt");
    let b = tmp("copy-b.txt");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    run_spec(&format!("literal visible|> {}|> {}|hole", a.display(), b.display())).unwrap();
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "visible\n");
    assert_eq!(std::fs::read_to_string(&b).unwrap(), "visible\n");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
}

#[test]
fn file_path_is_required() {
    let err = run_spec("< | hole").unwrap_err();
    assert!(err.to_string().contains("path"), "{err}");
    let err = run_spec("literal a|>|hole").unwrap_err();
    assert!(err.to_string().contains("path"), "{err}");
}
