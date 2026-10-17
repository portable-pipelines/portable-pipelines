//! SPLIT — Author's Edition 1.1.11, pp. 505–506.
//!
//! Syntax (fixed order):
//! `[ANYCase] [MINimum number] [AT | [snumber] BEFORE | [snumber] AFTER] [NOT] [BLANK | target [number]]`
//!
//! `target` is an `xrange`, `STRing dstring`, or `ANYof dstring`.

use pipe_core::{Record, StageCtx};

pub fn split(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_split(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("SPLIT: {e}"));
                return;
            }
        };

        while let Ok(Some(r)) = ctx.read() {
            // Note 1: copy null input records; never generate null records.
            if r.is_empty() {
                if ctx.write(Record::empty()).is_err() {
                    return;
                }
                continue;
            }

            for piece in apply_split(r.as_bytes(), &spec) {
                if piece.is_empty() {
                    continue;
                }
                if ctx.write(Record::from(piece)).is_err() {
                    return;
                }
            }
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Placement {
    At,
    Before(i32),
    After(i32),
}

#[derive(Clone, Debug)]
enum Target {
    Blank,
    Bytes(ByteSet),
    String(Vec<u8>),
}

#[derive(Clone, Debug)]
struct ByteSet {
    bits: [u64; 4],
}

impl ByteSet {
    fn new() -> Self {
        Self { bits: [0; 4] }
    }

    fn insert(&mut self, b: u8) {
        let i = (b >> 6) as usize;
        self.bits[i] |= 1u64 << (b & 63);
    }

    fn contains(&self, b: u8) -> bool {
        let i = (b >> 6) as usize;
        self.bits[i] & (1u64 << (b & 63)) != 0
    }

    fn insert_inclusive_wrap(&mut self, from: u8, to: u8) {
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

    fn insert_count_wrap(&mut self, from: u8, count: u32) {
        let mut b = from;
        for _ in 0..count {
            self.insert(b);
            b = b.wrapping_add(1);
        }
    }

    fn with_anycase(mut self) -> Self {
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
}

struct SplitSpec {
    anycase: bool,
    minimum: usize,
    placement: Placement,
    not: bool,
    target: Target,
    count: Option<u64>,
}

fn parse_split(args: &[String]) -> Result<SplitSpec, String> {
    let mut i = 0;

    // [ANYCase] — min 4; CASEANY / IGNORECASE / CASEIGNORE are synonyms.
    let mut anycase = false;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if matches_abbrev(&upper, "ANYCASE", 4)
            || upper == "CASEANY"
            || upper == "IGNORECASE"
            || upper == "CASEIGNORE"
        {
            anycase = true;
            i += 1;
        }
    }

    // [MINimum number]
    let mut minimum = 0usize;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if matches_abbrev(&upper, "MINIMUM", 3) {
            i += 1;
            if i >= args.len() {
                return Err("MINIMUM requires a number".into());
            }
            let n = parse_positive_usize(&args[i], "MINIMUM")?;
            minimum = n;
            i += 1;
        }
    }

    // [AT | [snumber] BEFORE | [snumber] AFTER]
    // A number is an snumber only when the next word is BEFORE or AFTER.
    let mut placement = Placement::At;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "AT" {
            placement = Placement::At;
            i += 1;
        } else if upper == "BEFORE" {
            placement = Placement::Before(0);
            i += 1;
        } else if upper == "AFTER" {
            placement = Placement::After(0);
            i += 1;
        } else if let Ok(offset) = args[i].parse::<i32>() {
            if i + 1 < args.len() {
                let next = args[i + 1].to_ascii_uppercase();
                if next == "BEFORE" {
                    placement = Placement::Before(offset);
                    i += 2;
                } else if next == "AFTER" {
                    placement = Placement::After(offset);
                    i += 2;
                }
            }
        }
    }

    // [NOT]
    let mut not = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("NOT") {
        not = true;
        i += 1;
    }

    // [BLANK | target [number]]
    let mut target = None;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "BLANK" || upper == "SPACE" {
            target = Some(Target::Blank);
            i += 1;
        } else if matches_abbrev(&upper, "STRING", 3) {
            i += 1;
            if i >= args.len() {
                return Err("STRING requires a delimited string".into());
            }
            target = Some(Target::String(decode_target_dstring(&args[i])?));
            i += 1;
        } else if matches_abbrev(&upper, "ANYOF", 3) {
            i += 1;
            if i >= args.len() {
                return Err("ANYOF requires a delimited string".into());
            }
            let mut set = ByteSet::new();
            for b in decode_target_dstring(&args[i])? {
                set.insert(b);
            }
            if anycase {
                set = set.with_anycase();
            }
            target = Some(Target::Bytes(set));
            i += 1;
        } else {
            match parse_target_word(&args[i], anycase)? {
                Some(t) => {
                    target = Some(t);
                    i += 1;
                }
                None => {
                    if args[i].parse::<u64>().is_ok() {
                        return Err(
                            "count is only valid after a target (for example: SPLIT BLANK 2)"
                                .into(),
                        );
                    }
                    return Err(format!("unexpected operand {}", args[i]));
                }
            }
        }
    }

    // Count is only valid after a target (including explicit BLANK).
    let mut count = None;
    if i < args.len() {
        if target.is_none() {
            return Err(format!(
                "count is only valid after a target, not before placement (got {})",
                args[i]
            ));
        }
        let n = args[i].parse::<u64>().map_err(|_| {
            let u = args[i].to_ascii_uppercase();
            if u == "BEFORE" || u == "AFTER" || u == "AT" {
                format!("{u} must appear before the target")
            } else {
                format!("unexpected operand {}", args[i])
            }
        })?;
        if n == 0 {
            return Err("count must be at least 1".into());
        }
        count = Some(n);
        i += 1;
    }

    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(SplitSpec {
        anycase,
        minimum,
        placement,
        not,
        target: target.unwrap_or(Target::Blank),
        count,
    })
}

