//! FILEUPDATE — replace records in a host file.
//!
//! z/VM CMS Pipelines Reference (FILEUPDATE). Must not be first. Each input
//! record is at least 11 bytes: columns 1-10 are the 1-based record number
//! (spaces allowed; the number need not be aligned), and column 11 through
//! the end is the replacement. Copies input through when an output is
//! connected. DISKUPDATE is a synonym.
//!
//! Host-file subset: a replacement may be a different length than the
//! original record; later records are kept (CMS V-format would truncate).
//! CMS `VARIABLE` / `FIXED` / blocked updates are not implemented. A
//! missing file or a record number that is not in the file is an error.

use pipe_core::{StageCtx, StreamId};

use crate::file_io::{file_path, load_host_records, save_host_records};

pub fn fileupdate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_fileupdate(ctx, "FILEUPDATE", args))
}

pub fn diskupdate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_fileupdate(ctx, "DISKUPDATE", args))
}

fn run_fileupdate(ctx: &mut StageCtx, verb: &str, args: Vec<String>) {
    if !ctx.input_connected(StreamId::PRIMARY) {
        ctx.fail(format!("{verb} must not be a first stage"));
        return;
    }
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail(format!("{verb}: only the primary input may be connected"));
            return;
        }
    }
    if let Some(extra) = args.get(1) {
        ctx.fail(format!("{verb}: unrecognized operand {extra}"));
        return;
    }
    let path = match file_path(&args, verb) {
        Ok(p) => p,
        Err(e) => {
            ctx.fail(e);
            return;
        }
    };
    let mut recs = match load_host_records(&path) {
        Ok(r) => r,
        Err(e) => {
            ctx.fail(format!("file read {path}: {e}"));
            return;
        }
    };

    let copy = ctx.output_connected(StreamId::PRIMARY);
    ctx.select_input(StreamId::PRIMARY);
    if copy {
        ctx.select_output(StreamId::PRIMARY);
    }
    loop {
        match ctx.read() {
            Ok(Some(r)) => {
                match parse_update(r.as_bytes()) {
                    Ok((num, data)) => {
                        let n = recs.len() as u64;
                        if num < 1 || num > n {
                            ctx.fail(format!(
                                "{verb}: record {num} is not in the file ({n} records)"
                            ));
                            return;
                        }
                        recs[(num - 1) as usize] = data;
                    }
                    Err(e) => {
                        ctx.fail(format!("{verb}: {e}"));
                        return;
                    }
                }
                if copy && ctx.output_connected(StreamId::PRIMARY) && ctx.write(r).is_err() {
                    return;
                }
            }
            Ok(None) => break,
            Err(_) => return,
        }
    }
    if let Err(e) = save_host_records(&path, &recs) {
        ctx.fail(format!("file write {path}: {e}"));
    }
}

fn parse_update(bytes: &[u8]) -> Result<(u64, Vec<u8>), String> {
    if bytes.len() < 11 {
        return Err(
            "each input record must be at least 11 bytes (record number in columns 1-10)".into(),
        );
    }
    let field = std::str::from_utf8(&bytes[..10])
        .map_err(|_| "columns 1-10 must be an ASCII record number".to_string())?;
    let field = field.trim();
    if field.is_empty() || !field.bytes().all(|b| b.is_ascii_digit()) {
        return Err("columns 1-10 must be a record number".into());
    }
    let num: u64 = field
        .parse()
        .map_err(|_| "columns 1-10 must be a record number".to_string())?;
    if num == 0 {
        return Err("record numbers start at 1".into());
    }
    Ok((num, bytes[10..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_need_not_be_aligned() {
        let (n, data) = parse_update(b"    5     Four vases").unwrap();
        assert_eq!(n, 5);
        assert_eq!(data, b"Four vases");
    }

    #[test]
    fn right_justified_ten() {
        let (n, data) = parse_update(b"         7Four pens").unwrap();
        assert_eq!(n, 7);
        assert_eq!(data, b"Four pens");
    }

    #[test]
    fn short_record_is_an_error() {
        assert!(parse_update(b"3 hello").is_err());
    }
}
