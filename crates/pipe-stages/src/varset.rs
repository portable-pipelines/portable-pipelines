//! VARSET - Set a variable in a REXX or CLIST pool.
//!
//! Author's Edition 1.1.11 p.590.
//!
//! Syntax:
//! `VARSET name [delimitedString]`
//!
//! Sets the named variable to the value of the delimited string (or empty if omitted).
//! Copies records from primary input to primary output (filter stage).
//!
//! The variable pool is the Regina variable space of the calling REXX program.
//! This stage only works when invoked from a REXX program running through Regina.

use pipe_core::{StageCtx, StreamId};
use crate::common::decode_dstring_text;

pub fn varset(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if args.is_empty() {
            ctx.fail("VARSET: variable name required");
            return;
        }

        let var_name = &args[0];
        let value = if args.len() > 1 {
            let arg_str = args[1..].join(" ");
            match decode_dstring_text(&arg_str) {
                Ok(s) => s,
                Err(e) => {
                    ctx.fail(format!("VARSET: {e}"));
                    return;
                }
            }
        } else {
            String::new()
        };

        if let Err(e) = set_rexx_variable(var_name, &value) {
            ctx.fail(format!("VARSET {var_name}: {e}"));
            return;
        }

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("VARSET: no input stream other than the primary may be connected");
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

fn set_rexx_variable(name: &str, value: &str) -> Result<(), String> {
    crate::rexx_varpool::set_variable(name, value)
}

