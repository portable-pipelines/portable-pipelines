/// Lexical tokens for a PIPE specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Stage separator `|`
    StageSep,
    /// Pipeline end character (from ENDCHAR), e.g. `?`
    PipelineEnd,
    /// Label definition/reference like `L:` (name without colon)
    Label(String),
    /// Connector `*:` (IBM; bare `*` also accepted as a synonym)
    Connector,
    /// Bare word / stage name / operand fragment
    Word(String),
    /// Delimited string including the delimiter chars, e.g. `/foo/`
    Delimited(String),
    /// Quoted string content (quotes stripped)
    Quoted(String),
}

/// A token plus byte offsets into the original specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpannedToken {
    pub token: Token,
    pub start: usize,
    pub end: usize,
}

/// Scan a pipeline specification into tokens.
///
/// `endchar` is the character that separates multiple pipelines in a set
/// (CMS default is often `?` when specified via `(ENDCHAR ?)`).
pub fn scan(input: &str, endchar: Option<char>) -> Vec<Token> {
    scan_spanned(input, endchar)
        .into_iter()
        .map(|t| t.token)
        .collect()
}

/// Like [`scan`], with source spans so a stage can keep its raw operand text.
pub fn scan_spanned(input: &str, endchar: Option<char>) -> Vec<SpannedToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut byte_at = Vec::with_capacity(chars.len() + 1);
    let mut b = 0usize;
    for &c in &chars {
        byte_at.push(b);
        b += c.len_utf8();
    }
    byte_at.push(b);
    let mut i = 0;

    let push = |tokens: &mut Vec<SpannedToken>, token: Token, from: usize, to: usize| {
        tokens.push(SpannedToken {
            token,
            start: byte_at[from],
            end: byte_at[to],
        });
    };

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '|' {
            push(&mut tokens, Token::StageSep, i, i + 1);
            i += 1;
            continue;
        }
        if c == '*' {
            // IBM connectors: `*:`, `*.input:`, `*.output:`. Bare `*` alone is a
            // non-IBM synonym. Windows globs like `c:\*.*` or `*.json` must stay Words.
            if let Some(consumed) = connector_len(&chars, i, endchar) {
                push(&mut tokens, Token::Connector, i, i + consumed);
                i += consumed;
                continue;
            }
            // Fall through to Word (glob / path fragment).
        }
        if let Some(ec) = endchar {
            if c == ec {
                push(&mut tokens, Token::PipelineEnd, i, i + 1);
                i += 1;
                continue;
            }
        }
        if c == '"' || c == '\'' {
            let quote = c;
            let qstart = i;
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != quote {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            if i < chars.len() {
                i += 1; // closing quote
            }
            push(&mut tokens, Token::Quoted(s), qstart, i);
            continue;
        }
        // Delimited string only at a *token boundary* (after whitespace / `|`):
        // `locate /foo/` → delimited. `examples/upper.rexx` → single path word.
        //
        // Slash is special on Windows: `/s /b /a` are COMMAND flags, not one
        // string `/s /` plus leftovers. So `/…/` is delimited only when both
        // slashes sit in the same token (no whitespace between). Spaced
        // patterns use another delimiter (`%foo bar%`) or quotes.
        if is_string_delimiter(c) {
            let delim = c;
            let token_start = i;
            if let Some(end) = find_delimited_end(&chars, token_start, delim, endchar) {
                let inner: String = chars[token_start + 1..end].iter().collect();
                i = end + 1;
                push(
                    &mut tokens,
                    Token::Delimited(format!("{delim}{inner}{delim}")),
                    token_start,
                    i,
                );
                continue;
            }
            // Not a delimited pair — fall through to Word (e.g. `/s`).
            i = token_start;
        }

        // Word, label, or filesystem path (may contain `/`, `\`, or `*` globs)
        let start = i;
        while i < chars.len() {
            let ch = chars[i];
            if ch.is_whitespace() || ch == '|' || endchar == Some(ch) {
                break;
            }
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        if word.is_empty() {
            i += 1;
            continue;
        }
        if let Some(name) = word.strip_suffix(':') {
            // Windows drive letters (`C:\` / `C:/`) are path fragments, not labels.
            // Single-letter CMS labels like `L:` remain labels when not followed by a path sep.
            let single_letter =
                name.len() == 1 && name.chars().next().unwrap().is_ascii_alphabetic();
            let next_is_path = i < chars.len() && (chars[i] == '\\' || chars[i] == '/');
            let is_drive = single_letter && next_is_path;
            if !name.is_empty()
                && !is_drive
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '#')
            {
                push(
                    &mut tokens,
                    Token::Label(name.to_ascii_uppercase()),
                    start,
                    i,
                );
                continue;
            }
        }
        // Reassemble Windows paths that were split as `C:` + `\Users\...`
        if word.len() == 2
            && word.as_bytes()[1] == b':'
            && word.as_bytes()[0].is_ascii_alphabetic()
            && i < chars.len()
            && (chars[i] == '\\' || chars[i] == '/')
        {
            let rest_start = i;
            while i < chars.len() {
                let ch = chars[i];
                if ch.is_whitespace() || ch == '|' || endchar == Some(ch) {
                    break;
                }
                i += 1;
            }
            let rest: String = chars[rest_start..i].iter().collect();
            push(&mut tokens, Token::Word(format!("{word}{rest}")), start, i);
            continue;
        }
        push(&mut tokens, Token::Word(word), start, i);
    }

    tokens
}

