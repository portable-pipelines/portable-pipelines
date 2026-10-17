//! Shared operand helpers used by built-in stages.

/// IBM abbreviation: `s` matches `full` when `min_len ≤ len(s) ≤ len(full)`.
pub fn matches_abbrev(s: &str, full: &str, min_len: usize) -> bool {
    s.len() >= min_len && s.len() <= full.len() && full.starts_with(s)
}

/// ANYCASE and the documented synonyms (1.1.11 notes on locate/change/strip).
pub fn is_anycase(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    matches_abbrev(&u, "ANYCASE", 4)
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
