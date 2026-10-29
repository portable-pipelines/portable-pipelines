//! Streaming CALLPIPE must not buffer the whole file. Peak RSS is sampled
//! on Linux; other hosts still check that every record arrives.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record};
use pipe_stages::{callpipe_from_spec, StageRegistry};

const N: usize = 1_000_000;
const PAYLOAD: usize = 32;

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
fn streaming_callpipe_memory_stays_bounded() {
    let count = Rc::new(RefCell::new(0usize));
    let n = Rc::clone(&count);
    #[cfg(target_os = "linux")]
    let before = peak_rss_kb();

    let mut b = PipelineBuilder::new();
    let src = b.add_stage(
        "src",
        Box::new(|ctx| {
            let rec = Record::from(vec![b'x'; PAYLOAD]);
            for _ in 0..N {
                if ctx.write(rec.clone()).is_err() {
                    break;
                }
            }
        }),
    );
    let caller = b.add_stage(
        "caller",
        Box::new(|ctx| {
            callpipe_from_spec(ctx, "*: | copy | *:", &StageRegistry::default()).unwrap();
        }),
    );
    let sink = b.add_stage(
        "sink",
        Box::new(move |ctx| {
            while let Ok(Some(r)) = ctx.read() {
                assert_eq!(r.len(), PAYLOAD);
                *n.borrow_mut() += 1;
            }
        }),
    );
    b.connect_primary(src, caller);
    b.connect_primary(caller, sink);
    b.run().unwrap();
    assert_eq!(*count.borrow(), N);

    #[cfg(target_os = "linux")]
    {
        let after = peak_rss_kb();
        if let (Some(before), Some(after)) = (before, after) {
            let delta = after.saturating_sub(before);
            // Payload is about 32 MiB. Buffered CALLPIPE would hold input and
            // output copies (tens of MiB). Streaming should stay near
            // coroutine stacks plus a few records. Allow a generous slop so
            // allocator noise does not flake CI.
            let payload_kb = ((N * PAYLOAD) / 1024) as u64;
            assert!(
                delta < payload_kb,
                "peak RSS grew {delta} kB; payload is {payload_kb} kB (before={before} after={after})"
            );
        }
    }
}
