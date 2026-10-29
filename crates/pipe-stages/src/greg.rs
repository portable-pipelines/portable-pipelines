//! GREG2SEC and SEC2GREG — Author's Edition 1.1.11 pp.369 and 485.
//!
//! Portable UTC mapping: `yyyymmddhhmmss` (or `yyyy-mm-dd hh:mm:ss` /
//! `yyyy-mm-ddThh:mm:ss`) to whole seconds since 1970-01-01 00:00:00 UTC,
//! and the converse. Local time zones are not used.

use pipe_core::{Record, StageCtx, StreamId};

use crate::civil::{from_unix_seconds, parse_gregorian, parse_unix_seconds_word, to_unix_seconds};

pub fn greg2sec(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = reject_operands("GREG2SEC", &args) {
            ctx.fail(e);
            return;
        }
        run_convert(ctx, "GREG2SEC", Convert::GregToSec);
    })
}

pub fn sec2greg(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = reject_operands("SEC2GREG", &args) {
            ctx.fail(e);
            return;
        }
        run_convert(ctx, "SEC2GREG", Convert::SecToGreg);
    })
}

fn reject_operands(verb: &str, args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("{verb}: unrecognized operand {s}")),
    }
}

#[derive(Clone, Copy)]
enum Convert {
    GregToSec,
    SecToGreg,
}

fn run_convert(ctx: &mut StageCtx, verb: &str, how: Convert) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail(format!("{verb}: only the primary input may be connected"));
            return;
        }
    }
    if !ctx.input_connected(StreamId::PRIMARY) {
        ctx.fail(format!("{verb} must not be a first stage"));
        return;
    }

    let secondary = ctx.output_connected(StreamId::SECONDARY);
    ctx.select_input(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                let line = rec.to_string_lossy();
                let word = line.trim();
                if word.is_empty() {
                    if !emit_bad(ctx, verb, rec, secondary, "missing value") {
                        return;
                    }
                    continue;
                }
                let converted = match how {
                    Convert::GregToSec => {
                        parse_gregorian(word).map(|dt| to_unix_seconds(dt).to_string())
                    }
                    Convert::SecToGreg => parse_unix_seconds_word(word)
                        .and_then(from_unix_seconds)
                        .and_then(|dt| dt.to_greg_digits()),
                };
                match converted {
                    Ok(out) => {
                        if !ctx.output_connected(StreamId::PRIMARY) {
                            if ctx.input_connected(StreamId::PRIMARY) {
                                ctx.short_input();
                            }
                            return;
                        }
                        ctx.select_output(StreamId::PRIMARY);
                        if ctx.write(Record::from(out)).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        if !emit_bad(ctx, verb, rec, secondary, &e) {
                            return;
                        }
                    }
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

fn emit_bad(ctx: &mut StageCtx, verb: &str, rec: Record, secondary: bool, err: &str) -> bool {
    if secondary {
        ctx.select_output(StreamId::SECONDARY);
        ctx.write(rec).is_ok()
    } else {
        ctx.fail(format!("{verb}: {err}"));
        false
    }
}
