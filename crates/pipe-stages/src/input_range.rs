//! `inputRange` from Author's Edition 1.1.11 pp.195–196 (syntactic variables).
//!
//! JOINCONT `RANGE` is the first consumer. Column, word, and field ranges are
//! supported, with `SUBSTRING … OF`, `WORDSEPARATOR`/`WS`, and
//! `FIELDSEPARATOR`/`FS`. Portable ASCII: blank/word separator is X'20' (book
//! cites EBCDIC X'40'); default field/tab separator is HT X'09' (book cites
//! EBCDIC X'05').

use crate::common::matches_abbrev;

/// Default blank (portable ASCII space; book xorc BLANK is EBCDIC X'40').
pub const WORD_SEP: u8 = b' ';
/// Default field separator (portable ASCII HT; book default is EBCDIC X'05').
pub const FIELD_SEP: u8 = b'\t';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pos {
    /// 1-based from the start when positive; from the end when negative (`-1` is last).
    Signed(i32),
    Star,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColRange {
    One(Pos),
    Span { start: Pos, end: Pos },
    Length { start: Pos, len: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Columns,
    Words { sep: u8 },
    Fields { sep: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangePart {
    pub units: Units,
    pub range: ColRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputRange {
    /// Applied inside-out: last is the outermost `SUBSTRING`.
    pub parts: Vec<RangePart>,
}

/// Parse one `inputRange` from `args` starting at `i`. Advances `i`.
pub fn parse_input_range(args: &[String], i: &mut usize) -> Result<InputRange, String> {
    let mut word_sep = WORD_SEP;
    let mut field_sep = FIELD_SEP;

    // Leading WORDSEPARATOR / FIELDSEPARATOR (remain in effect).
    while *i < args.len() {
        let upper = args[*i].to_ascii_uppercase();
        if is_wordsep(&upper) {
            *i += 1;
            word_sep = take_xorc(args, i, "WORDSEPARATOR")?;
        } else if is_fieldsep(&upper) {
            *i += 1;
            field_sep = take_xorc(args, i, "FIELDSEPARATOR")?;
        } else {
            break;
        }
    }

    let mut substrings = Vec::new();
    while *i < args.len() {
        let upper = args[*i].to_ascii_uppercase();
        if matches_abbrev(&upper, "SUBSTRING", 6) {
            *i += 1;
            let part = parse_range_part(args, i, word_sep, field_sep)?;
            if *i >= args.len() || !args[*i].eq_ignore_ascii_case("OF") {
                return Err("SUBSTRING requires OF".into());
            }
            *i += 1;
            substrings.push(part);
        } else {
            break;
        }
    }

    let inner = parse_range_part(args, i, word_sep, field_sep)?;
    let mut parts = vec![inner];
    parts.extend(substrings);
    Ok(InputRange { parts })
}

fn parse_range_part(
    args: &[String],
    i: &mut usize,
    mut word_sep: u8,
    mut field_sep: u8,
) -> Result<RangePart, String> {
    if *i >= args.len() {
        return Err("incomplete inputRange".into());
    }

    // Optional separator override immediately before WORDS/FIELDS.
    let upper = args[*i].to_ascii_uppercase();
    if is_wordsep(&upper) {
        *i += 1;
        word_sep = take_xorc(args, i, "WORDSEPARATOR")?;
    } else if is_fieldsep(&upper) {
        *i += 1;
        field_sep = take_xorc(args, i, "FIELDSEPARATOR")?;
    }

    if *i >= args.len() {
        return Err("incomplete inputRange".into());
    }
    let upper = args[*i].to_ascii_uppercase();
    if is_words(&upper) {
        *i += 1;
        let range = take_col_range(args, i, "WORDS")?;
        return Ok(RangePart {
            units: Units::Words { sep: word_sep },
            range,
        });
    }
    if is_fields(&upper) {
        *i += 1;
        let range = take_col_range(args, i, "FIELDS")?;
        return Ok(RangePart {
            units: Units::Fields { sep: field_sep },
            range,
        });
    }

    let range = take_col_range(args, i, "column range")?;
    Ok(RangePart {
        units: Units::Columns,
        range,
    })
}

fn take_col_range(args: &[String], i: &mut usize, what: &str) -> Result<ColRange, String> {
    if *i >= args.len() {
        return Err(format!("{what} requires a range"));
    }
    let tok = &args[*i];
    let range =
        parse_col_range_token(tok).ok_or_else(|| format!("{what} requires a range, got {tok}"))?;
    *i += 1;
    Ok(range)
}

fn take_xorc(args: &[String], i: &mut usize, what: &str) -> Result<u8, String> {
    if *i >= args.len() {
        return Err(format!("{what} requires an xorc"));
    }
    let b = parse_xorc(&args[*i])?;
    *i += 1;
    Ok(b)
}

pub fn parse_xorc(s: &str) -> Result<u8, String> {
    try_xorc(s).ok_or_else(|| format!("invalid xorc {s}"))
}

fn try_xorc(s: &str) -> Option<u8> {
    let upper = s.to_ascii_uppercase();
    if upper == "BLANK" || upper == "SPACE" {
        return Some(WORD_SEP);
    }
    if matches_abbrev(&upper, "TABULATE", 3) {
        return Some(FIELD_SEP);
    }
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return u8::from_str_radix(s, 16).ok();
    }
    if s.len() == 1 {
        return Some(s.as_bytes()[0]);
    }
    None
}

fn parse_col_range_token(s: &str) -> Option<ColRange> {
    if let Some((left, right)) = split_once_unescaped(s, ';') {
        let start = parse_snumber(left)?;
        let end = parse_snumber(right)?;
        return Some(ColRange::Span {
            start: Pos::Signed(start),
            end: Pos::Signed(end),
        });
    }
    if let Some((left, right)) = split_once_unescaped(s, '.') {
        if !left.is_empty() && !right.is_empty() {
            let start = parse_numorstar(left)?;
            let len = right.parse::<usize>().ok()?;
            return Some(ColRange::Length { start, len });
        }
    }
    // Hyphen range: not a leading sign. `-2;-1` uses semicolon; `1-3` / `*-*` here.
    if let Some(idx) = s
        .char_indices()
        .find_map(|(i, c)| if c == '-' && i > 0 { Some(i) } else { None })
    {
        let left = &s[..idx];
        let right = &s[idx + 1..];
        if !left.is_empty() && !right.is_empty() {
            if let (Some(start), Some(end)) = (parse_numorstar(left), parse_numorstar(right)) {
                return Some(ColRange::Span { start, end });
            }
        }
    }
    if s == "*" {
        // Lone `*` is not a listed range form (use `1-*` or `*-*`).
        return None;
    }
    let n = parse_snumber(s)?;
    Some(ColRange::One(Pos::Signed(n)))
}

fn parse_numorstar(s: &str) -> Option<Pos> {
    if s == "*" {
        return Some(Pos::Star);
    }
    parse_snumber(s).map(Pos::Signed)
}

fn parse_snumber(s: &str) -> Option<i32> {
    if s.is_empty() {
        return None;
    }
    s.parse::<i32>().ok()
}

fn split_once_unescaped(s: &str, sep: char) -> Option<(&str, &str)> {
    let i = s.find(sep)?;
    Some((&s[..i], &s[i + sep.len_utf8()..]))
}

fn is_wordsep(upper: &str) -> bool {
    matches_abbrev(upper, "WORDSEPARATOR", 7) || upper == "WS"
}

fn is_fieldsep(upper: &str) -> bool {
    matches_abbrev(upper, "FIELDSEPARATOR", 7) || upper == "FS"
}

fn is_words(upper: &str) -> bool {
    matches_abbrev(upper, "WORDS", 1) && upper.as_bytes()[0] == b'W'
}

fn is_fields(upper: &str) -> bool {
    matches_abbrev(upper, "FIELDS", 1) && upper.as_bytes()[0] == b'F'
}

/// Bytes selected by `range` from `record`.
pub fn extract_input_range(record: &[u8], range: &InputRange) -> Vec<u8> {
    let mut cur = record.to_vec();
    for part in &range.parts {
        cur = extract_part(&cur, part);
    }
    cur
}

fn extract_part(record: &[u8], part: &RangePart) -> Vec<u8> {
    match part.units {
        Units::Columns => slice_items(record, part.range, 1).unwrap_or_default(),
        Units::Words { sep } => extract_units(record, sep, false, part.range),
        Units::Fields { sep } => extract_units(record, sep, true, part.range),
    }
}

fn extract_units(record: &[u8], sep: u8, keep_empty: bool, range: ColRange) -> Vec<u8> {
    let spans = unit_spans(record, sep, keep_empty);
    if spans.is_empty() {
        return Vec::new();
    }
    let Some((a, b)) = resolve_range(range, spans.len()) else {
        return Vec::new();
    };
    if a >= b || a >= spans.len() {
        return Vec::new();
    }
    let last = (b - 1).min(spans.len() - 1);
    let start = spans[a].0;
    let end = spans[last].1;
    if start >= end || start > record.len() {
        Vec::new()
    } else {
        record[start..end.min(record.len())].to_vec()
    }
}

/// Item spans `(start, end)` in `record`.
fn unit_spans(record: &[u8], sep: u8, keep_empty: bool) -> Vec<(usize, usize)> {
    if keep_empty {
        let mut spans = Vec::new();
        let mut start = 0usize;
        for (i, &b) in record.iter().enumerate() {
            if b == sep {
                spans.push((start, i));
                start = i + 1;
            }
        }
        spans.push((start, record.len()));
        spans
    } else {
        let mut spans = Vec::new();
        let mut i = 0;
        while i < record.len() {
            while i < record.len() && record[i] == sep {
                i += 1;
            }
            if i >= record.len() {
                break;
            }
            let start = i;
            while i < record.len() && record[i] != sep {
                i += 1;
            }
            spans.push((start, i));
        }
        spans
    }
}

fn slice_items(items: &[u8], range: ColRange, _item_len: usize) -> Option<Vec<u8>> {
    let n = items.len();
    let (a, b) = resolve_range(range, n)?;
    if a >= b || a >= n {
        return Some(Vec::new());
    }
    Some(items[a..b.min(n)].to_vec())
}

/// 0-based half-open `[start, end)` into a sequence of `n` items.
fn resolve_range(range: ColRange, n: usize) -> Option<(usize, usize)> {
    match range {
        ColRange::One(pos) => {
            let c = resolve_index(pos, n, true)?;
            if c == 0 || c > n {
                Some((c.saturating_sub(1).min(n), c.saturating_sub(1).min(n)))
            } else {
                Some((c - 1, c))
            }
        }
        ColRange::Span { start, end } => {
            let s = resolve_index(start, n, true)?;
            let e = resolve_index(end, n, false)?;
            if s == 0 || e == 0 {
                return Some((0, 0));
            }
            // Opposite ends: null if ending column is left of beginning.
            if e < s {
                return Some((0, 0));
            }
            let start_i = s.saturating_sub(1);
            let end_i = e.min(n);
            if start_i >= n {
                Some((n, n))
            } else {
                Some((start_i, end_i.max(start_i)))
            }
        }
        ColRange::Length { start, len } => {
            let s = resolve_index(start, n, true)?;
            if s == 0 || s > n {
                return Some((n, n));
            }
            let start_i = s - 1;
            let end_i = (start_i.saturating_add(len)).min(n);
            Some((start_i, end_i))
        }
    }
}

/// 1-based index. `from_start` chooses `*` as 1 vs `n`.
fn resolve_index(pos: Pos, n: usize, from_start: bool) -> Option<usize> {
    match pos {
        Pos::Star => {
            if from_start {
                Some(1)
            } else {
                Some(n)
            }
        }
        Pos::Signed(0) => Some(0),
        Pos::Signed(v) if v > 0 => Some(v as usize),
        Pos::Signed(v) => {
            // -1 → n, -2 → n-1
            let idx = n as i64 + i64::from(v) + 1;
            if idx < 1 {
                Some(0)
            } else {
                Some(idx as usize)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(words: &[&str]) -> InputRange {
        let args: Vec<String> = words.iter().map(|s| (*s).to_string()).collect();
        let mut i = 0;
        let r = parse_input_range(&args, &mut i).expect("range");
        assert_eq!(i, args.len(), "unconsumed {:?}", &args[i..]);
        r
    }

    #[test]
    fn column_one_and_span() {
        let rec = b"abcdef";
        assert_eq!(extract_input_range(rec, &parse_all(&["1"])), b"a");
        assert_eq!(extract_input_range(rec, &parse_all(&["2-4"])), b"bcd");
        assert_eq!(extract_input_range(rec, &parse_all(&["1-*"])), b"abcdef");
        assert_eq!(extract_input_range(rec, &parse_all(&["4.2"])), b"de");
        assert_eq!(extract_input_range(rec, &parse_all(&["-1"])), b"f");
        assert_eq!(extract_input_range(rec, &parse_all(&["-2;-1"])), b"ef");
        assert_eq!(extract_input_range(rec, &parse_all(&["1;-1"])), b"abcdef");
    }

    #[test]
    fn words_and_fields() {
        let rec = b"one  two three";
        assert_eq!(extract_input_range(rec, &parse_all(&["word", "2"])), b"two");
        assert_eq!(
            extract_input_range(rec, &parse_all(&["words", "1-2"])),
            b"one  two"
        );
        let rec = b"a\tb\t\tc";
        assert_eq!(extract_input_range(rec, &parse_all(&["field", "2"])), b"b");
        assert_eq!(extract_input_range(rec, &parse_all(&["f", "3"])), b"");
        assert_eq!(
            extract_input_range(b"a*b*c", &parse_all(&["fieldsep", "*", "field", "2"])),
            b"b"
        );
    }

    #[test]
    fn substring_of_word() {
        let rec = b"What a rum fellow.";
        let r = parse_all(&["substr", "2", "of", "word", "3"]);
        assert_eq!(extract_input_range(rec, &r), b"u");
    }
}
