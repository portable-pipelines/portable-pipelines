//! VAR - Single variable in a REXX or CLIST pool.
//!
//! Author's Edition 1.1.11 p.580.
//!
//! Syntax: `VAR name`
//!
//! When first (device driver), reads from the REXX variable and writes its
//! content as records (split on newlines).
//!
//! When not first (sink), reads records from the pipeline and stores them in
//! the named variable as a multi-line value (records joined with newlines).
//!
//! The variable pool is the Regina variable space of the calling REXX program.
//! This stage only works when invoked from a REXX program running through Regina.

use pipe_core::{PipeError, Record, StageCtx, StreamId};

pub fn var(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if args.is_empty() {
            ctx.fail("VAR: variable name required");
            return;
        }

        if args.len() > 1 {
            ctx.fail("VAR: too many operands");
            return;
        }

        let var_name = &args[0];

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("VAR: no input stream other than the primary may be connected");
                return;
            }
        }

        let is_first = !ctx.input_connected(StreamId::PRIMARY);
        if is_first {
            run_var_source(ctx, var_name);
        } else {
            run_var_sink(ctx, var_name);
        }
    })
}

fn run_var_source(ctx: &mut StageCtx, var_name: &str) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        return;
    }

    let content = match get_rexx_variable(var_name) {
        Ok(val) => val,
        Err(e) => {
            ctx.fail(format!("VAR {var_name}: {e}"));
            return;
        }
    };

    ctx.select_output(StreamId::PRIMARY);
    for line in content.split('\n') {
        if ctx.write(Record::from(line)).is_err() {
            return;
        }
    }
}

fn run_var_sink(ctx: &mut StageCtx, var_name: &str) {
    let mut lines = Vec::new();
    ctx.select_input(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => lines.push(rec.to_string_lossy()),
            Ok(None) => break,
            Err(PipeError::Severed) => break,
            Err(_) => return,
        }
    }

    let content = lines.join("\n");
    if let Err(e) = set_rexx_variable(var_name, &content) {
        ctx.fail(format!("VAR {var_name}: {e}"));
    }
}

fn get_rexx_variable(name: &str) -> Result<String, String> {
    crate::rexx_varpool::get_variable(name)
}

fn set_rexx_variable(name: &str, value: &str) -> Result<(), String> {
    crate::rexx_varpool::set_variable(name, value)
}
