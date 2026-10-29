//! Portable Gregorian calendar and Unix-epoch helpers.
//!
//! Civil dates are proleptic Gregorian in UTC. There is no local time zone
//! and there are no leap seconds. Used by GREG2SEC, SEC2GREG, DATECONVERT,
//! and TIMESTAMP.

use std::fmt;

/// A civil date and time in UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CivilDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl fmt::Display for CivilDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

impl CivilDateTime {
    pub fn date(year: i32, month: u8, day: u8) -> Result<Self, String> {
        Self::new(year, month, day, 0, 0, 0)
    }

    pub fn new(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Result<Self, String> {
        if !(1..=12).contains(&month) {
            return Err(format!("month {month} is not 1 through 12"));
        }
        let dim = days_in_month(year, month);
        if day < 1 || day > dim {
            return Err(format!("day {day} is not valid for {year:04}-{month:02}"));
        }
        if hour > 23 {
            return Err(format!("hour {hour} is not 0 through 23"));
        }
        if minute > 59 {
            return Err(format!("minute {minute} is not 0 through 59"));
        }
        if second > 59 {
            return Err(format!("second {second} is not 0 through 59"));
        }
        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }

    pub fn day_of_year(self) -> u32 {
        let mut n = self.day as u32;
        for m in 1..self.month {
            n += days_in_month(self.year, m) as u32;
        }
        n
    }

    /// Compact `yyyymmddhhmmss` (year 0 through 9999).
    pub fn to_greg_digits(self) -> Result<String, String> {
        if !(0..=9999).contains(&self.year) {
            return Err(format!("year {} does not fit yyyymmddhhmmss", self.year));
        }
        Ok(format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        ))
    }

    pub fn to_iso_date(self) -> Result<String, String> {
        if !(0..=9999).contains(&self.year) {
            return Err(format!("year {} does not fit YYYY-MM-DD", self.year));
        }
        Ok(format!(
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        ))
    }

    pub fn to_ymd(self) -> Result<String, String> {
        if !(0..=9999).contains(&self.year) {
            return Err(format!("year {} does not fit YYYYMMDD", self.year));
        }
        Ok(format!("{:04}{:02}{:02}", self.year, self.month, self.day))
    }

    pub fn to_dmy_slash(self) -> Result<String, String> {
        if !(0..=9999).contains(&self.year) {
            return Err(format!("year {} does not fit DD/MM/YYYY", self.year));
        }
        Ok(format!(
            "{:02}/{:02}/{:04}",
            self.day, self.month, self.year
        ))
    }

    pub fn to_mdy_slash(self) -> Result<String, String> {
        if !(0..=9999).contains(&self.year) {
            return Err(format!("year {} does not fit MM/DD/YYYY", self.year));
        }
        Ok(format!(
            "{:02}/{:02}/{:04}",
            self.month, self.day, self.year
        ))
    }

    pub fn to_julian(self) -> Result<String, String> {
        if !(0..=9999).contains(&self.year) {
            return Err(format!("year {} does not fit YYYYDDD", self.year));
        }
        Ok(format!("{:04}{:03}", self.year, self.day_of_year()))
    }
}

pub fn is_leap_year(year: i32) -> bool {
    let y = year;
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

pub fn from_year_day(year: i32, doy: u32) -> Result<CivilDateTime, String> {
    if doy < 1 {
        return Err(format!("Julian day {doy} is not valid"));
    }
    let mut left = doy;
    for m in 1..=12 {
        let dim = days_in_month(year, m) as u32;
        if left <= dim {
            return CivilDateTime::date(year, m, left as u8);
        }
        left -= dim;
    }
    Err(format!("Julian day {doy} is not valid for year {year}"))
}

/// Days since 1970-01-01 (Howard Hinnant civil calendar).
pub fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let mut y = year;
    let m = month as i32;
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u32;
    let doy = (153 * mp + 2) / 5 + day as u32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe as i64 - 719468
}

pub fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year, m as u8, d as u8)
}

pub fn to_unix_seconds(dt: CivilDateTime) -> i64 {
    let days = days_from_civil(dt.year, dt.month, dt.day);
    days * 86400 + dt.hour as i64 * 3600 + dt.minute as i64 * 60 + dt.second as i64
}

pub fn from_unix_seconds(secs: i64) -> Result<CivilDateTime, String> {
    let days = secs.div_euclid(86400);
    let sod = secs.rem_euclid(86400) as u32;
    let (year, month, day) = civil_from_days(days);
    let hour = (sod / 3600) as u8;
    let minute = ((sod % 3600) / 60) as u8;
    let second = (sod % 60) as u8;
    CivilDateTime::new(year, month, day, hour, minute, second)
}

/// Parse `yyyymmddhhmmss` or a few separated UTC forms.
pub fn parse_gregorian(word: &str) -> Result<CivilDateTime, String> {
    let t = word.trim();
    if t.is_empty() {
        return Err("missing Gregorian timestamp".into());
    }
    if t.len() == 14 && t.bytes().all(|b| b.is_ascii_digit()) {
        return from_greg_digits(t);
    }
    if t.len() == 10 && t.as_bytes().get(4) == Some(&b'-') && t.as_bytes().get(7) == Some(&b'-') {
        if let Some((y, m, d)) = parse_ymd_dashed(t) {
            return CivilDateTime::date(y, m, d);
        }
    }
    if let Some(dt) = parse_separated_datetime(t) {
        return dt;
    }
    Err(format!("unrecognized Gregorian timestamp {word}"))
}

