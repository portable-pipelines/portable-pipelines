/// Lexical tokens for a PIPE specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Stage separator `|`
    StageSep,
    /// Pipeline end character (from ENDCHAR), e.g. `?`
    PipelineEnd,
    /// Label definition/reference like `L:` (name without colon)
    Label(String),
    /// Connector `*`
    Connector,
    /// Bare word / stage name / operand fragment
    Word(String),
    /// Delimited string including the delimiter chars, e.g. `/foo/`
    Delimited(String),
    /// Quoted string content (quotes stripped)
    Quoted(String),
}

/// Scan a pipeline specification into tokens.
///
/// `endchar` is the character that separates multiple pipelines in a set
/// (CMS default is often `?` when specified via `(ENDCHAR ?)`).
pub fn scan(input: &str, endchar: Option<char>) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '|' {
            tokens.push(Token::StageSep);
            i += 1;
            continue;
        }
        if c == '*' {
            tokens.push(Token::Connector);
            i += 1;
            continue;
        }
        if let Some(ec) = endchar {
            if c == ec {
                tokens.push(Token::PipelineEnd);
                i += 1;
                continue;
            }
        }
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != quote {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            tokens.push(Token::Quoted(s));
            if i < chars.len() {
                i += 1; // closing quote
            }
            continue;
        }
        // Delimited string only at a *token boundary* (after whitespace / `|`):
        // `locate /foo/` → delimited. `examples/upper.rexx` → single path word.
        if is_string_delimiter(c) {
            let delim = c;
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != delim {
                i += 1;
            }
            let inner: String = chars[start..i].iter().collect();
            if i < chars.len() {
                i += 1;
            }
            tokens.push(Token::Delimited(format!("{delim}{inner}{delim}")));
            continue;
        }

        // Word, label, or filesystem path (may contain `/` or `\`)
        let start = i;
        while i < chars.len() {
            let ch = chars[i];
            if ch.is_whitespace() || ch == '|' || ch == '*' || endchar == Some(ch) {
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
            let single_letter = name.len() == 1 && name.chars().next().unwrap().is_ascii_alphabetic();
            let next_is_path = i < chars.len() && (chars[i] == '\\' || chars[i] == '/');
            let is_drive = single_letter && next_is_path;
            if !name.is_empty()
                && !is_drive
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '#')
            {
                tokens.push(Token::Label(name.to_ascii_uppercase()));
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
            let start = i;
            while i < chars.len() {
                let ch = chars[i];
                if ch.is_whitespace() || ch == '|' || endchar == Some(ch) {
                    break;
                }
                i += 1;
            }
            let rest: String = chars[start..i].iter().collect();
            tokens.push(Token::Word(format!("{word}{rest}")));
            continue;
        }
        tokens.push(Token::Word(word));
    }

    tokens
}

fn is_string_delimiter(c: char) -> bool {
    // `:` is not included — CMS labels (`L:`) and Windows drives (`C:/…`).
    // `/` is a string delimiter only when it begins a token (see above), so
    // paths like `examples/upper.rexx` stay a single Word.
    matches!(c, '/' | '%' | '$' | '#' | '!' | ';' | ',')
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
}
