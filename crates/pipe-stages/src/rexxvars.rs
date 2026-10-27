//! REXXVARS - Retrieve variables from a REXX or CLIST pool.
//!
//! Author's Edition 1.1.11 p.477.
//!
//! Syntax:
//! `REXXVARS`
//!
//! Must be first stage (device driver). Writes one record for each variable
//! in the REXX pool, in the form `name value`. This is a snapshot of the
//! variables visible at the time the stage runs.
//!
//! The variable pool is the Regina variable space of the calling REXX program.
//! This stage only works when invoked from a REXX program running through Regina.

use pipe_core::{Record, StageCtx, StreamId};

pub fn rexxvars(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !args.is_empty() {
            ctx.fail(format!("REXXVARS: unexpected operand {}", args[0]));
            return;
        }

        for stream in ctx.connected_inputs() {
            ctx.fail(format!(
                "REXXVARS: input stream {} may not be connected (must be first)",
                stream.0
            ));
            return;
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }

        let vars = match list_rexx_variables() {
            Ok(v) => v,
            Err(e) => {
                ctx.fail(format!("REXXVARS: {e}"));
                return;
            }
        };

        ctx.select_output(StreamId::PRIMARY);
        for (name, value) in vars {
            let line = format!("{} {}", name, value);
            if ctx.write(Record::from(line)).is_err() {
                return;
            }
        }
    })
}

fn list_rexx_variables() -> Result<Vec<(String, String)>, String> {
    crate::rexx_varpool::list_variables()
}

