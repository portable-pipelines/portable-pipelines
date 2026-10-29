//! DATECONVERT — Author's Edition 1.1.11 p.311.
//!
//! Portable date conversion. This is not the IBM DATE stage. Supported
//! formats are a small host set (ISO, YMD/ORDERED, DMY/EUROPEAN, MDY/USA,
//! JULIAN, GREG). The first word of each record is converted; the rest of
//! the record is copied through. Invalid dates go to the secondary output
//! when it is connected, otherwise the stage fails.

use pipe_core::{Record, StageCtx, StreamId};

use crate::civil::{
    from_year_day, join_word_rest, parse_gregorian, split_first_word, CivilDateTime,
};
use crate::common::matches_abbrev;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DateFormat {
    Iso,
    Ymd,
    Dmy,
    Mdy,
    Julian,
    Greg,
}

pub fn dateconvert(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_dateconvert(&args) {
        Ok((from, to)) => run_dateconvert(ctx, from, to),
        Err(e) => ctx.fail(format!("DATECONVERT: {e}")),
    })
}

fn parse_dateconvert(args: &[String]) -> Result<(DateFormat, DateFormat), String> {
    let mut from = DateFormat::Iso;
    let mut to = DateFormat::Iso;
    let mut i = 0;
    while i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u == "FROM" {
            i += 1;
            let name = args.get(i).ok_or("FROM requires a format")?;
            from = parse_format(name)?;
            i += 1;
        } else if u == "TO" {
            i += 1;
            let name = args.get(i).ok_or("TO requires a format")?;
            to = parse_format(name)?;
            i += 1;
        } else {
            return Err(format!("unrecognized operand {}", args[i]));
        }
    }
    Ok((from, to))
}

fn parse_format(name: &str) -> Result<DateFormat, String> {
    let u = name.to_ascii_uppercase();
    if u == "ISO" {
        Ok(DateFormat::Iso)
    } else if u == "YMD" || matches_abbrev(&u, "ORDERED", 3) {
        Ok(DateFormat::Ymd)
    } else if u == "DMY" || matches_abbrev(&u, "EUROPEAN", 3) {
        Ok(DateFormat::Dmy)
    } else if u == "MDY" || u == "USA" {
        Ok(DateFormat::Mdy)
    } else if matches_abbrev(&u, "JULIAN", 3) {
        Ok(DateFormat::Julian)
    } else if matches_abbrev(&u, "GREG", 4) || matches_abbrev(&u, "GREGORIAN", 4) {
        Ok(DateFormat::Greg)
    } else {
        Err(format!("unrecognized date format {name}"))
    }
}

fn run_dateconvert(ctx: &mut StageCtx, from: DateFormat, to: DateFormat) {
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("DATECONVERT: only the primary input may be connected");
            return;
        }
    }
    if !ctx.input_connected(StreamId::PRIMARY) {
        ctx.fail("DATECONVERT must not be a first stage");
        return;
    }

    let secondary = ctx.output_connected(StreamId::SECONDARY);
    ctx.select_input(StreamId::PRIMARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                let line = rec.to_string_lossy();
                let (word, rest) = split_first_word(&line);
                if word.is_empty() {
                    if !emit_bad(ctx, rec, secondary, "missing date") {
                        return;
                    }
                    continue;
                }
                match parse_in(from, word).and_then(|dt| format_out(to, dt)) {
                    Ok(out) => {
                        if !ctx.output_connected(StreamId::PRIMARY) {
                            if ctx.input_connected(StreamId::PRIMARY) {
                                ctx.short_input();
                            }
                            return;
                        }
                        ctx.select_output(StreamId::PRIMARY);
                        let body = join_word_rest(&out, rest);
                        if ctx.write(Record::from(body)).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        if !emit_bad(ctx, rec, secondary, &e) {
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

fn emit_bad(ctx: &mut StageCtx, rec: Record, secondary: bool, err: &str) -> bool {
    if secondary {
        ctx.select_output(StreamId::SECONDARY);
        ctx.write(rec).is_ok()
    } else {
        ctx.fail(format!("DATECONVERT: {err}"));
        false
    }
}

fn parse_in(fmt: DateFormat, word: &str) -> Result<CivilDateTime, String> {
    match fmt {
        DateFormat::Iso => parse_iso_date(word),
        DateFormat::Ymd => parse_fixed_ymd(word),
        DateFormat::Dmy => parse_dmy(word),
        DateFormat::Mdy => parse_mdy(word),
        DateFormat::Julian => parse_julian(word),
        DateFormat::Greg => parse_gregorian(word)
            .map(|dt| CivilDateTime::date(dt.year, dt.month, dt.day).unwrap_or(dt)),
    }
}

fn format_out(fmt: DateFormat, dt: CivilDateTime) -> Result<String, String> {
    match fmt {
        DateFormat::Iso => dt.to_iso_date(),
        DateFormat::Ymd => dt.to_ymd(),
        DateFormat::Dmy => dt.to_dmy_slash(),
        DateFormat::Mdy => dt.to_mdy_slash(),
        DateFormat::Julian => dt.to_julian(),
        DateFormat::Greg => {
            let midnight = CivilDateTime::new(dt.year, dt.month, dt.day, 0, 0, 0)?;
            midnight.to_greg_digits()
        }
    }
}

fn parse_iso_date(word: &str) -> Result<CivilDateTime, String> {
    let t = word.trim();
    let parts: Vec<&str> = t.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("unrecognized ISO date {word}"));
    }
    let year: i32 = parts[0]
        .parse()
        .map_err(|_| format!("unrecognized ISO date {word}"))?;
    let month: u8 = parts[1]
        .parse()
        .map_err(|_| format!("unrecognized ISO date {word}"))?;
    let day: u8 = parts[2]
        .parse()
        .map_err(|_| format!("unrecognized ISO date {word}"))?;
    CivilDateTime::date(year, month, day)
}

fn parse_fixed_ymd(word: &str) -> Result<CivilDateTime, String> {
    let t = word.trim();
    if t.len() != 8 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("unrecognized YMD date {word}"));
    }
    let year: i32 = t[0..4].parse().unwrap();
    let month: u8 = t[4..6].parse().unwrap();
    let day: u8 = t[6..8].parse().unwrap();
    CivilDateTime::date(year, month, day)
}

