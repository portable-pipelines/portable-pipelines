//! DUPLICATE — Author's Edition 1.1.11, p.334 (tutorial pp.148–149).
//!
//! Syntax: `DUPlicate [snumber | *]`
//!
//! The number is additional copies (default 1 = write each record twice).
//! It must be -1 or larger. `*` copies the first input record until the output
//! severs, at most 2_147_483_647 times (p.149).

use pipe_core::{Record, StageCtx, StreamId};

use crate::xrange::{is_snumber_token, parse_snumber};

/// `duplicate *` stops after this many copies of the first record (p.149).
const STAR_LIMIT: u64 = i32::MAX as u64;

pub fn duplicate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_duplicate(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("DUPLICATE: {e}"));
                return;
            }
        };

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);

        match spec {
            Copies::Star => run_star(ctx),
            Copies::Additional(n) => run_additional(ctx, n),
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Copies {
    /// Write each record `n + 1` times (`n = -1` writes nothing).
    Additional(i32),
    /// Infinite copies of the first input record (capped at [`STAR_LIMIT`]).
    Star,
}

fn parse_duplicate(args: &[String]) -> Result<Copies, String> {
    if args.is_empty() {
        return Ok(Copies::Additional(1));
    }
    if args.len() > 1 {
        return Err(format!("unexpected operand {}", args[1]));
    }
    let a = &args[0];
    if a == "*" {
        return Ok(Copies::Star);
    }
    if !is_snumber_token(a) {
        return Err(format!("unexpected operand {a}"));
    }
    let n = parse_snumber(a)?;
    if n < -1 {
        return Err(format!("number must be -1 or larger, got {a}"));
    }
    Ok(Copies::Additional(n))
}

fn run_additional(ctx: &mut StageCtx, n: i32) {
    let times = (n as i64 + 1) as u64;
    while ctx.output_connected(StreamId::PRIMARY) {
        let rec = match ctx.read() {
            Ok(Some(r)) => r,
            Ok(None) | Err(_) => return,
        };
        if !write_copies(ctx, rec, times) {
            return;
        }
    }
}

fn run_star(ctx: &mut StageCtx) {
    let rec = match ctx.read() {
        Ok(Some(r)) => r,
        Ok(None) | Err(_) => return,
    };
    let _ = write_copies(ctx, rec, STAR_LIMIT);
    // Further input is not copied (p.334: first record only). Sever so a
    // producer with more records does not stall on the one-record slot.
    ctx.select_input(StreamId::PRIMARY);
    ctx.short_input();
}

/// Write `times` copies. The input record is held until every copy is written (p.334).
fn write_copies(ctx: &mut StageCtx, rec: Record, times: u64) -> bool {
    for _ in 0..times {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return false;
        }
        if ctx.write(rec.clone()).is_err() {
            return false;
        }
    }
    true
}
