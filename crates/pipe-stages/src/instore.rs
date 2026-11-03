//! INSTORE / OUTSTORE — in-process record buffer with an opaque token.
//!
//! z/VM CMS Pipelines Reference (INSTORE, OUTSTORE). INSTORE delays until
//! EOF, keeps the records in this process, and writes one token. OUTSTORE
//! reads that token and writes the stored records (reversed if INSTORE
//! REVERSE was used), then releases the buffer.
//!
//! The token is unique per buffer and is only meaningful to OUTSTORE. A
//! record that is not a token is an error. Buffers are visible to
//! CALLPIPE / subroutine pipelines in the same process. INSTORE ends if
//! its output is not connected. OUTSTORE does not delay records.
//!
//! CMS ALET / PGMLIST storage-key operands are not implemented.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use pipe_core::{PipeError, Record, StageCtx, StreamId};

const MAGIC: &[u8] = b"\x00PPINSTORE\x01";

struct Buffer {
    reverse: bool,
    records: Vec<Vec<u8>>,
}

static BUFFERS: OnceLock<Mutex<HashMap<u64, Buffer>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn buffers() -> std::sync::MutexGuard<'static, HashMap<u64, Buffer>> {
    BUFFERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Run `f` with an empty INSTORE table, then clear it again.
pub fn with_cleared_instore<R>(f: impl FnOnce() -> R) -> R {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    buffers().clear();
    NEXT_ID.store(1, Ordering::SeqCst);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    buffers().clear();
    NEXT_ID.store(1, Ordering::SeqCst);
    match result {
        Ok(r) => r,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn make_token(id: u64) -> Vec<u8> {
    let mut tok = MAGIC.to_vec();
    tok.extend_from_slice(&id.to_be_bytes());
    tok
}

fn parse_token(bytes: &[u8]) -> Option<u64> {
    if bytes.len() != MAGIC.len() + 8 || !bytes.starts_with(MAGIC) {
        return None;
    }
    let mut id = [0u8; 8];
    id.copy_from_slice(&bytes[MAGIC.len()..]);
    Some(u64::from_be_bytes(id))
}

pub fn instore(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let reverse = match parse_instore(&args) {
            Ok(r) => r,
            Err(e) => {
                ctx.fail(format!("INSTORE: {e}"));
                return;
            }
        };
        run_instore(ctx, reverse);
    })
}

pub fn outstore(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Some(s) = args.first() {
            ctx.fail(format!("OUTSTORE: unrecognized operand {s}"));
            return;
        }
        run_outstore(ctx);
    })
}

fn parse_instore(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [s] if s.eq_ignore_ascii_case("REVERSE") => Ok(true),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_instore(ctx: &mut StageCtx, reverse: bool) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("INSTORE: only the primary input may be connected");
            return;
        }
    }
    let write_token = ctx.output_connected(StreamId::PRIMARY);
    ctx.select_input(StreamId::PRIMARY);
    let mut records = Vec::new();
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                if write_token {
                    records.push(rec.as_bytes().to_vec());
                }
            }
            Ok(None) => break,
            Err(PipeError::Severed) => break,
            Err(_) => return,
        }
    }
    if !write_token {
        return;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    buffers().insert(id, Buffer { reverse, records });
    ctx.select_output(StreamId::PRIMARY);
    let _ = ctx.write(Record::from(make_token(id)));
}

fn run_outstore(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("OUTSTORE: only the primary input may be connected");
            return;
        }
    }
    if !ctx.input_connected(StreamId::PRIMARY) {
        return;
    }
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                let Some(id) = parse_token(rec.as_bytes()) else {
                    ctx.fail("OUTSTORE: record is not an INSTORE token");
                    return;
                };
                let Some(buf) = buffers().remove(&id) else {
                    ctx.fail("OUTSTORE: INSTORE buffer is no longer available");
                    return;
                };
                if !write_buffer(ctx, buf) {
                    return;
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

fn write_buffer(ctx: &mut StageCtx, buf: Buffer) -> bool {
    if buf.reverse {
        for rec in buf.records.into_iter().rev() {
            if !ctx.output_connected(StreamId::PRIMARY) {
                return false;
            }
            if ctx.write(Record::from(rec)).is_err() {
                return false;
            }
        }
    } else {
        for rec in buf.records {
            if !ctx.output_connected(StreamId::PRIMARY) {
                return false;
            }
            if ctx.write(Record::from(rec)).is_err() {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_round_trip() {
        let t = make_token(42);
        assert_eq!(parse_token(&t), Some(42));
        assert!(parse_token(b"hello").is_none());
        assert!(parse_token(&t[..t.len() - 1]).is_none());
    }
}
