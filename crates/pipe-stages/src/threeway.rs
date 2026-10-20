//! THREEWAY — Author's Edition 1.1.11 p.557.
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `THREEWAY inputRange`
//!
//! Gateway. Split each record before and after the specified input range:
//! prefix → primary, range contents → secondary, remainder → tertiary.
//! Then consume the input record. Synonym: `3WAY` (Note 1).

use pipe_core::{Record, StageCtx, StreamId};

use crate::input_range::{
    locate_input_range, looks_like_input_range_start, parse_input_range, InputRange,
};

pub fn threeway(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let range = match parse_threeway(&args) {
            Ok(r) => r,
            Err(e) => {
                ctx.fail(format!("THREEWAY: {e}"));
                return;
            }
        };
        run_threeway(ctx, &range);
    })
}

fn parse_threeway(args: &[String]) -> Result<InputRange, String> {
    let mut i = 0;
    if i >= args.len() {
        return Err("inputRange is required".into());
    }
    if !looks_like_input_range_start(&args[i]) {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let range = parse_input_range(args, &mut i)?;
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }
    Ok(range)
}

fn stream_defined(ctx: &StageCtx, stream: StreamId) -> bool {
    ctx.input_connected(stream) || ctx.output_connected(stream)
}

fn run_threeway(ctx: &mut StageCtx, range: &InputRange) {
    if ctx.connected_inputs().iter().any(|s| s.0 > 0) {
        ctx.fail("THREEWAY: no input stream other than the primary may be connected");
        return;
    }
    if ctx.connected_outputs().iter().any(|s| s.0 > 2)
        || ctx.connected_inputs().iter().any(|s| s.0 > 2)
    {
        ctx.fail("THREEWAY: only the primary, secondary, and tertiary streams may be defined");
        return;
    }
    // p.557: three streams must be defined.
    if !stream_defined(ctx, StreamId::SECONDARY) || !stream_defined(ctx, StreamId::TERTIARY) {
        ctx.fail("THREEWAY: three streams must be defined");
        return;
    }

    if !any_output(ctx) {
        if ctx.input_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
        }
        return;
    }

    ctx.select_input(StreamId::PRIMARY);

    loop {
        if !any_output(ctx) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
            }
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let (pre, mid, rest) = split_three(rec.as_bytes(), range);
            write_part(ctx, StreamId::PRIMARY, pre);
            write_part(ctx, StreamId::SECONDARY, mid);
            write_part(ctx, StreamId::TERTIARY, rest);
            if !any_output(ctx) {
                if ctx.input_connected(StreamId::PRIMARY) {
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.short_input();
                }
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

fn any_output(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY)
        || ctx.output_connected(StreamId::SECONDARY)
        || ctx.output_connected(StreamId::TERTIARY)
}

fn write_part(ctx: &mut StageCtx, stream: StreamId, bytes: Vec<u8>) {
    if !ctx.output_connected(stream) {
        return;
    }
    ctx.select_output(stream);
    let _ = ctx.write(Record::from(bytes));
}

/// Split `record` before and after `range` (p.557). A missing range (starting
/// column/word/field not in the record) writes the whole record to primary
/// and nulls to the other two streams.
fn split_three(record: &[u8], range: &InputRange) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let loc = locate_input_range(record, range);
    if !loc.present {
        return (record.to_vec(), Vec::new(), Vec::new());
    }
    let start = loc.start.min(record.len());
    let end = loc.end.min(record.len()).max(start);
    (
        record[..start].to_vec(),
        record[start..end].to_vec(),
        record[end..].to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(args: &[&str]) -> InputRange {
        parse_threeway(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn columns_split_before_and_after() {
        let (pre, mid, rest) = split_three(b"abcdef", &range(&["2-4"]));
        assert_eq!(pre, b"a");
        assert_eq!(mid, b"bcd");
        assert_eq!(rest, b"ef");
    }

    #[test]
    fn missing_range_is_whole_record_on_primary() {
        let (pre, mid, rest) = split_three(b"ab", &range(&["10"]));
        assert_eq!(pre, b"ab");
        assert_eq!(mid, b"");
        assert_eq!(rest, b"");
    }

    #[test]
    fn column_one_writes_null_prefix() {
        let (pre, mid, rest) = split_three(b"abc", &range(&["1"]));
        assert_eq!(pre, b"");
        assert_eq!(mid, b"a");
        assert_eq!(rest, b"bc");
    }
}
