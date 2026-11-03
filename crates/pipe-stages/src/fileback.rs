//! FILEBACK — read a host file last record first.
//!
//! z/VM CMS Pipelines Reference (FILEBACK). Must be first. DISKBACK is a
//! synonym. A missing file is an error, the same way `<` reports one. The
//! stage ends if its primary output is not connected.
//!
//! The operand is one host path (not CMS `fn ft fm`). The whole file is
//! buffered so records can be written last-to-first.

use pipe_core::{Record, StageCtx, StreamId};

use crate::file_io::{file_path, load_host_records};

pub fn fileback(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_fileback(ctx, "FILEBACK", &args))
}

pub fn diskback(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| run_fileback(ctx, "DISKBACK", &args))
}

fn run_fileback(ctx: &mut StageCtx, verb: &str, args: &[String]) {
    if ctx.input_connected(StreamId::PRIMARY) {
        ctx.fail(format!("{verb} must be a first stage"));
        return;
    }
    if !ctx.output_connected(StreamId::PRIMARY) {
        return;
    }
    let path = match file_path(args, verb) {
        Ok(p) => p,
        Err(e) => {
            ctx.fail(e);
            return;
        }
    };
    let recs = match load_host_records(&path) {
        Ok(r) => r,
        Err(e) => {
            ctx.fail(format!("file read {path}: {e}"));
            return;
        }
    };
    ctx.select_output(StreamId::PRIMARY);
    for rec in recs.into_iter().rev() {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.write(Record::from(rec)).is_err() {
            return;
        }
    }
}
