//! DELAY — Author's Edition 1.1.11 pp.321–322 (tutorial Ch.13 pp.148–150).
//!
//! Syntax (diagram is all-caps; no verb prefix, no operands):
//! `DELAY`
//!
//! Device driver. Must not be a first stage. The first blank-delimited word
//! of each input record is a time-of-day or a `+` interval; the rest of the
//! record is not inspected. The whole record is copied to the primary output
//! when that time is reached. `peek`, wait, `write`, then `read` so the
//! record is not delayed in the pipeline sense.

use std::cell::Cell;
use std::time::{Duration, Instant};
#[cfg(not(windows))]
use std::time::{SystemTime, UNIX_EPOCH};

use pipe_core::{StageCtx, StreamId};

thread_local! {
    static CIVIL_NOW: Cell<Option<Duration>> = Cell::new(None);
}

/// Override local time-since-midnight for tests. `None` restores the host clock.
pub fn set_test_civil_now_since_midnight(since_midnight: Option<Duration>) {
    CIVIL_NOW.with(|c| c.set(since_midnight));
}

/// Run `f` with a fixed civil time-since-midnight, then restore the host clock.
pub fn with_civil_now_since_midnight<R>(since_midnight: Duration, f: impl FnOnce() -> R) -> R {
    set_test_civil_now_since_midnight(Some(since_midnight));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    set_test_civil_now_since_midnight(None);
    match result {
        Ok(r) => r,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub fn delay(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        if let Err(e) = parse_delay(&args) {
            ctx.fail(format!("DELAY: {e}"));
            return;
        }
        run_delay(ctx);
    })
}

fn parse_delay(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [s, ..] => Err(format!("unrecognized operand {s}")),
    }
}

fn run_delay(ctx: &mut StageCtx) {
    if !ctx.input_connected(StreamId::PRIMARY) {
        ctx.fail("DELAY must not be a first stage");
        return;
    }
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("DELAY: only the primary input may be connected");
            return;
        }
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

        if !ctx.input_record_ready(StreamId::PRIMARY) {
            if ctx.input_at_eof(StreamId::PRIMARY) {
                return;
            }
            if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
                return;
            }
            continue;
        }

        let Some(rec) = ctx.peek() else {
            return;
        };
        let wait = match wait_for_record(rec.as_bytes()) {
            Ok(d) => d,
            Err(e) => {
                ctx.fail(format!("DELAY: {e}"));
                return;
            }
        };
        if !wait.is_zero() && ctx.wait_until(Instant::now() + wait).is_err() {
            return;
        }
        if !ctx.output_connected(StreamId::PRIMARY) {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return;
        }
        if ctx.write(rec).is_err() {
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return;
        }
        let _ = ctx.read();
    }
}

fn wait_for_record(bytes: &[u8]) -> Result<Duration, String> {
    let word = first_word(bytes).ok_or_else(|| "missing time word".to_string())?;
    let text = std::str::from_utf8(word).map_err(|_| {
        format!(
            "invalid time word {}",
            String::from_utf8_lossy(word)
        )
    })?;
    let when = parse_time_word(text)?;
    Ok(wait_duration(when, civil_now_since_midnight()))
}

fn first_word(bytes: &[u8]) -> Option<&[u8]> {
    let start = bytes.iter().position(|&b| b != b' ')?;
    let rest = &bytes[start..];
    let end = rest.iter().position(|&b| b == b' ').unwrap_or(rest.len());
    if end == 0 {
        None
    } else {
        Some(&rest[..end])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    /// Relative to the instant the record is read.
    Interval(Duration),
    /// Local time since the previous midnight.
    TimeOfDay(Duration),
}

fn parse_time_word(word: &str) -> Result<When, String> {
    if word.is_empty() {
        return Err("missing time word".into());
    }
    let interval = word.starts_with('+');
    let body = if interval { &word[1..] } else { word };
    if body.is_empty() {
        return Err(format!("invalid time word {word}"));
    }
    let parts: Vec<&str> = body.split(':').collect();
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return Err(format!("invalid time word {word}"));
    }
    let mut hours = 0u64;
    let mut minutes = 0u64;
    let mut secs = 0u64;
    let mut micros = 0u32;
    match parts.len() {
        1 if interval => {
            let (s, u) = parse_seconds_field(parts[0], word)?;
            secs = s;
            micros = u;
        }
        1 => {
            hours = parse_whole(parts[0], word)?;
        }
        2 if interval => {
            minutes = parse_whole(parts[0], word)?;
            let (s, u) = parse_seconds_field(parts[1], word)?;
            secs = s;
            micros = u;
        }
        2 => {
            hours = parse_whole(parts[0], word)?;
            minutes = parse_whole(parts[1], word)?;
        }
        3 => {
            hours = parse_whole(parts[0], word)?;
            minutes = parse_whole(parts[1], word)?;
            let (s, u) = parse_seconds_field(parts[2], word)?;
            secs = s;
            micros = u;
        }
        _ => return Err(format!("invalid time word {word}")),
    }
    let total_secs = hours
        .checked_mul(3600)
        .and_then(|h| minutes.checked_mul(60).and_then(|m| h.checked_add(m)))
        .and_then(|hm| hm.checked_add(secs))
        .ok_or_else(|| format!("invalid time word {word}"))?;
    let dur = Duration::from_secs(total_secs) + Duration::from_micros(micros as u64);
    if interval {
        Ok(When::Interval(dur))
    } else {
        Ok(When::TimeOfDay(dur))
    }
}

