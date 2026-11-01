//! Per-file streaming CALLPIPE must not buffer the whole file set. Peak
//! RSS is sampled on Linux; other hosts still check that every record
//! arrives. This lives in its own test binary so sibling tests do not
//! inflate `/proc/self/status` VmHWM. Modest file set (fast in debug).

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StreamId};
use pipe_stages::{callpipe_from_spec, StageRegistry};

const NFILES: usize = 8;
const NLINES: usize = 10_000;
const PAYLOAD: usize = 80;

#[cfg(target_os = "linux")]
fn peak_rss_kb() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kb = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb);
        }
    }
    None
}

#[test]
fn streaming_per_file_callpipe_memory_stays_bounded() {
    let root = std::env::temp_dir().join(format!(
        "cms-pipelines-callpipe-files-mem-{}",
        std::process::id()
    ));
    let files_dir = root.join("files");
    fs::create_dir_all(&files_dir).unwrap();
    let mut paths = Vec::new();
    for n in 1..=NFILES {
        let path = files_dir.join(format!("f{n:04}.txt"));
        let line = format!("{}\n", "x".repeat(PAYLOAD));
        let mut body = String::with_capacity(NLINES * (PAYLOAD + 1));
        for _ in 0..NLINES {
            body.push_str(&line);
        }
        fs::write(&path, body).unwrap();
        paths.push(path.to_string_lossy().into_owned());
    }

    let count = Rc::new(RefCell::new(0usize));
    let n = Rc::clone(&count);
    #[cfg(target_os = "linux")]
    let before = peak_rss_kb();

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
        Box::new(|ctx| {
            while let Ok(Some(rec)) = ctx.read() {
                if !ctx.output_connected(StreamId::PRIMARY) {
                    break;
                }
                let path = rec.to_string_lossy();
                let spec = format!("< {path} | copy | *:");
                callpipe_from_spec(ctx, &spec, &StageRegistry::default()).unwrap();
            }
        }),
    );
    let snk = b.add_stage(
        "sink",
        Box::new(move |ctx| {
            while let Ok(Some(r)) = ctx.read() {
                assert_eq!(r.len(), PAYLOAD);
                *n.borrow_mut() += 1;
            }
        }),
    );
    b.connect_primary(src, caller);
    b.connect_primary(caller, snk);
    b.run().unwrap();
    assert_eq!(*count.borrow(), NFILES * NLINES);

    #[cfg(target_os = "linux")]
    {
        let after = peak_rss_kb();
        if let (Some(before), Some(after)) = (before, after) {
            let delta = after.saturating_sub(before);
            // Payload is about 6.25 MiB. Buffered CALLPIPE would hold input
            // and output copies. Streaming should stay near coroutine stacks,
            // file buffers, and a few records. Same rule as
            // callpipe_memory.rs: delta must stay under the payload size.
            let payload_kb = ((NFILES * NLINES * PAYLOAD) / 1024) as u64;
            assert!(
                delta < payload_kb,
                "peak RSS grew {delta} kB; payload is {payload_kb} kB (before={before} after={after})"
            );
        }
    }

    let _ = fs::remove_dir_all(&root);
}
