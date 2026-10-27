//! STEM - REXX stem variables (arrays).
//!
//! Author's Edition 1.1.11 p.528.
//!
//! Syntax: `STEM name`
//!
//! When first (device driver), reads from the REXX stem variable and writes
//! records to the pipeline. STEM.0 should contain the count, and STEM.1,
//! STEM.2, etc. are read.
//!
//! When not first (sink), reads records from the pipeline and stores them in
//! the stem. Each record becomes STEM.i where i starts at 1, and STEM.0 is
//! set to the count.
//!
//! The variable pool is the Regina variable space of the calling REXX program.
//! This stage only works when invoked from a REXX program running through Regina.

use pipe_core::{Record, StageCtx, StreamId};

pub fn stem(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if args.is_empty() {
            ctx.fail("STEM: stem name required");
            return;
        }

        if args.len() > 1 {
            ctx.fail("STEM: too many operands");
            return;
        }

        let stem_base = normalize_stem_name(&args[0]);

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("STEM: no input stream other than the primary may be connected");
                return;
            }
        }

        let is_first = !ctx.input_connected(StreamId::PRIMARY);
        if is_first {
            run_stem_source(ctx, &stem_base);
        } else {
            run_stem_sink(ctx, &stem_base);
        }
    })
}

fn normalize_stem_name(name: &str) -> String {
    let trimmed = name.trim_end_matches('.');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}.", trimmed)
    }
}

fn run_stem_source(ctx: &mut StageCtx, stem_base: &str) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        return;
    }

    let count_var = format!("{}0", stem_base);
    let count_str = match get_rexx_variable(&count_var) {
        Ok(val) => val,
        Err(e) => {
            ctx.fail(format!("STEM {stem_base}: {e}"));
            return;
        }
    };

    let count: usize = match count_str.trim().parse() {
        Ok(n) => n,
        Err(_) => {
            ctx.fail(format!(
                "STEM {stem_base}: {count_var} is not a number: {count_str}"
            ));
            return;
        }
    };

    ctx.select_output(StreamId::PRIMARY);
    for i in 1..=count {
        let var_name = format!("{}{}", stem_base, i);
        let value = match get_rexx_variable(&var_name) {
            Ok(val) => val,
            Err(e) => {
                ctx.fail(format!("STEM {stem_base}: {e}"));
                return;
            }
        };
        if ctx.write(Record::from(value)).is_err() {
            return;
        }
    }
}

fn run_stem_sink(ctx: &mut StageCtx, stem_base: &str) {
    let mut records = Vec::new();
    ctx.select_input(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => records.push(rec.to_string_lossy()),
            Ok(None) => break,
            Err(_) => return,
        }
    }

    let count = records.len();
    let count_var = format!("{}0", stem_base);
    if let Err(e) = set_rexx_variable(&count_var, &count.to_string()) {
        ctx.fail(format!("STEM {stem_base}: {e}"));
        return;
    }

    for (i, record) in records.iter().enumerate() {
        let var_name = format!("{}{}", stem_base, i + 1);
        if let Err(e) = set_rexx_variable(&var_name, record) {
            ctx.fail(format!("STEM {stem_base}: {e}"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_stem_adds_period() {
        assert_eq!(normalize_stem_name("ABC"), "ABC.");
        assert_eq!(normalize_stem_name("ABC."), "ABC.");
        assert_eq!(normalize_stem_name("X."), "X.");
        assert_eq!(normalize_stem_name(""), "");
    }
}
