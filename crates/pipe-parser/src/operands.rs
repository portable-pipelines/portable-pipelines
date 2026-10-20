//! Per-stage operand scanning.
//!
//! The pipeline scanner only finds verbs, `|`, labels, and ENDCHAR. Each stage
//! then rescans its raw operand text with the rule that matches its syntax:
//! IBM filters use delimited strings (blanks preserved); COMMAND keeps
//! slash-flags as words; DISK treats the remainder as a file name.

use crate::scanner::is_string_delimiter;
use crate::spec::StageArg;

/// Choose a scanner from the stage verb.
pub fn stage_args(verb: &str, raw: &str) -> Vec<StageArg> {
    match verb.to_ascii_uppercase().as_str() {
        "DISK" | "<" | ">" | ">>" => file_operands(raw).into_iter().map(StageArg::Word).collect(),
        "COMMAND" => word_operands(raw).into_iter().map(StageArg::Word).collect(),
        // 1.1.11 p.337: `word [string]`. The remainder is the nested driver's
        // argument string and is re-parsed with that verb's own scanner, so a
        // Unix path `/tmp/x` is not split as the delimited string `/tmp/`.
        "EOFBACK" => word_then_remainder(raw)
            .into_iter()
            .map(StageArg::Word)
            .collect(),
        // 1.1.11 p.412: the string starts after exactly one blank; the rest is
        // the record, including further blanks. FIND / NFIND / NOTFIND use the
        // same remainder rule (pp.349, 438); FRLABEL / FROMLABEL / TOLABEL /
        // WHILELABEL likewise (pp.353, 559, 596). The string is not a dstring.
        "LITERAL" | "FIND" | "NFIND" | "NOTFIND" | "FRLABEL" | "FROMLABEL" | "TOLABEL"
        | "WHILELABEL" => {
            let s = strip_one_blank(raw);
            if s.is_empty() {
                Vec::new()
            } else {
                vec![StageArg::Word(s.to_string())]
            }
        }
        _ => scan_operands(raw),
    }
}

/// Choose a scanner from the stage verb and return string operands.
pub fn operands_for_stage(verb: &str, raw: &str) -> Vec<String> {
    stage_args(verb, raw)
        .into_iter()
        .map(|a| a.as_str().to_string())
        .collect()
}

