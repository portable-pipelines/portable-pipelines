//! TIMESTAMP — Author's Edition 1.1.11 p.557.
//!
//! Prefix each record with a UTC date-and-time stamp `YYYY-MM-DD HH:MM:SS`
//! and a separator (default one blank). The clock is UTC Unix time, not the
//! host local zone. Tests inject the clock through [`with_unix_now`].

use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

use pipe_core::{Record, StageCtx, StreamId};

use crate::civil::from_unix_seconds;
use crate::common::decode_dstring_text;

thread_local! {
    static UNIX_NOW: Cell<Option<i64>> = const { Cell::new(None) };
}

/// Override Unix seconds for tests. `None` restores the host clock.
pub fn set_test_unix_now(secs: Option<i64>) {
    UNIX_NOW.with(|c| c.set(secs));
}

/// Run `f` with a fixed Unix time, then restore the host clock.
pub fn with_unix_now<R>(secs: i64, f: impl FnOnce() -> R) -> R {
    set_test_unix_now(Some(secs));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    set_test_unix_now(None);
    match result {
        Ok(r) => r,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub fn timestamp(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_timestamp(&args) {
        Ok(sep) => run_timestamp(ctx, &sep),
        Err(e) => ctx.fail(format!("TIMESTAMP: {e}")),
    })
}

fn parse_timestamp(args: &[String]) -> Result<String, String> {
    match args {
        [] => Ok(" ".to_string()),
        [s] => decode_dstring_text(s),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_timestamp(ctx: &mut StageCtx, sep: &str) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("TIMESTAMP: only the primary input may be connected");
            return;
        }
    }
    if !ctx.input_connected(StreamId::PRIMARY) {
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return;
        }
        match ctx.read() {
            Ok(Some(rec)) => {
                let stamp = match format_stamp(unix_now()) {
                    Ok(s) => s,
                    Err(e) => {
                        ctx.fail(format!("TIMESTAMP: {e}"));
                        return;
                    }
                };
                let mut out = stamp.into_bytes();
                out.extend_from_slice(sep.as_bytes());
                out.extend_from_slice(rec.as_bytes());
                if ctx.write(Record::from(out)).is_err() {
                    return;
                }
            }
            Ok(None) => return,
            Err(_) => return,
        }
    }
}

fn unix_now() -> i64 {
    if let Some(secs) = UNIX_NOW.with(|c| c.get()) {
        return secs;
    }
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

pub fn format_stamp(secs: i64) -> Result<String, String> {
    let dt = from_unix_seconds(secs)?;
    if !(0..=9999).contains(&dt.year) {
        return Err(format!("year {} does not fit YYYY-MM-DD", dt.year));
    }
    Ok(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_at_epoch() {
        assert_eq!(format_stamp(0).unwrap(), "1970-01-01 00:00:00");
        assert_eq!(format_stamp(-1).unwrap(), "1969-12-31 23:59:59");
        assert_eq!(format_stamp(1_793_277_000).unwrap(), "2026-10-29 12:30:00");
    }
}
