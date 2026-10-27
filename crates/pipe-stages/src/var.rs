//! VAR - Single variable in a REXX or CLIST pool.
//!
//! Author's Edition 1.1.11 p.580.
//!
//! Syntax:
//! `VAR [INPUT] name`
//! `VAR OUTPUT name`
//!
//! When first (or when INPUT is specified), reads records from the pipeline
//! and stores them in the named variable as a multi-line value (records joined
//! with newlines). When OUTPUT is specified (must not be first), writes the
//! content of the variable as records (split on newlines).
//!
//! The variable pool is the Regina variable space of the calling REXX program.
//! This stage only works when invoked from a REXX program running through Regina.

use pipe_core::{Record, StageCtx, StreamId};

pub fn var(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if args.is_empty() {
            ctx.fail("VAR: variable name required");
            return;
        }

        let mut iter = args.iter();
        let first = iter.next().unwrap();
        let is_output = first.eq_ignore_ascii_case("OUTPUT");
        let is_input = first.eq_ignore_ascii_case("INPUT");

        let var_name = if is_output || is_input {
            match iter.next() {
                Some(name) => name,
                None => {
                    ctx.fail("VAR: variable name required after INPUT/OUTPUT");
                    return;
                }
            }
        } else {
            first
        };

        if iter.next().is_some() {
            ctx.fail("VAR: too many operands");
            return;
        }

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("VAR: no input stream other than the primary may be connected");
                return;
            }
        }

        if is_output {
            run_var_output(ctx, var_name);
        } else {
            run_var_input(ctx, var_name);
        }
    })
}

fn run_var_input(ctx: &mut StageCtx, var_name: &str) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.short_input();
        return;
    }

    let mut lines = Vec::new();
    ctx.select_input(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => lines.push(rec.to_string_lossy()),
            Ok(None) => break,
            Err(_) => return,
        }
    }

    let content = lines.join("\n");
    if let Err(e) = set_rexx_variable(var_name, &content) {
        ctx.fail(format!("VAR INPUT {var_name}: {e}"));
    }
}

fn run_var_output(ctx: &mut StageCtx, var_name: &str) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.short_input();
        return;
    }

    let content = match get_rexx_variable(var_name) {
        Ok(val) => val,
        Err(e) => {
            ctx.fail(format!("VAR OUTPUT {var_name}: {e}"));
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

fn get_rexx_variable(name: &str) -> Result<String, String> {
    crate::rexx_varpool::get_variable(name)
}

fn set_rexx_variable(name: &str, value: &str) -> Result<(), String> {
    crate::rexx_varpool::set_variable(name, value)
}

