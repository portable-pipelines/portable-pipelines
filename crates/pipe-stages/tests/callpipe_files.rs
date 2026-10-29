//! Per-file CALLPIPE over host files: TAKE stops before later files are
//! opened. Complements `callpipe_streaming.rs`. Bounded RSS for the same
//! shape lives in `callpipe_files_memory.rs` (its own process, so sibling
//! tests do not inflate VmHWM). Modest file sets so the tests stay fast.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId};
use pipe_stages::{callpipe_from_spec, StageRegistry};

fn tmp_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "cms-pipelines-callpipe-files-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn write_file(dir: &Path, n: usize, lines: usize) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("f{n:04}.txt"));
    let mut body = String::new();
    for ln in 1..=lines {
        body.push_str(&format!("file={n:04} line={ln:04}\n"));
    }
    fs::write(&path, body).unwrap();
    path
}

fn take_n(n: usize, out: Rc<RefCell<Vec<String>>>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let mut seen = 0usize;
        while seen < n {
            match ctx.read() {
                Ok(Some(r)) => {
                    out.borrow_mut().push(r.to_string_lossy());
                    seen += 1;
                }
                _ => break,
            }
        }
        ctx.short_input();
    })
}

/// TAKE 10 with two 5-line files and six missing later paths. Streaming
/// CALLPIPE must not open the missing files (that would fail the pipeline).
#[test]
fn per_file_callpipe_take_does_not_open_later_files() {
    let root = tmp_root();
    let files_dir = root.join("files");
    let existing: Vec<PathBuf> = (1..=2).map(|n| write_file(&files_dir, n, 5)).collect();
    let mut paths: Vec<String> = existing
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    for n in 3..=8 {
        paths.push(
            files_dir
                .join(format!("f{n:04}.txt"))
                .to_string_lossy()
                .into_owned(),
        );
    }

    let collected = Rc::new(RefCell::new(Vec::new()));
    let started = Rc::new(RefCell::new(0usize));
    let out = Rc::clone(&collected);
    let n_started = Rc::clone(&started);
    let path_list = paths.clone();

    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(move |ctx| {
            for p in path_list {
                if ctx.write(Record::from(p)).is_err() {
                    break;
                }
            }
        }),
    );
    let caller = b.add_stage(
        "caller",
        Box::new(move |ctx| loop {
            if !ctx.output_connected(StreamId::PRIMARY) {
                ctx.short_input();
                break;
            }
            match ctx.read() {
                Ok(Some(rec)) => {
                    let path = rec.to_string_lossy();
                    *n_started.borrow_mut() += 1;
                    let spec = format!("< {path} | copy | *:");
                    callpipe_from_spec(ctx, &spec, &StageRegistry::default()).unwrap();
                }
                _ => break,
            }
        }),
    );
    let snk = b.add_stage("sink", take_n(10, out));
    b.connect_primary(src, caller);
    b.connect_primary(caller, snk);
    b.run().unwrap();

    let got = collected.borrow().clone();
    assert_eq!(got.len(), 10, "{got:?}");
    assert_eq!(got[0], "file=0001 line=0001");
    assert_eq!(got[4], "file=0001 line=0005");
    assert_eq!(got[5], "file=0002 line=0001");
    assert_eq!(got[9], "file=0002 line=0005");
    assert!(
        *started.borrow() <= 2,
        "opened {} files; missing later paths must stay unread",
        started.borrow()
    );

    let _ = fs::remove_dir_all(&root);
}

/// Same early-stop shape as the PIPCMD demo: command records plus
/// `pipcmd stop anyeof | take 10`. Later listed files do not exist.
#[test]
fn pipcmd_per_file_callpipe_take_does_not_open_later_files() {
    let root = tmp_root();
    let files_dir = root.join("files");
    let _ = write_file(&files_dir, 1, 5);
    let _ = write_file(&files_dir, 2, 5);
    let mut commands = Vec::new();
    for n in 1..=8 {
        let path = files_dir.join(format!("f{n:04}.txt"));
        commands.push(format!(
            "callpipe < {} | xlate 1-* upper | *:",
            path.display()
        ));
    }
    let cmds_path = root.join("commands.txt");
    fs::write(&cmds_path, commands.join("\n") + "\n").unwrap();

    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let out = Rc::clone(&out);
        move |_| pipe_stages::collect_to(Rc::clone(&out))
    });
    let spec = format!(
        "< {} | pipcmd stop anyeof | take 10 | >>COLLECT",
        cmds_path.display()
    );
    let parsed = pipe_parser::parse_pipeline(&spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();

    let got = collected.borrow().clone();
    assert_eq!(got.len(), 10, "{got:?}");
    assert_eq!(got[0], "FILE=0001 LINE=0001");
    assert_eq!(got[4], "FILE=0001 LINE=0005");
    assert_eq!(got[9], "FILE=0002 LINE=0005");

    let _ = fs::remove_dir_all(&root);
}

/// TAKE 10 in the middle of a file (4 lines each). XLATE must short its
/// input so the nested `<` ends and PIPCMD can STOP, rather than stall.
#[test]
fn pipcmd_take_stops_mid_file_without_stall() {
    let root = tmp_root();
    let files_dir = root.join("files");
    for n in 1..=8 {
        let _ = write_file(&files_dir, n, 4);
    }
    let mut commands = Vec::new();
    for n in 1..=8 {
        let path = files_dir.join(format!("f{n:04}.txt"));
        commands.push(format!(
            "callpipe < {} | xlate 1-* upper | *:",
            path.display()
        ));
    }
    let cmds_path = root.join("commands.txt");
    fs::write(&cmds_path, commands.join("\n") + "\n").unwrap();

    let collected = Rc::new(RefCell::new(Vec::new()));
    let out = Rc::clone(&collected);
    let mut reg = StageRegistry::default();
    reg.register(">>COLLECT", {
        let out = Rc::clone(&out);
        move |_| pipe_stages::collect_to(Rc::clone(&out))
    });
    let spec = format!(
        "< {} | pipcmd stop anyeof | take 10 | >>COLLECT",
        cmds_path.display()
    );
    let parsed = pipe_parser::parse_pipeline(&spec).expect("parse");
    pipe_stages::run_parsed(&parsed, &reg).unwrap();

    let got = collected.borrow().clone();
    assert_eq!(got.len(), 10, "{got:?}");
    assert_eq!(got[0], "FILE=0001 LINE=0001");
    assert_eq!(got[3], "FILE=0001 LINE=0004");
    assert_eq!(got[8], "FILE=0003 LINE=0001");
    assert_eq!(got[9], "FILE=0003 LINE=0002");

    let _ = fs::remove_dir_all(&root);
}