fn from_greg_digits(t: &str) -> Result<CivilDateTime, String> {
    let year: i32 = t[0..4].parse().unwrap();
    let month: u8 = t[4..6].parse().unwrap();
    let day: u8 = t[6..8].parse().unwrap();
    let hour: u8 = t[8..10].parse().unwrap();
    let minute: u8 = t[10..12].parse().unwrap();
    let second: u8 = t[12..14].parse().unwrap();
    CivilDateTime::new(year, month, day, hour, minute, second)
}

fn parse_separated_datetime(t: &str) -> Option<Result<CivilDateTime, String>> {
    let sep = if t.len() >= 11 && (t.as_bytes()[10] == b'T' || t.as_bytes()[10] == b' ') {
        t.as_bytes()[10] as char
    } else {
        return None;
    };
    let (date, time) = t.split_once(sep)?;
    let (y, m, d) = parse_ymd_dashed(date)?;
    let (h, min, s) = parse_hms(time)?;
    Some(CivilDateTime::new(y, m, d, h, min, s))
}

fn parse_ymd_dashed(s: &str) -> Option<(i32, u8, u8)> {
    let mut p = s.split('-');
    let y = p.next()?.parse().ok()?;
    let m = p.next()?.parse().ok()?;
    let d = p.next()?.parse().ok()?;
    if p.next().is_some() {
        return None;
    }
    Some((y, m, d))
}

fn parse_hms(s: &str) -> Option<(u8, u8, u8)> {
    let mut p = s.split(':');
    let h = p.next()?.parse().ok()?;
    let m = p.next()?.parse().ok()?;
    let sec = p.next()?.parse().ok()?;
    if p.next().is_some() {
        return None;
    }
    Some((h, m, sec))
}

pub fn parse_unix_seconds_word(word: &str) -> Result<i64, String> {
    let t = word.trim();
    if t.is_empty() {
        return Err("missing seconds value".into());
    }
    if t.contains('.') {
        return Err(format!("seconds value {word} is not a whole number"));
    }
    t.parse::<i64>()
        .map_err(|_| format!("seconds value {word} is not a whole number"))
}

/// First blank-delimited token and the remainder (leading blanks on the rest stripped).
pub fn split_first_word(rec: &str) -> (&str, &str) {
    let s = rec.trim_start();
    match s.find(|c: char| c.is_ascii_whitespace()) {
        Some(i) => {
            let word = &s[..i];
            let rest = s[i..].trim_start();
            (word, rest)
        }
        None => (s, ""),
    }
}

pub fn join_word_rest(word: &str, rest: &str) -> String {
    if rest.is_empty() {
        word.to_string()
    } else {
        format!("{word} {rest}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_zero() {
        let dt = CivilDateTime::new(1970, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(to_unix_seconds(dt), 0);
        assert_eq!(from_unix_seconds(0).unwrap(), dt);
        assert_eq!(dt.to_greg_digits().unwrap(), "19700101000000");
    }

    #[test]
    fn before_epoch_is_negative() {
        let dt = CivilDateTime::new(1969, 12, 31, 23, 59, 59).unwrap();
        assert_eq!(to_unix_seconds(dt), -1);
        assert_eq!(from_unix_seconds(-1).unwrap(), dt);
    }

    #[test]
    fn known_utc_instants() {
        let dt = CivilDateTime::new(2026, 10, 29, 12, 30, 0).unwrap();
        assert_eq!(to_unix_seconds(dt), 1_793_277_000);
        assert_eq!(from_unix_seconds(1_793_277_000).unwrap(), dt);
    }

    #[test]
    fn leap_day_round_trip() {
        let dt = CivilDateTime::date(2020, 2, 29).unwrap();
        assert_eq!(dt.day_of_year(), 60);
        let secs = to_unix_seconds(dt);
        assert_eq!(from_unix_seconds(secs).unwrap(), dt);
        assert!(CivilDateTime::date(2019, 2, 29).is_err());
    }

    #[test]
    fn julian_day_302_is_29_oct_non_leap() {
        let dt = from_year_day(2026, 302).unwrap();
        assert_eq!(dt, CivilDateTime::date(2026, 10, 29).unwrap());
        assert_eq!(dt.to_julian().unwrap(), "2026302");
    }

    #[test]
    fn parse_greg_digits_and_separated() {
        let a = parse_gregorian("19700101000000").unwrap();
        let b = parse_gregorian("1970-01-01 00:00:00").unwrap();
        let c = parse_gregorian("1970-01-01T00:00:00").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert!(parse_gregorian("not-a-date").is_err());
        assert!(parse_gregorian("20261301000000").is_err());
    }

    #[test]
    fn days_from_civil_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(1970, 1, 2), 1);
    }
}
