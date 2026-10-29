//! STACK — Author's Edition 1.1.11 p.513.
//!
//! Portable in-process program stack. CMS has a session console stack; this
//! port keeps a deque for the life of the `pipe` process.
//!
//! `STACK FIFO` writes at the back and, when first, reads from the front.
//! `STACK LIFO` (the default) writes at the back and, when first, reads from
//! the back. When not first, records are stacked and copied through if an
//! output is connected.

use std::collections::VecDeque;
use std::sync::Mutex;

use pipe_core::{PipeError, Record, StageCtx, StreamId};

static ITEMS: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn items() -> std::sync::MutexGuard<'static, VecDeque<Vec<u8>>> {
    ITEMS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Run `f` with an empty process stack, then clear it again. Serializes tests
/// that share the process-wide deque.
pub fn with_cleared_program_stack<R>(f: impl FnOnce() -> R) -> R {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    items().clear();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    items().clear();
    match result {
        Ok(r) => r,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Fifo,
    Lifo,
}

pub fn stack(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_stack(&args) {
        Ok(mode) => run_stack(ctx, mode),
        Err(e) => ctx.fail(format!("STACK: {e}")),
    })
}

fn parse_stack(args: &[String]) -> Result<Mode, String> {
    match args {
        [] => Ok(Mode::Lifo),
        [s] => {
            let u = s.to_ascii_uppercase();
            if u == "FIFO" {
                Ok(Mode::Fifo)
            } else if u == "LIFO" {
                Ok(Mode::Lifo)
            } else {
                Err(format!("unrecognized operand {s}"))
            }
        }
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_stack(ctx: &mut StageCtx, mode: Mode) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("STACK: only the primary input may be connected");
            return;
        }
    }
    let is_first = !ctx.input_connected(StreamId::PRIMARY);
    if is_first {
        run_stack_source(ctx, mode);
    } else {
        run_stack_sink(ctx);
    }
}

fn run_stack_source(ctx: &mut StageCtx, mode: Mode) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        return;
    }
    ctx.select_output(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        let rec = {
            let mut q = items();
            match mode {
                Mode::Fifo => q.pop_front(),
                Mode::Lifo => q.pop_back(),
            }
        };
        let Some(bytes) = rec else {
            return;
        };
        if ctx.write(Record::from(bytes)).is_err() {
            return;
        }
    }
}

fn run_stack_sink(ctx: &mut StageCtx) {
    let copy = ctx.output_connected(StreamId::PRIMARY);
    ctx.select_input(StreamId::PRIMARY);
    if copy {
        ctx.select_output(StreamId::PRIMARY);
    }
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                items().push_back(rec.as_bytes().to_vec());
                if copy && ctx.write(rec).is_err() {
                    return;
                }
            }
            Ok(None) => return,
            Err(PipeError::Severed) => return,
            Err(_) => return,
        }
    }
}