fn parse_whole(s: &str, word: &str) -> Result<u64, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid time word {word}"));
    }
    s.parse::<u64>()
        .map_err(|_| format!("invalid time word {word}"))
}

fn parse_seconds_field(s: &str, word: &str) -> Result<(u64, u32), String> {
    let (whole, frac) = match s.split_once('.') {
        None => return Ok((parse_whole(s, word)?, 0)),
        Some((w, f)) => (w, f),
    };
    if frac.len() > 6 {
        return Err(format!("invalid time word {word}"));
    }
    if whole.is_empty() && frac.is_empty() {
        return Err(format!("invalid time word {word}"));
    }
    if !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid time word {word}"));
    }
    let secs = if whole.is_empty() {
        0
    } else {
        parse_whole(whole, word)?
    };
    let mut padded = frac.to_string();
    while padded.len() < 6 {
        padded.push('0');
    }
    let micros = padded
        .parse::<u32>()
        .map_err(|_| format!("invalid time word {word}"))?;
    Ok((secs, micros))
}

fn wait_duration(when: When, now_since_midnight: Duration) -> Duration {
    match when {
        When::Interval(d) => d,
        When::TimeOfDay(target) => {
            if target <= now_since_midnight {
                Duration::ZERO
            } else {
                target - now_since_midnight
            }
        }
    }
}

fn civil_now_since_midnight() -> Duration {
    if let Some(d) = CIVIL_NOW.with(|c| c.get()) {
        return d;
    }
    platform_civil_now()
}

fn platform_civil_now() -> Duration {
    #[cfg(unix)]
    {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        unix_civil_now(now)
    }
    #[cfg(windows)]
    {
        windows_civil_now()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        utc_since_midnight(now)
    }
}

#[cfg(unix)]
fn unix_civil_now(now: Duration) -> Duration {
    let secs_i = now.as_secs() as i64;
    match unix_local_hms(secs_i) {
        Some((h, m, s)) => {
            let h = h.max(0) as u64;
            let m = m.max(0) as u64;
            let s = s.max(0) as u64;
            Duration::from_secs(h * 3600 + m * 60 + s) + Duration::from_micros(now.subsec_micros() as u64)
        }
        None => utc_since_midnight(now),
    }
}

#[cfg(unix)]
fn unix_local_hms(secs: i64) -> Option<(i32, i32, i32)> {
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        _rest: [u64; 8],
    }
    #[cfg(target_pointer_width = "64")]
    type TimeT = i64;
    #[cfg(target_pointer_width = "32")]
    type TimeT = i32;

    extern "C" {
        fn localtime_r(timep: *const TimeT, result: *mut Tm) -> *mut Tm;
    }
    let t = secs as TimeT;
    let mut tm = unsafe { std::mem::zeroed() };
    let p = unsafe { localtime_r(&t, &mut tm) };
    if p.is_null() {
        None
    } else {
        Some((tm.tm_hour, tm.tm_min, tm.tm_sec))
    }
}

#[cfg(windows)]
fn windows_civil_now() -> Duration {
    #[repr(C)]
    struct SystemTime {
        w_year: u16,
        w_month: u16,
        w_day_of_week: u16,
        w_day: u16,
        w_hour: u16,
        w_minute: u16,
        w_second: u16,
        w_milliseconds: u16,
    }
    extern "system" {
        fn GetLocalTime(lp: *mut SystemTime);
    }
    let mut st = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    Duration::from_secs(st.w_hour as u64 * 3600 + st.w_minute as u64 * 60 + st.w_second as u64)
        + Duration::from_millis(st.w_milliseconds as u64)
}

