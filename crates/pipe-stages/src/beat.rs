//! BEAT — Author's Edition 1.1.11 pp.271–272.
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `BEAT [ONCE] number[.number] [delimitedString]`
//!
//! Device driver. Copies primary input to primary output. If no record
//! arrives within the interval, writes the marker (default: a null record)
//! to the secondary output. Without ONCE, another timeout starts after that
//! marker is consumed; with ONCE, at most one marker is written per input
//! record. Peek, write, then consume so the copied record is not delayed.

use std::time::{Duration, Instant};

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_delimited};

pub fn beat(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_beat(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("BEAT: {e}"));
                return;
            }
        };
        run_beat(ctx, spec);
    })
}

struct BeatSpec {
    once: bool,
    interval: Duration,
    marker: Record,
}

fn parse_beat(args: &[String]) -> Result<BeatSpec, String> {
    let mut i = 0;
    let once = if i < args.len() && args[i].eq_ignore_ascii_case("ONCE") {
        i += 1;
        true
    } else {
        false
    };
    if i >= args.len() {
        return Err("an interval in seconds is required".into());
    }
    let interval = parse_interval(&args[i])?;
    i += 1;
    let marker = if i < args.len() {
        let token = &args[i];
        i += 1;
        if !is_delimited(token) {
            return Err(format!("unrecognized operand {token}"));
        }
        Record::new(decode_dstring(token)?)
    } else {
        Record::empty()
    };
    if i < args.len() {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    Ok(BeatSpec {
        once,
        interval,
        marker,
    })
}

/// Interval operand: `number` or `number.number` seconds (p.271). Up to six
/// fractional digits. No leading plus (that is DELAY), no colon parts.
fn parse_interval(word: &str) -> Result<Duration, String> {
    let err = || format!("invalid interval {word}");
    if word.is_empty() || word.starts_with('+') || word.starts_with('-') || word.contains(':') {
        return Err(err());
    }
    let (whole, frac) = match word.split_once('.') {
        None => {
            if !word.bytes().all(|b| b.is_ascii_digit()) {
                return Err(err());
            }
            let secs: u64 = word.parse().map_err(|_| err())?;
            return Ok(Duration::from_secs(secs));
        }
        Some((w, f)) => (w, f),
    };
    if whole.is_empty()
        || frac.is_empty()
        || frac.len() > 6
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(err());
    }
    let secs: u64 = whole.parse().map_err(|_| err())?;
    let mut padded = frac.to_string();
    while padded.len() < 6 {
        padded.push('0');
    }
    let micros: u32 = padded.parse().map_err(|_| err())?;
    Duration::from_secs(secs)
        .checked_add(Duration::from_micros(micros as u64))
        .ok_or_else(err)
}

fn run_beat(ctx: &mut StageCtx, spec: BeatSpec) {
    if ctx.input_connected(StreamId::SECONDARY) || ctx.connected_inputs().iter().any(|s| s.0 > 1) {
        ctx.fail("BEAT: no other input stream may be connected");
        return;
    }
    if ctx.connected_outputs().iter().any(|s| s.0 > 1) {
        ctx.fail("BEAT: only the primary and secondary output streams may be connected");
        return;
    }

    // ONCE: after a marker, wait for the next input without further markers.
    let mut once_waiting = false;

    loop {
        // Premature termination (p.271): primary output not connected.
        // Secondary may be defined but unconnected until a marker write.
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_primary(ctx);
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            if !copy_one(ctx) {
                return;
            }
            once_waiting = false;
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }

        if once_waiting {
            if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
                short_primary(ctx);
                return;
            }
            continue;
        }

        let deadline = Instant::now() + spec.interval;
        match ctx.wait_until_or_input(deadline, StreamId::PRIMARY) {
            Ok(true) => continue,
            Ok(false) => {
                if ctx.input_record_ready(StreamId::PRIMARY) || ctx.input_at_eof(StreamId::PRIMARY)
                {
                    continue;
                }
                if !write_marker(ctx, &spec.marker) {
                    return;
                }
                if spec.once {
                    once_waiting = true;
                } else {
                    ctx.select_output(StreamId::SECONDARY);
                    if ctx.wait_output().is_err() {
                        short_primary(ctx);
                        return;
                    }
                }
            }
            Err(_) => {
                short_primary(ctx);
                return;
            }
        }
    }
}

/// Strictly does not delay (p.271): peek, write, then consume.
fn copy_one(ctx: &mut StageCtx) -> bool {
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    let Some(rec) = ctx.peek() else {
        return true;
    };
    if !ctx.output_connected(StreamId::PRIMARY) {
        short_primary(ctx);
        return false;
    }
    if ctx.write(rec).is_err() {
        short_primary(ctx);
        return false;
    }
    let _ = ctx.read();
    true
}

fn write_marker(ctx: &mut StageCtx, marker: &Record) -> bool {
    // p.272 example: defined but unconnected secondary terminates here.
    if !ctx.output_connected(StreamId::SECONDARY) {
        short_primary(ctx);
        return false;
    }
    ctx.select_output(StreamId::SECONDARY);
    if ctx.write(marker.clone()).is_err() {
        short_primary(ctx);
        return false;
    }
    true
}

fn short_primary(ctx: &mut StageCtx) {
    if ctx.input_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_whole_and_fraction() {
        assert_eq!(parse_interval("0").unwrap(), Duration::ZERO);
        assert_eq!(parse_interval("1").unwrap(), Duration::from_secs(1));
        assert_eq!(parse_interval("60").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_interval("0.02").unwrap(), Duration::from_millis(20));
        assert_eq!(parse_interval("1.5").unwrap(), Duration::from_millis(1500));
        assert_eq!(
            parse_interval("0.000001").unwrap(),
            Duration::from_micros(1)
        );
        assert_eq!(
            parse_interval("0.000010").unwrap(),
            Duration::from_micros(10)
        );
    }

    #[test]
    fn interval_rejects_undocumented_forms() {
        for w in [
            "",
            "+",
            "+1",
            "-1",
            "1:30",
            ".5",
            "1.",
            "1.0000000",
            "1.2.3",
            "abc",
            "1e2",
            "ONCE",
        ] {
            assert!(parse_interval(w).is_err(), "{w}");
        }
    }

    #[test]
    fn parse_once_number_and_dstring() {
        let s = parse_beat(&["ONCE".into(), "0.02".into(), "/tick/".into()]).unwrap();
        assert!(s.once);
        assert_eq!(s.interval, Duration::from_millis(20));
        assert_eq!(s.marker.as_bytes(), b"tick");
    }

    #[test]
    fn parse_default_marker_is_null() {
        let s = parse_beat(&["1".into()]).unwrap();
        assert!(!s.once);
        assert!(s.marker.is_empty());
    }

    #[test]
    fn parse_rejects_wrong_order_and_invented() {
        assert!(parse_beat(&[]).is_err());
        assert!(parse_beat(&["ONCE".into()]).is_err());
        assert!(parse_beat(&["1".into(), "ONCE".into()]).is_err());
        assert!(parse_beat(&["1".into(), "tick".into()]).is_err());
        assert!(parse_beat(&["FOREVER".into(), "1".into()]).is_err());
        assert!(parse_beat(&["1".into(), "/a/".into(), "/b/".into()]).is_err());
        assert!(parse_beat(&["ONC".into(), "1".into()]).is_err());
    }

    #[test]
    fn parse_hex_marker() {
        let s = parse_beat(&["1".into(), "x4142".into()]).unwrap();
        assert_eq!(s.marker.as_bytes(), b"AB");
    }
}
