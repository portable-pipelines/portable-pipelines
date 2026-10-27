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

        let copy = ctx.output_connected(StreamId::PRIMARY);
        ctx.select_input(StreamId::PRIMARY);
        if copy {
            ctx.select_output(StreamId::PRIMARY);
        }
        loop {
            match ctx.read() {
                Ok(Some(rec)) => {
                    let line = rec.to_string_lossy();
                    if let Err(e) = process_varload_line(&line) {
                        ctx.fail(format!("VARLOAD: {e}"));
                        return;
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

fn process_varload_line(line: &str) -> Result<(), String> {
    match parse_name_value(line) {
        None => Ok(()),
        Some((name, value)) => set_rexx_variable(&name, &value),
    }
}

/// Name, then the rest of the record as the value (leading blanks on the
/// value stripped). A record that is only blanks is ignored. A name with no
/// value sets the variable to empty.
fn parse_name_value(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.split_once(|c: char| c.is_ascii_whitespace()) {
        Some((name, value)) => Some((name.to_string(), value.trim_start().to_string())),
        None => Some((trimmed.to_string(), String::new())),
    }
}

fn set_rexx_variable(name: &str, value: &str) -> Result<(), String> {
    crate::rexx_varpool::set_variable(name, value)
}

#[cfg(test)]
mod tests {
    use super::parse_name_value;

    #[test]
    fn splits_name_and_value_on_first_blank() {
        assert_eq!(
            parse_name_value("KEY1 value_one"),
            Some(("KEY1".into(), "value_one".into()))
        );
        assert_eq!(
            parse_name_value("KEY2  value  with spaces"),
            Some(("KEY2".into(), "value  with spaces".into()))
        );
        assert_eq!(
            parse_name_value("ONLYNAME"),
            Some(("ONLYNAME".into(), "".into()))
        );
        assert_eq!(parse_name_value(""), None);
        assert_eq!(parse_name_value("   "), None);
        assert_eq!(
            parse_name_value("\tNAME\tval"),
            Some(("NAME".into(), "val".into()))
        );
    }
}