/// Whitespace-separated words. `/s` and `/b` stay separate flags.
pub fn word_operands(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

/// Portable file-stage operand: the remainder is one host path.
///
/// Unix names such as `/tmp/x.txt` are one path, not a delimited `/tmp/`.
/// Used by `DISK`, `<`, `>`, and `>>` (1.1.11 pp.230, 237, 245, 328).
pub fn file_operands(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    vec![strip_outer_quotes(raw)]
}

/// Alias used by older tests; same as [`file_operands`].
pub fn disk_operands(raw: &str) -> Vec<String> {
    file_operands(raw)
}

fn strip_outer_quotes(s: &str) -> String {
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'"' && *bytes.last().unwrap() == b'"')
            || (bytes[0] == b'\'' && *bytes.last().unwrap() == b'\'')
        {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// IBM filter operands: words plus delimited strings that may contain blanks.
///
/// The exact characters between the delimiters are kept (including multiple
/// blanks). `/ /` is the blank insert; `/foo bar/` is one string. An opener
/// with no closer (`/s`) is a Word.
pub fn scan_operands(raw: &str) -> Vec<StageArg> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let c = match raw[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        // IBM delimitedString (p.194): any non-alphanumeric delimiter, including
        // `"` / `'`. Keep the quotes so `"0800"` is a dstring, not column 800
        // (PICK 1.1.11 p.456). An opener with no closer is a Word.
        if c == '"' || c == '\'' {
            if let Some(end) = delimited_end(raw, i, c) {
                out.push(StageArg::Delimited(raw[i..end + c.len_utf8()].to_string()));
                i = end + c.len_utf8();
                continue;
            }
        }
        if is_string_delimiter(c) {
            if let Some(end) = delimited_end(raw, i, c) {
                out.push(StageArg::Delimited(raw[i..end + c.len_utf8()].to_string()));
                i = end + c.len_utf8();
                continue;
            }
        }
        let start = i;
        while i < raw.len() {
            let ch = raw[i..].chars().next().unwrap();
            if ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        out.push(StageArg::Word(raw[start..i].to_string()));
    }
    out
}

/// First token is `word`; the rest of the operand text is one remainder string.
fn word_then_remainder(raw: &str) -> Vec<String> {
    let raw = raw.trim_start();
    if raw.is_empty() {
        return Vec::new();
    }
    match raw.split_once(char::is_whitespace) {
        None => vec![raw.to_string()],
        Some((word, rest)) => {
            let rest = rest.trim();
            if rest.is_empty() {
                vec![word.to_string()]
            } else {
                vec![word.to_string(), rest.to_string()]
            }
        }
    }
}

/// Drop the blank that separates the verb from its argument string (p.208).
pub fn strip_one_blank(raw: &str) -> &str {
    raw.strip_prefix(' ').unwrap_or(raw)
}

/// Decode a `delimitedString` (Author's Edition 1.1.11 p.194).
///
/// A delimited string is written between two occurrences of a delimiter
/// (`/abc/`; two adjacent delimiters are the null string), as a hexadecimal
/// literal (`X`/`H` plus an even number of hex digits, e.g. `xf1f2f3`), or as
/// a binary literal (`B` plus 0/1 bits in a multiple of eight, e.g. `b11000001`).
/// Hex and binary decode to raw bytes (the book's `xf1f2f3` → `123` example is
/// EBCDIC; portable ASCII uses `x313233` for the same characters).
pub fn decode_dstring(s: &str) -> Result<Vec<u8>, String> {
    if let Some(bytes) = try_hex_or_bin_literal(s)? {
        return Ok(bytes);
    }
    if let Some(inner) = pair_delimited_inner(s) {
        return Ok(inner.as_bytes().to_vec());
    }
    Err(format!("not a delimited string: {s}"))
}

/// True when `s` is a pair-delimited string or a hex/binary literal (p.194).
pub fn is_dstring(s: &str) -> bool {
    match try_hex_or_bin_literal(s) {
        Ok(Some(_)) => true,
        Ok(None) => pair_delimited_inner(s).is_some(),
        Err(_) => true,
    }
}

fn try_hex_or_bin_literal(s: &str) -> Result<Option<Vec<u8>>, String> {
    if s.len() < 2 {
        return Ok(None);
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    let rest = &s[first.len_utf8()..];
    match first {
        'X' | 'x' | 'H' | 'h' => {
            if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_hexdigit()) {
                return Ok(None);
            }
            if rest.len() % 2 != 0 {
                return Err(
                    "hexadecimal literal must have an even number of digits (1.1.11 p.194)".into(),
                );
            }
            let mut out = Vec::with_capacity(rest.len() / 2);
            for chunk in rest.as_bytes().chunks(2) {
                let hex = std::str::from_utf8(chunk).unwrap();
                out.push(
                    u8::from_str_radix(hex, 16)
                        .map_err(|_| format!("invalid hexadecimal literal {s}"))?,
                );
            }
            Ok(Some(out))
        }
        'B' | 'b' => {
            if rest.is_empty() || !rest.chars().all(|c| c == '0' || c == '1') {
                return Ok(None);
            }
            if rest.len() % 8 != 0 {
                return Err(
                    "binary literal length must be a multiple of eight (1.1.11 p.194)".into(),
                );
            }
            let mut out = Vec::with_capacity(rest.len() / 8);
            for chunk in rest.as_bytes().chunks(8) {
                let mut b = 0u8;
                for &bit in chunk {
                    b = (b << 1) | (bit - b'0');
                }
                out.push(b);
            }
            Ok(Some(out))
        }
        _ => Ok(None),
    }
}

fn pair_delimited_inner(s: &str) -> Option<&str> {
    if s.len() < 2 {
        return None;
    }
    let mut chars = s.chars();
    let first = chars.next()?;
    let last = chars.next_back()?;
    if first == last
        && !first.is_whitespace()
        && !first.is_alphanumeric()
        && first != '-'
        && first != '.'
    {
        let start = first.len_utf8();
        let end = s.len() - last.len_utf8();
        if end >= start {
            return Some(&s[start..end]);
        }
    }
    None
}

fn delimited_end(raw: &str, start: usize, delim: char) -> Option<usize> {
    let mut i = start + delim.len_utf8();
    while i < raw.len() {
        let ch = raw[i..].chars().next().unwrap();
        if ch == delim {
            return Some(i);
        }
        i += ch.len_utf8();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_dstring_preserves_spaces() {
        let t = scan_operands("* / / 10");
        assert_eq!(
            t,
            vec![
                StageArg::Word("*".into()),
                StageArg::Delimited("/ /".into()),
                StageArg::Word("10".into()),
            ]
        );
        assert_eq!(t[1].inner(), " ");

        let t = scan_operands("* /  / 10");
        assert_eq!(t[1], StageArg::Delimited("/  /".into()));
        assert_eq!(t[1].inner(), "  ");
    }

    #[test]
    fn slash_flag_without_closer_is_a_word() {
        let t = scan_operands("/s /b");
        // IBM dstring: `/s /` closes at the second slash; COMMAND must not
        // use this scanner.
        assert_eq!(t[0], StageArg::Delimited("/s /".into()));
        assert_eq!(word_operands("/s /b"), vec!["/s", "/b"]);
    }

    #[test]
    fn disk_keeps_unix_path() {
        assert_eq!(
            disk_operands("/tmp/cms-pipelines-phase1-exit.txt"),
            vec!["/tmp/cms-pipelines-phase1-exit.txt".to_string()]
        );
        assert_eq!(
            disk_operands("OUTPUT /tmp/out.txt"),
            vec!["OUTPUT /tmp/out.txt".to_string()]
        );
        assert_eq!(
            operands_for_stage("DISK", "/tmp/x.txt"),
            vec!["/tmp/x.txt".to_string()]
        );
        assert_eq!(
            operands_for_stage("COMMAND", "dir /s /b"),
            vec!["dir".to_string(), "/s".to_string(), "/b".to_string()]
        );
        assert_eq!(
            operands_for_stage("JOIN", "* / / 10"),
            vec!["*".to_string(), "/ /".to_string(), "10".to_string()]
        );
        assert_eq!(
            operands_for_stage("<", " /tmp/in.txt"),
            vec!["/tmp/in.txt".to_string()]
        );
        assert_eq!(
            operands_for_stage(">", "/tmp/out.txt"),
            vec!["/tmp/out.txt".to_string()]
        );
        assert_eq!(
            operands_for_stage(">>", "/tmp/out.txt"),
            vec!["/tmp/out.txt".to_string()]
        );
        assert_eq!(
            operands_for_stage("EOFBACK", "> /tmp/eofback-out.txt"),
            vec![">".to_string(), "/tmp/eofback-out.txt".to_string()]
        );
        assert_eq!(
            operands_for_stage("EOFBACK", "hole"),
            vec!["hole".to_string()]
        );
        assert_eq!(
            operands_for_stage("EOFBACK", "locate /a/"),
            vec!["locate".to_string(), "/a/".to_string()]
        );
        // FIND / NFIND: remainder after one blank; blanks kept; not a dstring.
        assert_eq!(operands_for_stage("FIND", " a c"), vec!["a c".to_string()]);
        assert_eq!(
            operands_for_stage("FIND", " a c "),
            vec!["a c ".to_string()]
        );
        assert_eq!(operands_for_stage("FIND", "  a"), vec![" a".to_string()]);
        assert_eq!(operands_for_stage("FIND", ""), Vec::<String>::new());
        assert_eq!(
            operands_for_stage("FIND", " /abc/"),
            vec!["/abc/".to_string()]
        );
        assert_eq!(operands_for_stage("NFIND", " a c"), vec!["a c".to_string()]);
        assert_eq!(
            operands_for_stage("NOTFIND", " a c"),
            vec!["a c".to_string()]
        );
        assert_eq!(
            operands_for_stage("find", " ANYCASE abc"),
            vec!["ANYCASE abc".to_string()]
        );
        // FRLABEL / TOLABEL / WHILELABEL: same remainder rule (pp.353, 559, 596).
        assert_eq!(
            operands_for_stage("FRLABEL", " abc"),
            vec!["abc".to_string()]
        );
        assert_eq!(
            operands_for_stage("FRLABEL", "  abc"),
            vec![" abc".to_string()]
        );
        assert_eq!(
            operands_for_stage("TOLABEL", " .end"),
            vec![".end".to_string()]
        );
        assert_eq!(
            operands_for_stage("WHILELABEL", " *"),
            vec!["*".to_string()]
        );
        assert_eq!(
            operands_for_stage("FROMLABEL", " abc"),
            vec!["abc".to_string()]
        );
        assert_eq!(operands_for_stage("FRLABEL", ""), Vec::<String>::new());
        // STR forms use filter dstring scanning, not the remainder rule.
        assert_eq!(
            operands_for_stage("STRLITERAL", " APPEND /a b/"),
            vec!["APPEND".to_string(), "/a b/".to_string()]
        );
        assert_eq!(
            operands_for_stage("FRTARGET", " locate /abc/"),
            vec!["locate".to_string(), "/abc/".to_string()]
        );
        assert_eq!(
            operands_for_stage("TOTARGET", " find abc"),
            vec!["find".to_string(), "abc".to_string()]
        );
        assert_eq!(
            operands_for_stage("SPILL", " 12 OFFSET 4"),
            vec!["12".to_string(), "OFFSET".to_string(), "4".to_string()]
        );
        assert_eq!(
            operands_for_stage("STRFIND", " ANYCASE x02"),
            vec!["ANYCASE".to_string(), "x02".to_string()]
        );
        assert_eq!(
            operands_for_stage("STRFRLABEL", " /abc/"),
            vec!["/abc/".to_string()]
        );
        assert_eq!(
            operands_for_stage("STRWHILELABEL", " ANYCASE /ab/"),
            vec!["ANYCASE".to_string(), "/ab/".to_string()]
        );
    }

    #[test]
    fn dstring_may_contain_words() {
        let t = scan_operands("/foo bar/");
        assert_eq!(t, vec![StageArg::Delimited("/foo bar/".into())]);
        assert_eq!(t[0].inner(), "foo bar");
    }

    #[test]
    fn hex_and_binary_literals_are_dstrings() {
        // 1.1.11 p.194: X/H + even hex; B + bits in multiples of 8.
        assert_eq!(decode_dstring("x4142").unwrap(), b"AB");
        assert_eq!(decode_dstring("H6869").unwrap(), b"hi");
        assert_eq!(decode_dstring("b01100001").unwrap(), b"a");
        assert_eq!(decode_dstring("/abc/").unwrap(), b"abc");
        assert_eq!(decode_dstring(",,").unwrap(), b"");
        assert!(decode_dstring("x1").unwrap_err().contains("even"));
        assert!(decode_dstring("b01")
            .unwrap_err()
            .contains("multiple of eight"));
        assert!(is_dstring("xf1f2f3"));
        assert!(!is_dstring("BLANK"));
        let t = scan_operands("x4142 /zz/");
        assert_eq!(t[0], StageArg::Word("x4142".into()));
        assert_eq!(decode_dstring(t[0].as_str()).unwrap(), b"AB");
    }

    #[test]
    fn quoted_pair_is_a_dstring() {
        // PICK 1.1.11 p.456: `"0800"` is a delimitedString, not column 800.
        let t = scan_operands(r#"1.4 << "0800""#);
        assert_eq!(t[0], StageArg::Word("1.4".into()));
        assert_eq!(t[1], StageArg::Word("<<".into()));
        assert_eq!(t[2], StageArg::Delimited("\"0800\"".into()));
        assert_eq!(decode_dstring(t[2].as_str()).unwrap(), b"0800");
        assert_eq!(decode_dstring("'0800'").unwrap(), b"0800");
        assert!(is_dstring("\"0800\""));
    }
}