fn parse_dmy(word: &str) -> Result<CivilDateTime, String> {
    parse_slash_date(word, true)
}

fn parse_mdy(word: &str) -> Result<CivilDateTime, String> {
    parse_slash_date(word, false)
}

fn parse_slash_date(word: &str, dmy: bool) -> Result<CivilDateTime, String> {
    let t = word.trim();
    let sep = if t.contains('/') {
        '/'
    } else if t.contains('-') {
        '-'
    } else if t.contains('.') {
        '.'
    } else {
        return Err(format!("unrecognized date {word}"));
    };
    let parts: Vec<&str> = t.split(sep).collect();
    if parts.len() != 3 {
        return Err(format!("unrecognized date {word}"));
    }
    let a: u8 = parts[0]
        .parse()
        .map_err(|_| format!("unrecognized date {word}"))?;
    let b: u8 = parts[1]
        .parse()
        .map_err(|_| format!("unrecognized date {word}"))?;
    let year: i32 = parts[2]
        .parse()
        .map_err(|_| format!("unrecognized date {word}"))?;
    if dmy {
        CivilDateTime::date(year, b, a)
    } else {
        CivilDateTime::date(year, a, b)
    }
}

fn parse_julian(word: &str) -> Result<CivilDateTime, String> {
    let t = word.trim();
    if t.len() != 7 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("unrecognized Julian date {word}"));
    }
    let year: i32 = t[0..4].parse().unwrap();
    let doy: u32 = t[4..7].parse().unwrap();
    from_year_day(year, doy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_round_trip_iso_ymd_julian() {
        let dt = parse_in(DateFormat::Iso, "2026-10-29").unwrap();
        assert_eq!(format_out(DateFormat::Ymd, dt).unwrap(), "20261029");
        assert_eq!(format_out(DateFormat::Julian, dt).unwrap(), "2026302");
        assert_eq!(format_out(DateFormat::Dmy, dt).unwrap(), "29/10/2026");
        assert_eq!(format_out(DateFormat::Mdy, dt).unwrap(), "10/29/2026");
        assert_eq!(format_out(DateFormat::Greg, dt).unwrap(), "20261029000000");
        assert_eq!(parse_in(DateFormat::Ymd, "20261029").unwrap(), dt);
        assert_eq!(parse_in(DateFormat::Julian, "2026302").unwrap(), dt);
        assert_eq!(parse_in(DateFormat::Dmy, "29/10/2026").unwrap(), dt);
        assert_eq!(parse_in(DateFormat::Mdy, "10/29/2026").unwrap(), dt);
    }

    #[test]
    fn from_to_keywords() {
        let (from, to) =
            parse_dateconvert(&["FROM".into(), "ISO".into(), "TO".into(), "YMD".into()]).unwrap();
        assert_eq!(from, DateFormat::Iso);
        assert_eq!(to, DateFormat::Ymd);
        assert!(parse_dateconvert(&["FROM".into()]).is_err());
        assert!(parse_dateconvert(&["bogus".into()]).is_err());
    }

    #[test]
    fn rejects_invalid_dates() {
        assert!(parse_in(DateFormat::Iso, "2026-13-01").is_err());
        assert!(parse_in(DateFormat::Iso, "2026-02-30").is_err());
        assert!(parse_in(DateFormat::Julian, "2026366").is_err());
        assert!(parse_in(DateFormat::Ymd, "20260229").is_err());
        assert!(parse_in(DateFormat::Ymd, "20200229").is_ok());
    }
}
