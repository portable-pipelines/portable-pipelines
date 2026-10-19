//! Shared `xrange` / xorc / STRING / ANYOF target matching.
//!
//! Used by SPLIT and CHOP (Author's Edition 1.1.11 syntactic variables pp.194–197).
//! Portable ASCII: xorc `BLANK`/`SPACE` is X'20' (book cites EBCDIC X'40');
//! `TABULATE` is ASCII HT X'09' (book cites EBCDIC X'05').

use crate::common::matches_abbrev;

/// 256-bit set of bytes (xorc / xrange / ANYOF).
#[derive(Clone, Debug)]
pub struct ByteSet {
    bits: [u64; 4],
}

impl ByteSet {
    pub fn new() -> Self {
        Self { bits: [0; 4] }
    }

    pub fn insert(&mut self, b: u8) {
        let i = (b >> 6) as usize;
        self.bits[i] |= 1u64 << (b & 63);
    }

    pub fn contains(&self, b: u8) -> bool {
        let i = (b >> 6) as usize;
        self.bits[i] & (1u64 << (b & 63)) != 0
    }

    pub fn insert_inclusive_wrap(&mut self, from: u8, to: u8) {
        if from <= to {
            for b in from..=to {
                self.insert(b);
            }
        } else {
            for b in from..=255 {
                self.insert(b);
            }
            for b in 0..=to {
                self.insert(b);
            }
        }
    }

    pub fn insert_count_wrap(&mut self, from: u8, count: u32) {
        let mut b = from;
        for _ in 0..count {
            self.insert(b);
            b = b.wrapping_add(1);
        }
    }

    pub fn with_anycase(mut self) -> Self {
        let original = self.bits;
        for b in 0u8..=255 {
            let i = (b >> 6) as usize;
            if original[i] & (1u64 << (b & 63)) != 0 {
                self.insert(b.to_ascii_uppercase());
                self.insert(b.to_ascii_lowercase());
            }
        }
        self
    }

    pub fn from_bytes<I: IntoIterator<Item = u8>>(bytes: I, anycase: bool) -> Self {
        let mut set = Self::new();
        for b in bytes {
            set.insert(b);
        }
        if anycase {
            set = set.with_anycase();
        }
        set
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    At,
    Before(i32),
    After(i32),
}

#[derive(Clone, Debug)]
pub enum Target {
    Blank,
    Bytes(ByteSet),
    String(Vec<u8>),
}

/// xorc: one-character word, two hex digits, BLANK/SPACE, or TABULATE.
pub fn parse_xorc(s: &str) -> Result<u8, String> {
    try_xorc(s).ok_or_else(|| format!("invalid xorc {s}"))
}

pub fn try_xorc(s: &str) -> Option<u8> {
    let upper = s.to_ascii_uppercase();
    if upper == "BLANK" || upper == "SPACE" {
        return Some(b' ');
    }
    if matches_abbrev(&upper, "TABULATE", 3) {
        return Some(b'\t');
    }
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        return u8::from_str_radix(s, 16).ok();
    }
    if s.len() == 1 {
        return Some(s.as_bytes()[0]);
    }
    None
}

/// xrange: xorc | xorc-xorc | xorc.number (1.1.11 p.197).
pub fn parse_xrange(s: &str) -> Result<Option<ByteSet>, String> {
    if let Some((left, right)) = split_once_unescaped(s, '-') {
        if !left.is_empty() && !right.is_empty() {
            let from = parse_xorc(left)?;
            let to = parse_xorc(right)?;
            let mut set = ByteSet::new();
            set.insert_inclusive_wrap(from, to);
            return Ok(Some(set));
        }
    }
    if let Some((left, right)) = split_once_unescaped(s, '.') {
        if !left.is_empty() && !right.is_empty() {
            let from = parse_xorc(left)?;
            let count = parse_positive_usize(right, "xrange count")?;
            let mut set = ByteSet::new();
            set.insert_count_wrap(from, count as u32);
            return Ok(Some(set));
        }
    }
    if let Some(b) = try_xorc(s) {
        let mut set = ByteSet::new();
        set.insert(b);
        return Ok(Some(set));
    }
    Ok(None)
}

/// Ordered bytes of an xrange (`xorc`, `xorc-xorc`, `xorc.number`).
///
/// Wrap-around ranges keep sequence order (`fe-01` → FE, FF, 00, 01). Used by
/// XLATE from/to pairs (1.1.11 p.604); [`parse_xrange`] returns a set and
/// cannot preserve that order.
pub fn expand_xrange(s: &str) -> Result<Option<Vec<u8>>, String> {
    if let Some((left, right)) = split_once_unescaped(s, '-') {
        if !left.is_empty() && !right.is_empty() {
            let from = parse_xorc(left)?;
            let to = parse_xorc(right)?;
            return Ok(Some(expand_inclusive_wrap(from, to)));
        }
    }
    if let Some((left, right)) = split_once_unescaped(s, '.') {
        if !left.is_empty() && !right.is_empty() {
            let from = parse_xorc(left)?;
            let count = parse_positive_usize(right, "xrange count")?;
            return Ok(Some(expand_count_wrap(from, count as u32)));
        }
    }
    if let Some(b) = try_xorc(s) {
        return Ok(Some(vec![b]));
    }
    Ok(None)
}

fn expand_inclusive_wrap(from: u8, to: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut b = from;
    loop {
        out.push(b);
        if b == to {
            break;
        }
        b = b.wrapping_add(1);
    }
    out
}

