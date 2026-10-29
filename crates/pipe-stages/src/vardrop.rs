//! VARDROP — Author's Edition 1.1.11 p.583.
//!
//! Drop variables in a REXX pool. Operand names are dropped first. Each
//! primary input record then drops the variable named by its first word
//! (the whole record if it has no blank). Copies records through when an
//! output is connected. A name ending in `.` drops that stem. Regina and
//! CALLPIPE; fails clearly when there is no in-process REXX pool.

use pipe_core::{StageCtx, StreamId};

use crate::civil::split_first_word;

pub fn vardrop(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        for stream in ctx.connected_inputs() {
            if stream != StreamId::PRIMARY {
                ctx.fail("VARDROP: only the primary input may be connected");
                return;
            }
        }

        for name in &args {
            if let Err(e) = drop_rexx_variable(name) {
                ctx.fail(format!("VARDROP {name}: {e}"));
                return;
            }
        }

        if !ctx.input_connected(StreamId::PRIMARY) {
            if args.is_empty() {
                ctx.fail("VARDROP: variable name required");
            }
            return;
        }

        let copy = ctx.output_connected(StreamId::PRIMARY);
        ctx.select_input(StreamId::PRIMARY);
        if copy {
            ctx.select_output(StreamId::PRIMARY);
        }
        loop {
            match ctx.read() {
                Ok(Some(rec)) => {
                    let line = rec.to_string_lossy();
                    let (word, _) = split_first_word(&line);
                    if !word.is_empty() {
                        if let Err(e) = drop_rexx_variable(word) {
                            ctx.fail(format!("VARDROP {word}: {e}"));
                            return;
                        }
                    }
                    if copy && ctx.write(rec).is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(_) => return,
            }
        }
    })
}

fn drop_rexx_variable(name: &str) -> Result<(), String> {
    crate::rexx_varpool::drop_variable(name)
}