#[cfg(not(windows))]
fn utc_since_midnight(now: Duration) -> Duration {
    let secs = now.as_secs() % 86_400;
    Duration::from_secs(secs) + Duration::from_micros(now.subsec_micros() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dur(h: u64, m: u64, s: u64, us: u64) -> Duration {
        Duration::from_secs(h * 3600 + m * 60 + s) + Duration::from_micros(us)
    }

    #[test]
    fn interval_one_two_three_parts() {
        assert_eq!(
            parse_time_word("+60").unwrap(),
            When::Interval(Duration::from_secs(60))
        );
        assert_eq!(
            parse_time_word("+1:30").unwrap(),
            When::Interval(Duration::from_secs(90))
        );
        assert_eq!(
            parse_time_word("+1:2:3").unwrap(),
            When::Interval(dur(1, 2, 3, 0))
        );
        assert_eq!(
            parse_time_word("+0").unwrap(),
            When::Interval(Duration::ZERO)
        );
        assert_eq!(
            parse_time_word("+0.000001").unwrap(),
            When::Interval(Duration::from_micros(1))
        );
    }

    #[test]
    fn interval_minutes_not_capped_at_sixty() {
        // p.149: +1:67 ≡ +2:7 ≡ +2:007
        let a = parse_time_word("+1:67").unwrap();
        let b = parse_time_word("+2:7").unwrap();
        let c = parse_time_word("+2:007").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a, When::Interval(Duration::from_secs(127)));
    }

    #[test]
    fn tod_one_two_three_parts() {
        assert_eq!(parse_time_word("3").unwrap(), When::TimeOfDay(dur(3, 0, 0, 0)));
        assert_eq!(
            parse_time_word("3:30").unwrap(),
            When::TimeOfDay(dur(3, 30, 0, 0))
        );
        assert_eq!(
            parse_time_word("3:30:15").unwrap(),
            When::TimeOfDay(dur(3, 30, 15, 0))
        );
        assert_eq!(parse_time_word("0").unwrap(), When::TimeOfDay(Duration::ZERO));
        assert_eq!(
            parse_time_word("24").unwrap(),
            When::TimeOfDay(Duration::from_secs(24 * 3600))
        );
        assert_eq!(
            parse_time_word("8760").unwrap(),
            When::TimeOfDay(Duration::from_secs(8760 * 3600))
        );
    }

    #[test]
    fn tod_seconds_not_capped_at_sixty() {
        // p.321: 1:17:64 ≡ 1:18:04
        let a = parse_time_word("1:17:64").unwrap();
        let b = parse_time_word("1:18:04").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, When::TimeOfDay(dur(1, 17, 64, 0)));
    }

    #[test]
    fn fractional_seconds_only() {
        assert_eq!(
            parse_time_word("+0.5").unwrap(),
            When::Interval(Duration::from_millis(500))
        );
        assert_eq!(
            parse_time_word("+1:30.5").unwrap(),
            When::Interval(Duration::from_millis(90_500))
        );
        assert_eq!(
            parse_time_word("1:2:3.25").unwrap(),
            When::TimeOfDay(dur(1, 2, 3, 250_000))
        );
        assert!(parse_time_word("3.5").is_err());
        assert!(parse_time_word("1:30.5").is_err());
        assert!(parse_time_word("+0.1234567").is_err());
    }

    #[test]
    fn rejects_undocumented_time_words() {
        for w in ["", "+", "++60", "-1", "+-1", "1:2:3:4", "abc", "+:60", "1:", ":1"] {
            assert!(parse_time_word(w).is_err(), "{w}");
        }
    }

    #[test]
    fn past_tod_does_not_wait() {
        // Note 3: literal 0 | delay never waits.
        let now = dur(15, 0, 0, 0);
        assert_eq!(
            wait_duration(When::TimeOfDay(Duration::ZERO), now),
            Duration::ZERO
        );
        assert_eq!(
            wait_duration(When::TimeOfDay(dur(3, 0, 0, 0)), now),
            Duration::ZERO
        );
        assert_eq!(
            wait_duration(When::TimeOfDay(dur(24, 0, 0, 0)), now),
            dur(9, 0, 0, 0)
        );
    }

    #[test]
    fn first_word_is_blank_delimited() {
        assert_eq!(first_word(b"+0 rest"), Some(&b"+0"[..]));
        assert_eq!(first_word(b"  24"), Some(&b"24"[..]));
        assert_eq!(first_word(b""), None);
        assert_eq!(first_word(b"   "), None);
    }
}
