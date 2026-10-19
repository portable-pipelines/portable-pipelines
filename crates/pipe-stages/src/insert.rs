//! INSERT — insert a string in records.
//!
//! Author's Edition 1.1.11 p.392.
//!
//! Syntax (fixed order; diagram all-caps):
//! `INSERT delimitedString [BEFORE | AFTER] [inputRange]`
//!
//! Default placement is BEFORE. Default range is the entire record.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_delimited};
use crate::input_range::{
    locate_input_range, looks_like_input_range_start, parse_input_range, range_start_from_end,
    InputRange,
};

pub fn insert(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_insert(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("INSERT: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("INSERT: no input stream other than the primary may be connected");
                return;
            }
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);
        run_insert(ctx, &spec);
    })
}

struct InsertSpec {
    insert: Vec<u8>,
    after: bool,
    range: Option<InputRange>,
}

fn parse_insert(args: &[String]) -> Result<InsertSpec, String> {
    let mut i = 0;
    if i >= args.len() {
        return Err("delimitedString is required".into());
    }
    if !is_delimited(&args[i]) {
        return Err(first_operand_error(&args[i]));
    }
    let insert = decode_dstring(&args[i])?;
    i += 1;

    // [BEFORE | AFTER] — whole words; diagram default is BEFORE (p.392).
    let mut after = false;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "BEFORE" {
            i += 1;
        } else if upper == "AFTER" {
            after = true;
            i += 1;
        }
    }

    let mut range = None;
    if i < args.len() {
        if looks_like_input_range_start(&args[i]) {
            range = Some(parse_input_range(args, &mut i)?);
        } else {
            return Err(unexpected_operand(&args[i]));
        }
    }

    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    Ok(InsertSpec {
        insert,
        after,
        range,
    })
}

fn first_operand_error(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if upper == "BEFORE" || upper == "AFTER" {
        format!("{upper} must appear after the delimited string")
    } else if looks_like_input_range_start(arg) {
        "inputRange must appear after the delimited string".into()
    } else {
        format!("not a delimited string: {arg}")
    }
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if upper == "BEFORE" || upper == "AFTER" {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn run_insert(ctx: &mut StageCtx, spec: &InsertSpec) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = apply_insert(rec.as_bytes(), spec);
            if ctx.write(Record::from(out)).is_err() {
                return;
            }
            let _ = ctx.read();
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

fn apply_insert(record: &[u8], spec: &InsertSpec) -> Vec<u8> {
    let (start, end) = insertion_span(record, spec);
    let pos = if spec.after { end } else { start };
    let mut out = Vec::with_capacity(record.len() + spec.insert.len());
    out.extend_from_slice(&record[..pos]);
    out.extend_from_slice(&spec.insert);
    out.extend_from_slice(&record[pos..]);
    out
}

/// Half-open span where the string is inserted before `start` or after `end`.
fn insertion_span(record: &[u8], spec: &InsertSpec) -> (usize, usize) {
    let Some(range) = &spec.range else {
        // Omitted range is the entire record (p.392), including a null record.
        return (0, record.len());
    };
    let loc = locate_input_range(record, range);
    if loc.present {
        return (loc.start, loc.end);
    }
    // Missing range: first or last column of the record, according to whether
    // the innermost start is relative to the beginning or the end (p.392).
    if record.is_empty() {
        return (0, 0);
    }
    if range_start_from_end(range) {
        (record.len() - 1, record.len())
    } else {
        (0, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> InsertSpec {
        parse_insert(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn default_before_entire_record() {
        assert_eq!(apply_insert(b"abc", &spec(&["/*/"])), b"*abc");
    }

    #[test]
    fn after_entire_record() {
        assert_eq!(apply_insert(b"abc", &spec(&["/*/", "after"])), b"abc*");
    }

    #[test]
    fn missing_range_from_start_uses_first_column() {
        assert_eq!(apply_insert(b"abc", &spec(&["/X/", "10"])), b"Xabc");
        assert_eq!(
            apply_insert(b"abc", &spec(&["/X/", "after", "10"])),
            b"aXbc"
        );
    }

    #[test]
    fn missing_range_from_end_uses_last_column() {
        assert_eq!(apply_insert(b"abc", &spec(&["/X/", "-10"])), b"abXc");
        assert_eq!(
            apply_insert(b"abc", &spec(&["/X/", "after", "-10"])),
            b"abcX"
        );
    }
}