fn expand_count_wrap(from: u8, count: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(count as usize);
    let mut b = from;
    for _ in 0..count {
        out.push(b);
        b = b.wrapping_add(1);
    }
    out
}

pub fn parse_positive_usize(s: &str, what: &str) -> Result<usize, String> {
    let n = s
        .parse::<usize>()
        .map_err(|_| format!("{what} requires a positive number, got {s}"))?;
    if n == 0 {
        return Err(format!("{what} must be positive"));
    }
    Ok(n)
}

fn split_once_unescaped(s: &str, sep: char) -> Option<(&str, &str)> {
    let i = s.find(sep)?;
    Some((&s[..i], &s[i + sep.len_utf8()..]))
}

pub fn decode_target_dstring(s: &str) -> Result<Vec<u8>, String> {
    if !pipe_parser::is_dstring(s) {
        return Err("STRING/ANYOF requires a delimited string".into());
    }
    pipe_parser::decode_dstring(s)
}

/// One-character dstring (keyword optional) or an xrange word.
pub fn parse_bare_target(s: &str, anycase: bool) -> Result<Option<Target>, String> {
    if pipe_parser::is_dstring(s) {
        let inner = pipe_parser::decode_dstring(s)?;
        if inner.len() == 1 {
            return Ok(Some(Target::Bytes(ByteSet::from_bytes(inner, anycase))));
        }
        return Err("multi-character delimited string requires STRING or ANYOF keyword".into());
    }
    if let Some(mut set) = parse_xrange(s)? {
        if anycase {
            set = set.with_anycase();
        }
        return Ok(Some(Target::Bytes(set)));
    }
    Ok(None)
}

pub fn string_target(s: &str) -> Result<Target, String> {
    Ok(Target::String(decode_target_dstring(s)?))
}

pub fn anyof_target(s: &str, anycase: bool) -> Result<Target, String> {
    Ok(Target::Bytes(ByteSet::from_bytes(
        decode_target_dstring(s)?,
        anycase,
    )))
}

/// First match at or after `start`. NOT with a string uses match length 1 (p.59).
pub fn find_match(
    bytes: &[u8],
    start: usize,
    target: &Target,
    not: bool,
    anycase: bool,
) -> Option<(usize, usize)> {
    match target {
        Target::Blank => {
            for (i, &b) in bytes.iter().enumerate().skip(start) {
                let is_blank = b == b' ';
                if is_blank != not {
                    return Some((i, 1));
                }
            }
            None
        }
        Target::Bytes(set) => {
            for (i, &b) in bytes.iter().enumerate().skip(start) {
                if set.contains(b) != not {
                    return Some((i, 1));
                }
            }
            None
        }
        Target::String(needle) => {
            if needle.is_empty() {
                return None;
            }
            if not {
                // Skip whole occurrences; a mismatch (including a tail shorter
                // than the needle) has length 1 (1.1.11 p.59).
                let mut i = start;
                while i < bytes.len() {
                    if i + needle.len() <= bytes.len() {
                        let window = &bytes[i..i + needle.len()];
                        let matched = if anycase {
                            window.eq_ignore_ascii_case(needle)
                        } else {
                            window == needle.as_slice()
                        };
                        if matched {
                            i += needle.len();
                            continue;
                        }
                    }
                    return Some((i, 1));
                }
                return None;
            }
            let last = bytes.len().saturating_sub(needle.len());
            if start > last {
                return None;
            }
            for i in start..=last {
                let window = &bytes[i..i + needle.len()];
                let matched = if anycase {
                    window.eq_ignore_ascii_case(needle)
                } else {
                    window == needle.as_slice()
                };
                if matched {
                    return Some((i, needle.len()));
                }
            }
            None
        }
    }
}

/// Cut/split index. `n AFTER` ≡ `m BEFORE` with `m = -n - length(target)`.
pub fn cut_index(
    placement: Placement,
    match_start: usize,
    match_len: usize,
    rec_len: usize,
) -> usize {
    let raw = match placement {
        Placement::At => match_start as i64,
        Placement::Before(n) => match_start as i64 - i64::from(n),
        Placement::After(n) => match_start as i64 + match_len as i64 + i64::from(n),
    };
    if raw < 0 {
        0
    } else if raw as usize > rec_len {
        rec_len
    } else {
        raw as usize
    }
}

/// Book snumber (p.195): optional leading hyphen, no plus sign.
pub fn is_snumber_token(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

pub fn parse_snumber(s: &str) -> Result<i32, String> {
    if !is_snumber_token(s) {
        return Err(format!("invalid snumber {s}"));
    }
    s.parse::<i32>()
        .map_err(|_| format!("snumber {s} is out of range"))
}

/// Unsigned column number (0 allowed; max 2**31-1 per p.195).
pub fn parse_column_number(s: &str) -> Result<Option<u32>, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let n = s
        .parse::<i64>()
        .map_err(|_| format!("column number {s} is out of range"))?;
    if n < 0 || n > i32::MAX as i64 {
        return Err(format!(
            "column number must be 0 through {}, got {s}",
            i32::MAX
        ));
    }
    Ok(Some(n as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_wrap_keeps_order() {
        assert_eq!(
            expand_xrange("fe-01").unwrap(),
            Some(vec![0xfe, 0xff, 0x00, 0x01])
        );
        assert_eq!(expand_xrange("00-02").unwrap(), Some(vec![0, 1, 2]));
        assert_eq!(expand_xrange("a.3").unwrap(), Some(vec![b'a', b'b', b'c']));
        assert_eq!(expand_xrange("blank").unwrap(), Some(vec![b' ']));
    }
}
