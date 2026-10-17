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
        "DISK" => disk_operands(raw).into_iter().map(StageArg::Word).collect(),
        "COMMAND" => word_operands(raw).into_iter().map(StageArg::Word).collect(),
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

/// Portable DISK: optional `OUTPUT`/`APPEND`, then the rest is the path.
///
/// Unix names such as `/tmp/x.txt` are one path, not a delimited `/tmp/`.
pub fn disk_operands(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }
    let mut parts = raw.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("");
    if first.eq_ignore_ascii_case("OUTPUT") || first.eq_ignore_ascii_case("APPEND") {
        let path = parts.next().map(str::trim).unwrap_or("").to_string();
        vec![first.to_string(), path]
    } else {
        vec![raw.to_string()]
    }
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
        if c == '"' || c == '\'' {
            let quote = c;
            i += quote.len_utf8();
            let start = i;
            while i < raw.len() {
                let ch = raw[i..].chars().next().unwrap();
                if ch == quote {
                    break;
                }
                i += ch.len_utf8();
            }
            out.push(StageArg::Quoted(raw[start..i].to_string()));
            if i < raw.len() {
                i += quote.len_utf8();
            }
            continue;
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
            vec!["OUTPUT".to_string(), "/tmp/out.txt".to_string()]
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
    }

    #[test]
    fn dstring_may_contain_words() {
        let t = scan_operands("/foo bar/");
        assert_eq!(t, vec![StageArg::Delimited("/foo bar/".into())]);
        assert_eq!(t[0].inner(), "foo bar");
    }
}