/// Bytes to consume for a CALLPIPE connector at `i`, or `None` if not a connector.
fn connector_len(chars: &[char], i: usize, endchar: Option<char>) -> Option<usize> {
    let rest: String = chars[i..].iter().collect();
    let rest_upper = rest.to_ascii_uppercase();
    if rest_upper.starts_with("*.INPUT:") {
        return Some("*.input:".len());
    }
    if rest_upper.starts_with("*.OUTPUT:") {
        return Some("*.output:".len());
    }
    if rest.starts_with("*:") {
        return Some(2);
    }
    // Bare `*` only when it is the whole token (CMS synonym for `*:`).
    let next = chars.get(i + 1).copied();
    let alone = match next {
        None => true,
        Some(ch) => ch.is_whitespace() || ch == '|' || endchar == Some(ch),
    };
    if alone {
        Some(1)
    } else {
        None
    }
}

pub(crate) fn is_string_delimiter(c: char) -> bool {
    // `:` is not included — CMS labels (`L:`) and Windows drives (`C:/…`).
    // `/` is a string delimiter only when it begins a token (see above), so
    // paths like `examples/upper.rexx` stay a single Word.
    matches!(c, '/' | '%' | '$' | '#' | '!' | ';' | ',')
}

/// Index of the closing delimiter, or `None` if this is not a delimited string.
///
/// For `/`, the closer must appear before whitespace / `|` / end (same token).
/// Other delimiters may span spaces (`%foo bar%`) but still require a closer
/// before EOF (unclosed → not delimited).
fn find_delimited_end(
    chars: &[char],
    start: usize,
    delim: char,
    endchar: Option<char>,
) -> Option<usize> {
    let mut i = start + 1;
    while i < chars.len() {
        let ch = chars[i];
        if ch == delim {
            return Some(i);
        }
        if delim == '/' {
            if ch.is_whitespace() || ch == '|' || ch == '*' || endchar == Some(ch) {
                return None;
            }
        } else if ch == '|' || endchar == Some(ch) {
            // Stop stage-spanning unclosed non-slash delimiters.
            return None;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_simple_pipeline() {
        let t = scan("literal hello | console", None);
        assert_eq!(
            t,
            vec![
                Token::Word("literal".into()),
                Token::Word("hello".into()),
                Token::StageSep,
                Token::Word("console".into()),
            ]
        );
    }

    #[test]
    fn scans_label_and_delimited() {
        let t = scan("L: locate /foo/ | console", None);
        assert_eq!(t[0], Token::Label("L".into()));
        assert!(matches!(t[2], Token::Delimited(_)));
    }

    #[test]
    fn scans_rex_path_as_single_word() {
        let t = scan("literal hello | examples/upper.rexx | console", None);
        assert_eq!(
            t,
            vec![
                Token::Word("literal".into()),
                Token::Word("hello".into()),
                Token::StageSep,
                Token::Word("examples/upper.rexx".into()),
                Token::StageSep,
                Token::Word("console".into()),
            ]
        );
    }

    #[test]
    fn scans_ibm_callpipe_connectors() {
        let t = scan("*: | change /a/ /b/ | *:", None);
        assert_eq!(
            t,
            vec![
                Token::Connector,
                Token::StageSep,
                Token::Word("change".into()),
                Token::Delimited("/a/".into()),
                Token::Delimited("/b/".into()),
                Token::StageSep,
                Token::Connector,
            ]
        );
        let t2 = scan("*.input: | hole | *.output:", None);
        assert_eq!(t2[0], Token::Connector);
        assert_eq!(t2[t2.len() - 1], Token::Connector);
        let t3 = scan("* | hole | *", None);
        assert_eq!(t3[0], Token::Connector);
        assert_eq!(t3[t3.len() - 1], Token::Connector);
    }

    #[test]
    fn scans_windows_glob_stars_as_words() {
        let t = scan(
            r"command dir c:\*.* /s | locate /.json/ | count | console",
            None,
        );
        assert_eq!(
            t,
            vec![
                Token::Word("command".into()),
                Token::Word("dir".into()),
                Token::Word(r"c:\*.*".into()),
                Token::Word("/s".into()),
                Token::StageSep,
                Token::Word("locate".into()),
                Token::Delimited("/.json/".into()),
                Token::StageSep,
                Token::Word("count".into()),
                Token::StageSep,
                Token::Word("console".into()),
            ]
        );
        let t2 = scan("command dir *.json | console", None);
        assert_eq!(t2[2], Token::Word("*.json".into()));
    }

    #[test]
    fn scans_windows_slash_flag_as_word() {
        let t = scan("command dir /s | count | console", None);
        assert_eq!(
            t,
            vec![
                Token::Word("command".into()),
                Token::Word("dir".into()),
                Token::Word("/s".into()),
                Token::StageSep,
                Token::Word("count".into()),
                Token::StageSep,
                Token::Word("console".into()),
            ]
        );
    }

    #[test]
    fn scans_multiple_windows_slash_flags() {
        let t = scan("command dir /s /b /a | locate /ee/ | count | console", None);
        assert_eq!(
            t,
            vec![
                Token::Word("command".into()),
                Token::Word("dir".into()),
                Token::Word("/s".into()),
                Token::Word("/b".into()),
                Token::Word("/a".into()),
                Token::StageSep,
                Token::Word("locate".into()),
                Token::Delimited("/ee/".into()),
                Token::StageSep,
                Token::Word("count".into()),
                Token::StageSep,
                Token::Word("console".into()),
            ]
        );
    }

    #[test]
    fn closed_delimited_still_works() {
        let t = scan("locate /s/ | console", None);
        assert_eq!(
            t,
            vec![
                Token::Word("locate".into()),
                Token::Delimited("/s/".into()),
                Token::StageSep,
                Token::Word("console".into()),
            ]
        );
    }

    #[test]
    fn non_slash_delimiter_may_contain_spaces() {
        let t = scan("locate %ee ee% | console", None);
        assert_eq!(
            t,
            vec![
                Token::Word("locate".into()),
                Token::Delimited("%ee ee%".into()),
                Token::StageSep,
                Token::Word("console".into()),
            ]
        );
    }
}
