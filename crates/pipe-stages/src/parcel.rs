//! PARCEL — parcel a byte stream into records.
//!
//! Author's Edition 1.1.11 p.452.
//!
//! Syntax (diagram all-caps; no operands):
//! `PARCEL`
//!
//! Primary input is a byte stream. Secondary input supplies unsigned decimal
//! lengths (blank-delimited; several may share a record). Each length becomes
//! one output record.

use pipe_core::{Record, StageCtx, StreamId};

pub fn parcel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_parcel(&args) {
            ctx.fail(format!("PARCEL: {e}"));
            return;
        }
        run_parcel(ctx);
    })
}

fn parse_parcel(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn parse_lengths(rec: &[u8]) -> Result<Vec<usize>, String> {
    let text = String::from_utf8_lossy(rec);
    let mut out = Vec::new();
    for word in text.split_ascii_whitespace() {
        if !word.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!(
                "number expected on the secondary input, found {word}"
            ));
        }
        let n: usize = word
            .parse()
            .map_err(|_| format!("number {word} is not a valid parcel length"))?;
        out.push(n);
    }
    Ok(out)
}

fn run_parcel(ctx: &mut StageCtx) {
    if ctx.connected_inputs().iter().any(|s| s.0 > 1) {
        ctx.fail("PARCEL: only the primary and secondary input streams may be connected");
        return;
    }
    if !ctx.input_connected(StreamId::SECONDARY) {
        ctx.fail("PARCEL: the secondary input stream must be defined and connected");
        return;
    }
    if ctx.output_connected(StreamId::SECONDARY) || ctx.connected_outputs().iter().any(|s| s.0 > 1)
    {
        ctx.fail("PARCEL: the secondary output stream must not be connected");
        return;
    }

    if !ctx.output_connected(StreamId::PRIMARY) {
        short_inputs(ctx);
        return;
    }

    ctx.select_output(StreamId::PRIMARY);

    let mut pending = Vec::new();
    let mut lengths: Vec<usize> = Vec::new();
    let mut length_at = 0usize;
    let mut hold_secondary = false;
    let mut primary_held = false;

    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }

        if length_at >= lengths.len() {
            if hold_secondary {
                ctx.select_input(StreamId::SECONDARY);
                let _ = ctx.read();
                hold_secondary = false;
            }
            if ctx.input_record_ready(StreamId::SECONDARY) {
                ctx.select_input(StreamId::SECONDARY);
                let Some(rec) = ctx.peek() else {
                    continue;
                };
                match parse_lengths(rec.as_bytes()) {
                    Ok(ns) => {
                        if ns.is_empty() {
                            let _ = ctx.read();
                            continue;
                        }
                        lengths = ns;
                        length_at = 0;
                        hold_secondary = true;
                    }
                    Err(e) => {
                        ctx.fail(format!("PARCEL: {e}"));
                        return;
                    }
                }
                continue;
            }
            if ctx.input_at_eof(StreamId::SECONDARY) {
                // Secondary EOF: stop. Leftover primary bytes stay unconsumed.
                if primary_held {
                    // Last primary record is not consumed (p.452).
                }
                return;
            }
            if ctx.wait_any_input(&[StreamId::SECONDARY]).is_err() {
                short_inputs(ctx);
                return;
            }
            continue;
        }

        let need = lengths[length_at];
        if pending.len() >= need {
            let rec: Vec<u8> = pending.drain(..need).collect();
            if ctx.write(Record::from(rec)).is_err() {
                return;
            }
            length_at += 1;
            let last_of_secondary = length_at >= lengths.len();
            let primary_emptied = pending.is_empty() && primary_held;
            // p.452: when both records empty together, consume primary first.
            if primary_emptied {
                ctx.select_input(StreamId::PRIMARY);
                let _ = ctx.read();
                primary_held = false;
            }
            if last_of_secondary && hold_secondary {
                ctx.select_input(StreamId::SECONDARY);
                let _ = ctx.read();
                hold_secondary = false;
                lengths.clear();
                length_at = 0;
            }
            continue;
        }

        if primary_held {
            // Current primary record is already in `pending`. Consume it only
            // when it cannot finish this parcel, so the next input block can
            // join the byte stream.
            if pending.len() < need {
                ctx.select_input(StreamId::PRIMARY);
                let _ = ctx.read();
                primary_held = false;
            }
            continue;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if rec.is_empty() {
                let _ = ctx.read();
                continue;
            }
            pending.extend_from_slice(rec.as_bytes());
            primary_held = true;
            continue;
        }

        if ctx.input_at_eof(StreamId::PRIMARY) {
            // 72E last record not complete. Do not consume the secondary record.
            ctx.fail("PARCEL: 72E last record not complete");
            return;
        }

        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            short_inputs(ctx);
            return;
        }
    }
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}
