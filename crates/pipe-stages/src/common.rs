//! Shared operand helpers used by built-in stages.

use std::cmp::Ordering;

use pipe_core::{Record, StageCtx, StreamId};

/// IBM abbreviation: `s` matches `full` when `min_len ≤ len(s) ≤ len(full)`.
pub fn matches_abbrev(s: &str, full: &str, min_len: usize) -> bool {
    s.len() >= min_len && s.len() <= full.len() && full.starts_with(s)
}

/// Compare two key fields (LOOKUP / UNIQUE / PICK / COLLATE / MERGE).
///
/// Portable ASCII byte order. The book compares with the IBM System/360
/// (EBCDIC) collating sequence unless ANYCASE is specified.
pub fn compare_keys(a: &[u8], b: &[u8], pad: Option<u8>, anycase: bool) -> Ordering {
    let mut left = a.to_vec();
    let mut right = b.to_vec();
    if anycase {
        for x in &mut left {
            *x = x.to_ascii_uppercase();
        }
        for x in &mut right {
            *x = x.to_ascii_uppercase();
        }
    }
    if let Some(p) = pad {
        if left.len() < right.len() {
            left.resize(right.len(), p);
        } else if right.len() < left.len() {
            right.resize(left.len(), p);
        }
        left.cmp(&right)
    } else {
        let n = left.len().min(right.len());
        match left[..n].cmp(&right[..n]) {
            Ordering::Equal => left.len().cmp(&right.len()),
            o => o,
        }
    }
}

/// Wait until `stream` has a record or is at end-of-file, then clone the slot.
///
/// Used by SYNCHRONISE, COLLATE, MERGE, DEAL, and GATHER so a stage can
/// write before `read` consumes (IBM “strictly does not delay”).
pub fn wait_peek(ctx: &mut StageCtx, stream: StreamId) -> Option<Record> {
    ctx.select_input(stream);
    loop {
        if ctx.input_record_ready(stream) {
            return ctx.peek();
        }
        if ctx.input_at_eof(stream) {
            return None;
        }
        if ctx.wait_any_input(&[stream]).is_err() {
            return None;
        }
    }
}

/// Parse a numeric stream identifier (FANIN / DEAL / GATHER).
///
/// Named (non-numeric) stream identifiers are not implemented.
pub fn parse_numeric_stream_id(word: &str) -> Result<StreamId, String> {
    let t = word.trim();
    if let Ok(n) = t.parse::<u32>() {
        Ok(StreamId(n))
    } else {
        Err(format!("stream identifier {word} is not implemented"))
    }
}

/// First blank-delimited word of a record (DEAL SECONDARY, p.314).
pub fn first_word(rec: &[u8]) -> &[u8] {
    let start = rec
        .iter()
        .position(|&b| !b.is_ascii_whitespace())
        .unwrap_or(rec.len());
    let rest = &rec[start..];
    let end = rest
        .iter()
        .position(|&b| b.is_ascii_whitespace())
        .unwrap_or(rest.len());
    &rest[..end]
}

/// ANYCASE and the documented synonyms (1.1.11 notes on locate/change/strip).
///
/// The usual railroad is `ANYcase` (min 3): `any` is accepted, `an` is not.
/// Stages whose diagram is `ANYCase` and/or that also have `ANYof` (min 3)
/// must use [`is_anycase_min4`] so `ANY` is ANYOF (STRIP/SPLIT/CHOP/JOINCONT
/// notes; LOCATE has `ANYcase` plus `ANYof`).
pub fn is_anycase(s: &str) -> bool {
    anycase_at(s, 3)
}

/// ANYCASE when `ANYof` shares the three-character prefix, or the railroad is
/// `ANYCase` (min 4). `CASEANY` / `IGNORECASE` / `CASEIGNORE` stay whole words.
pub fn is_anycase_min4(s: &str) -> bool {
    anycase_at(s, 4)
}

fn anycase_at(s: &str, min_len: usize) -> bool {
    let u = s.to_ascii_uppercase();
    matches_abbrev(&u, "ANYCASE", min_len)
        || u == "CASEANY"
        || u == "IGNORECASE"
        || u == "CASEIGNORE"
}

/// Strip a matching pair of non-alphanumeric delimiters (pair form only).
pub fn strip_delim(s: &str) -> &str {
    if s.len() >= 2 {
        let mut chars = s.chars();
        let first = chars.next().unwrap();
        let last = chars.next_back().unwrap();
        if first == last && !first.is_alphanumeric() && first != '-' && first != '.' {
            let start = first.len_utf8();
            let end = s.len() - last.len_utf8();
            if end >= start {
                return &s[start..end];
            }
        }
    }
    s
}

pub fn is_delimited(s: &str) -> bool {
    pipe_parser::is_dstring(s)
}

/// Decode a `delimitedString` (1.1.11 p.194): pair, hex (`x…`/`h…`), or binary (`b…`).
pub fn decode_dstring(s: &str) -> Result<Vec<u8>, String> {
    pipe_parser::decode_dstring(s)
}

/// UTF-8 view of a decoded dstring; non-UTF-8 bytes are kept as lossy text.
pub fn decode_dstring_text(s: &str) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&decode_dstring(s)?).into_owned())
}

/// Copy the selected input to the selected output until EOF or a write fails.
pub fn copy_selected(ctx: &mut pipe_core::StageCtx) {
    while let Ok(Some(r)) = ctx.read() {
        if ctx.write(r).is_err() {
            break;
        }
    }
}

/// Write one record, then copy remaining primary input (LITERAL / STRLITERAL PREFACE).
pub fn write_then_copy(ctx: &mut pipe_core::StageCtx, rec: pipe_core::Record) {
    use pipe_core::StreamId;
    if !ctx.output_connected(StreamId::PRIMARY) {
        return;
    }
    ctx.select_output(StreamId::PRIMARY);
    if ctx.write(rec).is_err() {
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    copy_selected(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anycase_railroad_is_min_three() {
        assert!(is_anycase("ANYCASE"));
        assert!(is_anycase("any"));
        assert!(is_anycase("ANYC"));
        assert!(!is_anycase("AN"));
        assert!(is_anycase("CASEANY"));
        assert!(is_anycase("IGNORECASE"));
        assert!(is_anycase("CASEIGNORE"));
    }

    #[test]
    fn anycase_min4_rejects_any_so_anyof_wins() {
        assert!(is_anycase_min4("ANYCASE"));
        assert!(is_anycase_min4("ANYC"));
        assert!(!is_anycase_min4("ANY"));
        assert!(!is_anycase_min4("AN"));
        assert!(is_anycase_min4("CASEANY"));
    }

    #[test]
    fn compare_keys_pad_and_anycase() {
        assert_eq!(compare_keys(b"ab", b"ab", None, false), Ordering::Equal);
        assert_eq!(compare_keys(b"ab", b"abc", None, false), Ordering::Less);
        assert_eq!(
            compare_keys(b"ab", b"ab ", Some(b' '), false),
            Ordering::Equal
        );
        assert_eq!(compare_keys(b"ab", b"AB", None, true), Ordering::Equal);
        assert_eq!(compare_keys(b"A", b"a", None, false), Ordering::Less);
    }

    #[test]
    fn first_word_skips_leading_blanks() {
        assert_eq!(first_word(b"  2 rest"), b"2");
        assert_eq!(first_word(b""), b"");
        assert_eq!(first_word(b"0"), b"0");
    }
}
