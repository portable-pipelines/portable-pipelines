//! Shared host-file record I/O for DISK, `<`, `>`, `>>`, and the FILE* stages.
//!
//! A record ends at LF. A single CR immediately before that LF is stripped.
//! Bytes are otherwise passed through unchanged.

use std::io::{BufRead, Write};

use pipe_core::{Record, StageCtx, StreamId};

pub(crate) fn file_path(args: &[String], verb: &str) -> Result<String, String> {
    if args.is_empty() || args[0].is_empty() {
        return Err(format!("{verb}: a path is required"));
    }
    Ok(args[0].clone())
}

pub(crate) fn reject_invented_disk_mode(path: &str) -> Result<(), String> {
    let mut parts = path.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("");
    if (first.eq_ignore_ascii_case("OUTPUT") || first.eq_ignore_ascii_case("APPEND"))
        && parts.next().is_some()
    {
        return Err("OUTPUT/APPEND are not IBM operands; use > or >>".into());
    }
    Ok(())
}

/// Split one host-stream record: LF ends the record; a single CR immediately
/// before that LF is stripped. Bytes are otherwise passed through unchanged.
pub(crate) fn read_host_record<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Vec<u8>>> {
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

pub(crate) fn write_host_record<W: Write>(writer: &mut W, rec: &Record) -> std::io::Result<()> {
    writer.write_all(rec.as_bytes())?;
    writer.write_all(b"\n")?;
    Ok(())
}

pub(crate) fn write_host_bytes<W: Write>(writer: &mut W, rec: &[u8]) -> std::io::Result<()> {
    writer.write_all(rec)?;
    writer.write_all(b"\n")?;
    Ok(())
}

pub(crate) fn file_read(ctx: &mut StageCtx, path: &str, missing_is_empty: bool) {
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && missing_is_empty => return,
        Err(e) => {
            ctx.fail(format!("file read {path}: {e}"));
            return;
        }
    };
    let mut reader = std::io::BufReader::new(f);
    loop {
        match read_host_record(&mut reader) {
            Ok(Some(line)) => {
                if ctx.write(Record::from(line)).is_err() {
                    break;
                }
            }
            Ok(None) => break,
            Err(e) => {
                ctx.fail(format!("file read: {e}"));
                return;
            }
        }
    }
}

pub(crate) fn file_write(ctx: &mut StageCtx, path: &str, append: bool) {
    let mut f = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => {
            ctx.fail(format!("file write {path}: {e}"));
            return;
        }
    };
    // > / >> / diskfast copy through: do not delay the record.
    while let Ok(Some(r)) = ctx.read() {
        if let Err(e) = write_host_record(&mut f, &r) {
            ctx.fail(format!("file write {path}: {e}"));
            return;
        }
        if ctx.output_connected(StreamId::PRIMARY) && ctx.write(r).is_err() {
            break;
        }
    }
}

pub(crate) fn load_host_records(path: &str) -> std::io::Result<Vec<Vec<u8>>> {
    let f = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(f);
    let mut recs = Vec::new();
    loop {
        match read_host_record(&mut reader)? {
            Some(line) => recs.push(line),
            None => return Ok(recs),
        }
    }
}

pub(crate) fn save_host_records(path: &str, recs: &[Vec<u8>]) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    for rec in recs {
        write_host_bytes(&mut f, rec)?;
    }
    Ok(())
}

/// DISK / FILEFAST / DISKFAST: first stage reads (missing file is empty);
/// not-first appends or creates and copies through.
pub(crate) fn disk_named(verb: &'static str, args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let path = match file_path(&args, verb) {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(e);
                return;
            }
        };
        if let Err(e) = reject_invented_disk_mode(&path) {
            ctx.fail(format!("{verb}: {e}"));
            return;
        }
        if !ctx.input_connected(StreamId::PRIMARY) {
            file_read(ctx, &path, true);
        } else {
            file_write(ctx, &path, true);
        }
    })
}
