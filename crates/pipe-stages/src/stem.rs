//! STEM - REXX stem variables (arrays).
//!
//! Author's Edition 1.1.11 p.528.
//!
//! Syntax:
//! `STEM [INPUT] name`
//! `STEM OUTPUT name`
//!
//! When first (or when INPUT is specified), reads records from the pipeline
//! and stores them in a REXX stem variable. Each record becomes STEM.i where
//! i starts at 1, and STEM.0 is set to the count.
//!
//! When OUTPUT is specified (must not be first), reads the stem and writes
//! records. STEM.0 should contain the count, and STEM.1, STEM.2, etc. are written.
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

        let mut iter = args.iter();
        let first = iter.next().unwrap();
        let is_output = first.eq_ignore_ascii_case("OUTPUT");
        let is_input = first.eq_ignore_ascii_case("INPUT");

        let stem_name = if is_output || is_input {
            match iter.next() {
                Some(name) => name,
                None => {
                    ctx.fail("STEM: stem name required after INPUT/OUTPUT");
                    return;
                }
            }
        } else {
            first
        };

        if iter.next().is_some() {
            ctx.fail("STEM: too many operands");
            return;
        }

        let stem_base = normalize_stem_name(stem_name);

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("STEM: no input stream other than the primary may be connected");
                return;
            }
        }

        if is_output {
            run_stem_output(ctx, &stem_base);
        } else {
            run_stem_input(ctx, &stem_base);
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

fn run_stem_input(ctx: &mut StageCtx, stem_base: &str) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.short_input();
        return;
    }

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
        ctx.fail(format!("STEM INPUT {stem_base}: {e}"));
        return;
    }

    for (i, record) in records.iter().enumerate() {
        let var_name = format!("{}{}", stem_base, i + 1);
        if let Err(e) = set_rexx_variable(&var_name, record) {
            ctx.fail(format!("STEM INPUT {stem_base}: {e}"));
            return;
        }
    }
}

fn run_stem_output(ctx: &mut StageCtx, stem_base: &str) {
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.short_input();
        return;
    }

    let count_var = format!("{}0", stem_base);
    let count_str = match get_rexx_variable(&count_var) {
        Ok(val) => val,
        Err(e) => {
            ctx.fail(format!("STEM OUTPUT {stem_base}: {e}"));
            return;
        }
    };

    let count: usize = match count_str.trim().parse() {
        Ok(n) => n,
        Err(_) => {
            ctx.fail(format!(
                "STEM OUTPUT {stem_base}: {count_var} is not a number: {count_str}"
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
                ctx.fail(format!("STEM OUTPUT {stem_base}: {e}"));
                return;
            }
        };
        if ctx.write(Record::from(value)).is_err() {
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
    fn stem_requires_name() {
        use pipe_core::PipelineBuilder;
        let mut b = PipelineBuilder::new();
        let id = b.add_stage("STEM", stem(vec![]));
        let result = b.run();
        assert!(result.is_err());
    }

    #[test]
    fn normalize_stem_adds_period() {
        assert_eq!(normalize_stem_name("ABC"), "ABC.");
        assert_eq!(normalize_stem_name("ABC."), "ABC.");
        assert_eq!(normalize_stem_name("X.."), "X..");
        assert_eq!(normalize_stem_name(""), "");
    }
}
