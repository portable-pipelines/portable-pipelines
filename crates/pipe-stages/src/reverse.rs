//! REVERSE — reverse contents of records.
//!
//! Author's Edition 1.1.11 p.474.
//!
//! Syntax (diagram all-caps; no verb prefix):
//! `REVERSE`
//!
//! No operands. Each record is written with its bytes in reverse order so
//! the first character becomes the last (portable ASCII: one byte is one
//! character, matching EBCDIC in the book).

use pipe_core::{Record, StageCtx, StreamId};

pub fn reverse(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if !args.is_empty() {
            ctx.fail(format!("REVERSE: unexpected operand {}", args[0]));
            return;
        }

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("REVERSE: no input stream other than the primary may be connected");
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
        run_reverse(ctx);
    })
}

fn run_reverse(ctx: &mut StageCtx) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = reverse_bytes(rec.as_bytes());
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

fn reverse_bytes(record: &[u8]) -> Vec<u8> {
    let mut out = record.to_vec();
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverses_ascii_bytes() {
        assert_eq!(reverse_bytes(b"Hello, World!"), b"!dlroW ,olleH");
        assert_eq!(reverse_bytes(b""), b"");
        assert_eq!(reverse_bytes(b"a"), b"a");
        assert_eq!(reverse_bytes(&[0xff, 0x00, 0x01]), &[0x01, 0x00, 0xff]);
    }
}
