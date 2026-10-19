//! ELASTIC — Author's Edition 1.1.11 p.335 (tutorial Ch.5 pp.88–89).
//!
//! Syntax (diagram is all-caps; no operands):
//! `ELASTIC`
//!
//! Gateway. Reads into an internal buffer and writes from it so a write
//! wait does not stop a further read. Primary-only mode keeps the buffer
//! as small as it can. With a secondary input (assumed feedback from the
//! primary output), primary records are not delayed; secondary records
//! form a to-do list that is written only after primary EOF.

use std::collections::VecDeque;

use pipe_core::{Record, StageCtx, StreamId};

pub fn elastic(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Some(op) = args.first() {
            ctx.fail(format!("ELASTIC: unrecognized operand {op}"));
            return;
        }
        if ctx
            .connected_outputs()
            .iter()
            .any(|s| *s != StreamId::PRIMARY)
        {
            ctx.fail("ELASTIC: the secondary output stream must not be connected");
            return;
        }
        if ctx.connected_inputs().iter().any(|s| s.0 > 1) {
            ctx.fail("ELASTIC: only the primary and secondary input streams may be connected");
            return;
        }
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_connected(StreamId::SECONDARY) {
            run_feedback(ctx);
        } else {
            run_primary_only(ctx);
        }
    })
}

fn run_primary_only(ctx: &mut StageCtx) {
    let mut buf: VecDeque<Record> = VecDeque::new();
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }

        // Prefer a write when the consumer can take a record so the buffer
        // stays as small as the topology allows (p.335).
        if !buf.is_empty() && ctx.output_can_accept(StreamId::PRIMARY) {
            if !write_one(ctx, buf.pop_front().unwrap()) {
                return;
            }
            continue;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            match take(ctx, StreamId::PRIMARY) {
                Some(r) => buf.push_back(r),
                None => {
                    flush(ctx, &mut buf);
                    return;
                }
            }
            continue;
        }

        if ctx.input_at_eof(StreamId::PRIMARY) {
            flush(ctx, &mut buf);
            return;
        }

        if buf.is_empty() {
            match take(ctx, StreamId::PRIMARY) {
                Some(r) => buf.push_back(r),
                None => {
                    flush(ctx, &mut buf);
                    return;
                }
            }
        } else {
            ctx.select_input(StreamId::PRIMARY);
            ctx.select_output(StreamId::PRIMARY);
            if ctx.wait_read_or_write().is_err() {
                return;
            }
        }
    }
}

fn run_feedback(ctx: &mut StageCtx) {
    let mut buf: VecDeque<Record> = VecDeque::new();

    // Phase 1: primary through to the output; secondary into the to-do list.
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        slurp_secondary(ctx, &mut buf);
        if ctx.input_at_eof(StreamId::PRIMARY) {
            break;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            match take(ctx, StreamId::PRIMARY) {
                Some(r) => {
                    if !write_one(ctx, r) {
                        return;
                    }
                }
                None => break,
            }
            continue;
        }
        if ctx.input_at_eof(StreamId::SECONDARY) {
            match take(ctx, StreamId::PRIMARY) {
                Some(r) => {
                    if !write_one(ctx, r) {
                        return;
                    }
                }
                None => break,
            }
            continue;
        }
        if ctx
            .wait_any_input(&[StreamId::PRIMARY, StreamId::SECONDARY])
            .is_err()
        {
            return;
        }
    }

    // Phase 2: listening mode. Write the to-do list; accept more secondary.
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        slurp_secondary(ctx, &mut buf);
        if ctx.input_at_eof(StreamId::SECONDARY) {
            flush(ctx, &mut buf);
            return;
        }
        if buf.is_empty() {
            if ctx.yield_others().is_err() {
                return;
            }
            slurp_secondary(ctx, &mut buf);
            if buf.is_empty() {
                // No record after every other ready stage ran — stop rather
                // than read and risk a stall (p.335; Note 2).
                return;
            }
            continue;
        }
        if ctx.input_record_ready(StreamId::SECONDARY) {
            slurp_secondary(ctx, &mut buf);
            continue;
        }
        if ctx.output_can_accept(StreamId::PRIMARY) {
            if !write_one(ctx, buf.pop_front().unwrap()) {
                return;
            }
            continue;
        }
        ctx.select_input(StreamId::SECONDARY);
        ctx.select_output(StreamId::PRIMARY);
        if ctx.wait_read_or_write().is_err() {
            return;
        }
    }
}

fn slurp_secondary(ctx: &mut StageCtx, buf: &mut VecDeque<Record>) {
    while ctx.input_record_ready(StreamId::SECONDARY) {
        match take(ctx, StreamId::SECONDARY) {
            Some(r) => buf.push_back(r),
            None => break,
        }
    }
}

fn take(ctx: &mut StageCtx, stream: StreamId) -> Option<Record> {
    ctx.select_input(stream);
    match ctx.read() {
        Ok(Some(r)) => Some(r),
        _ => None,
    }
}

fn write_one(ctx: &mut StageCtx, rec: Record) -> bool {
    if !ctx.output_connected(StreamId::PRIMARY) {
        return false;
    }
    ctx.select_output(StreamId::PRIMARY);
    ctx.write(rec).is_ok()
}

fn flush(ctx: &mut StageCtx, buf: &mut VecDeque<Record>) {
    while let Some(r) = buf.pop_front() {
        if !write_one(ctx, r) {
            return;
        }
    }
}
