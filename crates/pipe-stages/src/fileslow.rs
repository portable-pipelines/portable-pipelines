//! FILESLOW — read or write a host file beginning at a record number.
//!
//! z/VM CMS Pipelines Reference (FILESLOW). The operand is one host path
//! (not CMS `fn ft fm`), then optional `FROM recno`. Without FROM this is
//! the same as FILEFAST / DISK (missing file is empty when first; not-first
//! appends). `FROM recno` starts the read or write at that 1-based record.
//!
//! CMS `VARIABLE` / `FIXED` / `lrecl` and the secondary-stream header overlay
//! are not implemented. DISKSLOW is a synonym.

use pipe_core::{Record, StageCtx, StreamId};

use crate::file_io::{disk_named, file_path, load_host_records, save_host_records};

pub fn fileslow(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_fileslow(ctx, "FILESLOW", args))
}

pub fn diskslow(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_fileslow(ctx, "DISKSLOW", args))
}

fn run_fileslow(ctx: &mut StageCtx, verb: &'static str, args: Vec<String>) {
    let from = match parse_fileslow(&args, verb) {
        Ok(f) => f,
        Err(e) => {
            ctx.fail(format!("{verb}: {e}"));
            return;
        }
    };
    let Some(recno) = from else {
        disk_named(verb, args)(ctx);
        return;
    };
    let path = match file_path(&args, verb) {
        Ok(p) => p,
        Err(e) => {
            ctx.fail(e);
            return;
        }
    };
    if !ctx.input_connected(StreamId::PRIMARY) {
        read_from(ctx, verb, &path, recno);
    } else {
        write_from(ctx, verb, &path, recno);
    }
}

fn parse_fileslow(args: &[String], verb: &str) -> Result<Option<u64>, String> {
    let _ = file_path(args, verb)?;
    let mut i = 1;
    let mut from = None;
    while i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u == "FROM" {
            i += 1;
            let rec = args.get(i).ok_or("FROM requires a record number")?;
            from = Some(parse_recno(rec)?);
            i += 1;
        } else if u == "VARIABLE" || u == "FIXED" {
            return Err(format!("{u} is not implemented (CMS record format)"));
        } else if args[i].bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!(
                "unrecognized operand {}; use FROM {n}",
                args[i],
                n = args[i]
            ));
        } else {
            return Err(format!("unrecognized operand {}", args[i]));
        }
    }
    Ok(from)
}

fn parse_recno(s: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("FROM requires a record number, not {s}"));
    }
    let n: u64 = s
        .parse()
        .map_err(|_| format!("FROM requires a record number, not {s}"))?;
    if n == 0 {
        return Err("record numbers start at 1".into());
    }
    Ok(n)
}

fn read_from(ctx: &mut StageCtx, verb: &str, path: &str, recno: u64) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        return;
    }
    let recs = match load_host_records(path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            ctx.fail(format!("file read {path}: {e}"));
            return;
        }
        Err(e) => {
            ctx.fail(format!("file read {path}: {e}"));
            return;
        }
    };
    let n = recs.len() as u64;
    if recno > n {
        ctx.fail(format!(
            "{verb}: FROM {recno} is past the end of the file ({n} records)"
        ));
        return;
    }
    ctx.select_output(StreamId::PRIMARY);
    for rec in recs.into_iter().skip((recno - 1) as usize) {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.write(Record::from(rec)).is_err() {
            return;
        }
    }
}

fn write_from(ctx: &mut StageCtx, verb: &str, path: &str, recno: u64) {
    let existing = match load_host_records(path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => {
            ctx.fail(format!("file read {path}: {e}"));
            return;
        }
    };
    let n = existing.len() as u64;
    if recno > n + 1 {
        ctx.fail(format!(
            "{verb}: FROM {recno} is past one past the end of the file ({n} records)"
        ));
        return;
    }

    let copy = ctx.output_connected(StreamId::PRIMARY);
    ctx.select_input(StreamId::PRIMARY);
    if copy {
        ctx.select_output(StreamId::PRIMARY);
    }
    let mut incoming = Vec::new();
    loop {
        match ctx.read() {
            Ok(Some(r)) => {
                if !r.as_bytes().is_empty() {
                    incoming.push(r.as_bytes().to_vec());
                }
                if copy && ctx.output_connected(StreamId::PRIMARY) && ctx.write(r).is_err() {
                    return;
                }
            }
            Ok(None) => break,
            Err(_) => return,
        }
    }

    let mut out = existing;
    let start = (recno - 1) as usize;
    if start > out.len() {
        ctx.fail(format!("{verb}: FROM {recno} is past the end of the file"));
        return;
    }
    let replace_end = (start + incoming.len()).min(out.len());
    out.splice(start..replace_end, incoming);
    if let Err(e) = save_host_records(path, &out) {
        ctx.fail(format!("file write {path}: {e}"));
    }
}
