//! VARFETCH - Fetch a variable from a REXX or CLIST pool.
//!
//! Author's Edition 1.1.11 p.585.
//!
//! Syntax:
//! `VARFETCH name`
//!
//! Fetches the value of the named variable and writes it as a single record.
//! Must be first stage in the pipeline (device driver).
//!
//! The variable pool is the Regina variable space of the calling REXX program.
//! This stage only works when invoked from a REXX program running through Regina.

use pipe_core::{Record, StageCtx, StreamId};

pub fn varfetch(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if args.is_empty() {
            ctx.fail("VARFETCH: variable name required");
            return;
        }

        if args.len() > 1 {
            ctx.fail("VARFETCH: too many operands");
            return;
        }

        let var_name = &args[0];

        for stream in ctx.connected_inputs() {
            ctx.fail(format!(
                "VARFETCH: input stream {} may not be connected (must be first)",
                stream.0
            ));
            return;
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }

        let value = match get_rexx_variable(var_name) {
            Ok(val) => val,
            Err(e) => {
                ctx.fail(format!("VARFETCH {var_name}: {e}"));
                return;
            }
        };

        ctx.select_output(StreamId::PRIMARY);
        let _ = ctx.write(Record::from(value));
    })
}

fn get_rexx_variable(name: &str) -> Result<String, String> {
    crate::rexx_varpool::get_variable(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varfetch_requires_name() {
        use pipe_core::PipelineBuilder;
        let mut b = PipelineBuilder::new();
        let id = b.add_stage("VARFETCH", varfetch(vec![]));
        let result = b.run();
        assert!(result.is_err());
    }
}
