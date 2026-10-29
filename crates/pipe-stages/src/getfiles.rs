//! GETFILES — Author's Edition 1.1.11 p.368.
//!
//! Reads host files whose names arrive as input records. Each non-blank
//! record is a host path. File contents are written as records the same way
//! `<` does (LF ends a record; a CR immediately before that LF is stripped).
//! A missing file is an error unless the secondary output is connected, in
//! which case the path is written there and the stage continues.

use std::io::BufRead;

use pipe_core::{Record, StageCtx, StreamId};

pub fn getfiles(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Some(s) = args.first() {
            ctx.fail(format!("GETFILES: unrecognized operand {s}"));
            return;
        }
        run_getfiles(ctx);
    })
}

fn run_getfiles(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("GETFILES: only the primary input may be connected");
            return;
        }
    }
    if !ctx.input_connected(StreamId::PRIMARY) {
        ctx.fail("GETFILES must not be a first stage");
        return;
    }

    let secondary = ctx.output_connected(StreamId::SECONDARY);
    ctx.select_input(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                let path = rec.to_string_lossy();
                let path = path.trim();
                if path.is_empty() {
                    continue;
                }
                if !write_file_records(ctx, path, secondary) {
                    return;
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

fn write_file_records(ctx: &mut StageCtx, path: &str, secondary: bool) -> bool {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            if secondary {
                if !ctx.output_connected(StreamId::SECONDARY) {
                    return true;
                }
                ctx.select_output(StreamId::SECONDARY);
                return ctx.write(Record::from(path)).is_ok();
            }
            ctx.fail(format!("GETFILES: file read {path}: {e}"));
            return false;
        }
    };
    let mut reader = std::io::BufReader::new(f);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return false;
        }
        match read_host_record(&mut reader) {
            Ok(Some(line)) => {
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(Record::from(line)).is_err() {
                    return false;
                }
            }
            Ok(None) => return true,
            Err(e) => {
                ctx.fail(format!("GETFILES: file read {path}: {e}"));
                return false;
            }
        }
    }
}

fn read_host_record<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
    Ok(Some(buf))
}