fn parse_positive_usize(s: &str, what: &str) -> Result<usize, String> {
    let n = s
        .parse::<usize>()
        .map_err(|_| format!("{what} requires a positive number, got {s}"))?;
    if n == 0 {
        return Err(format!("{what} must be positive"));
    }
    Ok(n)
}

fn matches_abbrev(s: &str, full: &str, min_len: usize) -> bool {
    s.len() >= min_len && s.len() <= full.len() && full.starts_with(s)
}

fn decode_target_dstring(s: &str) -> Result<Vec<u8>, String> {
    if !pipe_parser::is_dstring(s) {
        return Err("STRING/ANYOF requires a delimited string".into());
    }
    pipe_parser::decode_dstring(s)
}

fn parse_target_word(s: &str, anycase: bool) -> Result<Option<Target>, String> {
    if pipe_parser::is_dstring(s) {
        let inner = pipe_parser::decode_dstring(s)?;
        if inner.len() == 1 {
            let mut set = ByteSet::new();
            set.insert(inner[0]);
            if anycase {
                set = set.with_anycase();
            }
            return Ok(Some(Target::Bytes(set)));
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

/// xrange: xorc | xorc-xorc | xorc.number (syntactic variables, 1.1.11).
fn parse_xrange(s: &str) -> Result<Option<ByteSet>, String> {
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

fn split_once_unescaped(s: &str, sep: char) -> Option<(&str, &str)> {
    let i = s.find(sep)?;
    Some((&s[..i], &s[i + sep.len_utf8()..]))
}

/// xorc: one-character word, two hex digits, BLANK/SPACE, or TABULATE.
fn parse_xorc(s: &str) -> Result<u8, String> {
    try_xorc(s).ok_or_else(|| format!("invalid xorc {s}"))
}

fn try_xorc(s: &str) -> Option<u8> {
    let upper = s.to_ascii_uppercase();
    if upper == "BLANK" || upper == "SPACE" {
        return Some(b' ');
    }
    if matches_abbrev(&upper, "TABULATE", 3) {
        // Book xorc TABULATE is EBCDIC X'05'; portable ASCII uses HT (0x09).
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


fn apply_split(bytes: &[u8], spec: &SplitSpec) -> Vec<Vec<u8>> {
    let mut pieces = Vec::new();
    let mut prev = 0usize;
    let mut search = spec.minimum.min(bytes.len());
    let mut made = 0u64;
    let max = spec.count.unwrap_or(u64::MAX);

    while search < bytes.len() && made < max {
        let Some((ms, mlen)) = find_match(bytes, search, spec) else {
            break;
        };
        let sp = split_index(spec.placement, ms, mlen, bytes.len());

        if sp > prev {
            emit(&mut pieces, &bytes[prev..sp]);
        }

        match spec.placement {
            Placement::At => {
                prev = ms.saturating_add(mlen).min(bytes.len());
            }
            Placement::Before(_) | Placement::After(_) => {
                if sp > prev {
                    prev = sp.min(bytes.len());
                }
            }
        }

        search = ms.saturating_add(mlen).max(prev);
        if spec.minimum > 0 {
            search = search.max(prev.saturating_add(spec.minimum));
        }
        if search <= prev && spec.placement == Placement::At {
            search = prev;
        }
        made += 1;
    }

    if prev < bytes.len() {
        emit(&mut pieces, &bytes[prev..]);
    }
    pieces
}

fn emit(pieces: &mut Vec<Vec<u8>>, slice: &[u8]) {
    if !slice.is_empty() {
        pieces.push(slice.to_vec());
    }
}

fn split_index(
    placement: Placement,
    match_start: usize,
    match_len: usize,
    rec_len: usize,
) -> usize {
    // n AFTER target ≡ m BEFORE target with m = -n - length(target).
    // BEFORE subtracts the snumber from the start of the match; AFTER adds it
    // after the match.
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

fn find_match(bytes: &[u8], start: usize, spec: &SplitSpec) -> Option<(usize, usize)> {
    match &spec.target {
        Target::Blank => {
            for (i, &b) in bytes.iter().enumerate().skip(start) {
                let is_blank = b == b' ';
                if is_blank != spec.not {
                    return Some((i, 1));
                }
            }
            None
        }
        Target::Bytes(set) => {
            for (i, &b) in bytes.iter().enumerate().skip(start) {
                if set.contains(b) != spec.not {
                    return Some((i, 1));
                }
            }
            None
        }
        Target::String(needle) => {
            if needle.is_empty() {
                return None;
            }
            let last = bytes.len().saturating_sub(needle.len());
            if start > last {
                return None;
            }
            for i in start..=last {
                let window = &bytes[i..i + needle.len()];
                let matched = if spec.anycase {
                    window.eq_ignore_ascii_case(needle)
                } else {
                    window == needle.as_slice()
                };
                if spec.not {
                    if !matched {
                        return Some((i, 1));
                    }
                } else if matched {
                    return Some((i, needle.len()));
                }
            }
            None
        }
    }
}
