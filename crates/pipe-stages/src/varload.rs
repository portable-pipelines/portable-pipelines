//! VARLOAD - Set variables in a REXX or CLIST pool from records.
//!
//! Author's Edition 1.1.11 p.587.
//!
//! Syntax:
//! `VARLOAD`
//!
//! Reads records from the primary input. Each record should be in the form
//! `name value` (variable name, blank(s), value). Sets each variable in the
//! REXX pool. Copies records to the primary output (filter stage).
//!
//! The variable pool is the Regina variable space of the calling REXX program.
//! This stage only works when invoked from a REXX program running through Regina.

use pipe_core::{StageCtx, StreamId};

pub fn varload(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !args.is_empty() {
            ctx.fail(format!("VARLOAD: unexpected operand {}", args[0]));
            return;
        }

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("VARLOAD: no input stream other than the primary may be connected");
                return;
            }
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);
        loop {
            match ctx.read() {
                Ok(Some(rec)) => {
                    let line = rec.to_string_lossy();
                    if let Err(e) = process_varload_line(&line) {
                        ctx.fail(format!("VARLOAD: {e}"));
                        return;
                    }
                    if ctx.write(rec).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(_) => return,
            }
        }
    })
}

fn process_varload_line(line: &str) -> Result<(), String> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return Ok(());
    }

    match trimmed.split_once(|c: char| c.is_ascii_whitespace()) {
        Some((name, value)) => {
            let trimmed_value = value.trim_start();
            set_rexx_variable(name, trimmed_value)?;
            Ok(())
        }
        None => {
            set_rexx_variable(trimmed, "")?;
            Ok(())
        }
    }
}

fn set_rexx_variable(name: &str, value: &str) -> Result<(), String> {
    crate::rexx_varpool::set_variable(name, value)
}

