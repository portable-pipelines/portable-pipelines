//! JUXTAPOSE — Author's Edition 1.1.11 pp.403–405 (tutorial pp.83–84;
//! collate example p.297; fullscr example p.405).
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `JUXTAPOSE [COUNT]`
//!
//! Gateway. Each primary input record is stored in a one-record buffer and
//! consumed. Each secondary input record is appended to that buffer and the
//! combined record is written to the primary output. An unused primary record
//! (no secondary arrived while it resided in the buffer) is written to the
//! secondary output before the next primary is stored.

use pipe_core::{Record, StageCtx, StreamId};

pub fn juxtapose(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let count = match parse_juxtapose(&args) {
            Ok(c) => c,
            Err(e) => {
                ctx.fail(format!("JUXTAPOSE: {e}"));
                return;
            }
        };
        run_juxtapose(ctx, count);
    })
}

fn parse_juxtapose(args: &[String]) -> Result<bool, String> {
    match args {
        [] => Ok(false),
        [s] if s.eq_ignore_ascii_case("COUNT") => Ok(true),
        [s] => Err(format!("unrecognized operand {s}")),
        [s, rest, ..] if s.eq_ignore_ascii_case("COUNT") => {
            Err(format!("unrecognized operand {rest}"))
        }
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_juxtapose(ctx: &mut StageCtx, count: bool) {
    if ctx.connected_inputs().iter().any(|s| s.0 > 1)
        || ctx.connected_outputs().iter().any(|s| s.0 > 1)
    {
        ctx.fail("JUXTAPOSE: only the primary and secondary streams may be connected");
        return;
    }

    // p.404: two streams must be defined.
    if !ctx.input_connected(StreamId::SECONDARY) && !ctx.output_connected(StreamId::SECONDARY) {
        ctx.fail("JUXTAPOSE: two streams must be defined");
        return;
    }

    if ctx.connected_outputs().is_empty() {
        short_inputs(ctx);
        return;
    }

    let mut buffer: Option<Record> = None;
    let mut used = false;
    let mut appended: u64 = 0;

    loop {
        if ctx.connected_outputs().is_empty() {
            short_inputs(ctx);
            return;
        }

        match ctx.read_any(&[StreamId::PRIMARY, StreamId::SECONDARY]) {
            Ok(Some((stream, rec))) if stream == StreamId::PRIMARY => {
                if !store_primary(ctx, &mut buffer, &mut used, &mut appended, rec) {
                    short_inputs(ctx);
                    return;
                }
            }
            Ok(Some((stream, rec))) if stream == StreamId::SECONDARY => {
                if !append_secondary(ctx, &buffer, &mut used, &mut appended, count, rec) {
                    short_inputs(ctx);
                    return;
                }
            }
            Ok(Some(_)) => unreachable!("juxtapose reads only streams 0 and 1"),
            Ok(None) => {
                let _ = flush_unused(ctx, buffer.take(), used);
                short_inputs(ctx);
                return;
            }
            Err(_) => {
                short_inputs(ctx);
                return;
            }
        }
    }
}

/// Replace the one-record buffer. An unused previous primary goes to the
/// secondary output first (p.404).
fn store_primary(
    ctx: &mut StageCtx,
    buffer: &mut Option<Record>,
    used: &mut bool,
    appended: &mut u64,
    rec: Record,
) -> bool {
    if !flush_unused(ctx, buffer.take(), *used) {
        return false;
    }
    *buffer = Some(rec);
    *used = false;
    *appended = 0;
    true
}

/// Append a secondary record to the current buffer and write the combined
/// record to the primary output. COUNT starts at one (p.404).
fn append_secondary(
    ctx: &mut StageCtx,
    buffer: &Option<Record>,
    used: &mut bool,
    appended: &mut u64,
    count: bool,
    rec: Record,
) -> bool {
    *appended += 1;
    if buffer.is_some() {
        *used = true;
    }
    let prefix = buffer.as_ref().map(Record::as_bytes).unwrap_or(b"");
    emit(
        ctx,
        StreamId::PRIMARY,
        combine(prefix, rec.as_bytes(), count, *appended),
    )
}

fn flush_unused(ctx: &mut StageCtx, buffer: Option<Record>, used: bool) -> bool {
    match buffer {
        Some(rec) if !used => emit(ctx, StreamId::SECONDARY, rec),
        _ => true,
    }
}

fn combine(prefix: &[u8], suffix: &[u8], count: bool, n: u64) -> Record {
    let mut out = Vec::new();
    if count {
        out.extend_from_slice(&count_prefix(n));
    }
    out.extend_from_slice(prefix);
    out.extend_from_slice(suffix);
    Record::from(out)
}

fn count_prefix(n: u64) -> Vec<u8> {
    format!("{n:>10}").into_bytes()
}

fn emit(ctx: &mut StageCtx, stream: StreamId, rec: Record) -> bool {
    if ctx.output_connected(stream) {
        ctx.select_output(stream);
        if ctx.write(rec).is_err() {
            return any_output_live(ctx);
        }
    }
    any_output_live(ctx)
}

fn any_output_live(ctx: &StageCtx) -> bool {
    !ctx.connected_outputs().is_empty()
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}
