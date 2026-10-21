use std::collections::HashMap;
use std::fmt;

use crate::operands::stage_args;
use crate::scanner::{scan_spanned_with, ScanConfig, SpannedToken, Token};

#[derive(Debug, Clone)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    fn msg(m: impl Into<String>) -> Self {
        Self(m.into())
    }
}

/// Options from a leading `(…)` group, e.g. `(ENDCHAR ?)` or `(TRACE)`.
#[derive(Debug, Clone)]
pub struct PipeOptions {
    pub endchar: Option<char>,
    /// Stage separator (Author's Edition 1.1.11 p.206; default solid vertical bar).
    pub separator: char,
    /// Escape character (p.206; no default). Deleted; the next character is ordinary.
    pub escape: Option<char>,
    /// Emit dispatcher TRACE lines (CMS-inspired).
    pub trace: bool,
    /// Optional pipeline name (NAME word); stored for messages.
    pub name: Option<String>,
}

impl Default for PipeOptions {
    fn default() -> Self {
        Self {
            endchar: None,
            separator: '|',
            escape: None,
            trace: false,
            name: None,
        }
    }
}

/// A connector `*:` (IBM) at the start or end of a nested pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorKind {
    Input,
    Output,
}

/// One stage invocation in a parsed specification.
#[derive(Debug, Clone)]
pub struct ParsedStage {
    /// Optional label on this occurrence (defines/references a stream).
    pub label: Option<String>,
    /// Occurrence index for this label (0 = primary / definition).
    pub label_occurrence: u32,
    /// Stage verb (uppercased for lookup).
    pub verb: String,
    /// Remaining operand words / delimited strings (after per-stage rescan).
    pub args: Vec<StageArg>,
    /// Exact operand text after the verb (internal blanks preserved).
    pub raw_args: String,
    /// Original stage specification for CMS message 1 (`... Running "…"`).
    ///
    /// Label and local options are omitted; the verb is the typed spelling.
    pub running: String,
    /// True if this stage is a `*:` connector placeholder.
    pub connector: Option<ConnectorKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageArg {
    Word(String),
    Delimited(String),
    Quoted(String),
}

impl StageArg {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Word(s) | Self::Delimited(s) | Self::Quoted(s) => s,
        }
    }

    /// Inner text of a delimited string `/foo/` → `foo`, or the raw word.
    pub fn inner(&self) -> &str {
        match self {
            Self::Delimited(s) if s.len() >= 2 => &s[1..s.len() - 1],
            Self::Quoted(s) => s,
            Self::Word(s) => s,
            Self::Delimited(s) => s,
        }
    }
}

/// One pipeline (possibly part of a multi-pipeline set).
#[derive(Debug, Clone)]
pub struct ParsedPipeline {
    pub options: PipeOptions,
    pub stages: Vec<ParsedStage>,
    /// Parallel pipelines in the set (separated by ENDCHAR).
    pub pipelines: Vec<Vec<ParsedStage>>,
}

/// Parse a full PIPE argument string, including optional leading `(OPTS)`.
pub fn parse_pipeline(input: &str) -> Result<ParsedPipeline, ParseError> {
    let (opts, rest) = split_options(input)?;
    parse_with_options(rest, opts)
}

pub fn parse_with_options(input: &str, options: PipeOptions) -> Result<ParsedPipeline, ParseError> {
    let tokens = scan_spanned_with(
        input,
        ScanConfig {
            separator: options.separator,
            endchar: options.endchar,
            escape: options.escape,
        },
    );
    let mut pipelines: Vec<Vec<ParsedStage>> = Vec::new();
    let mut current: Vec<ParsedStage> = Vec::new();
    let mut label_counts: HashMap<String, u32> = HashMap::new();

    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i].token {
            Token::PipelineEnd => {
                if !current.is_empty() {
                    pipelines.push(std::mem::take(&mut current));
                }
                i += 1;
            }
            Token::StageSep => {
                i += 1;
            }
            Token::Connector => {
                let kind = if current.is_empty() {
                    ConnectorKind::Input
                } else {
                    ConnectorKind::Output
                };
                current.push(ParsedStage {
                    label: None,
                    label_occurrence: 0,
                    verb: "*:".into(),
                    args: Vec::new(),
                    raw_args: String::new(),
                    running: "*:".into(),
                    connector: Some(kind),
                });
                i += 1;
            }
            Token::Label(name) => {
                let name = name.clone();
                let occ = {
                    let e = label_counts.entry(name.clone()).or_insert(0);
                    let occ = *e;
                    *e += 1;
                    occ
                };
                i += 1;
                // Label may stand alone as a stream reconnect (no verb), or
                // precede a verb on the defining occurrence.
                if occ == 0 {
                    // Definition: next token should be a verb (or connector).
                    let (verb, args, raw_args, running, ni) = parse_stage_body(input, &tokens, i)?;
                    i = ni;
                    current.push(ParsedStage {
                        label: Some(name),
                        label_occurrence: 0,
                        verb,
                        args,
                        raw_args,
                        running,
                        connector: None,
                    });
                } else {
                    // Reference: creates another stream attachment point.
                    // May be followed by a verb (rare) or stand alone.
                    if i < tokens.len() && matches!(tokens[i].token, Token::Word(_)) {
                        let (verb, args, raw_args, running, ni) =
                            parse_stage_body(input, &tokens, i)?;
                        i = ni;
                        current.push(ParsedStage {
                            label: Some(name),
                            label_occurrence: occ,
                            verb,
                            args,
                            raw_args,
                            running,
                            connector: None,
                        });
                    } else {
                        current.push(ParsedStage {
                            label: Some(name.clone()),
                            label_occurrence: occ,
                            verb: String::new(),
                            args: Vec::new(),
                            raw_args: String::new(),
                            running: format!("{name}:"),
                            connector: None,
                        });
                    }
                }
            }
            Token::Word(_) | Token::Delimited(_) | Token::Quoted(_) => {
                let (verb, args, raw_args, running, ni) = parse_stage_body(input, &tokens, i)?;
                i = ni;
                current.push(ParsedStage {
                    label: None,
                    label_occurrence: 0,
                    verb,
                    args,
                    raw_args,
                    running,
                    connector: None,
                });
            }
        }
    }
    if !current.is_empty() {
        pipelines.push(current);
    }

    if pipelines.is_empty() {
        return Err(ParseError::msg("empty pipeline specification"));
    }

    // Flatten primary list for convenience; keep all in `pipelines`.
    let stages = pipelines[0].clone();
    Ok(ParsedPipeline {
        options,
        stages,
        pipelines,
    })
}

fn parse_stage_body(
    input: &str,
    tokens: &[SpannedToken],
    mut i: usize,
) -> Result<(String, Vec<StageArg>, String, String, usize), ParseError> {
    let verb_start = tokens.get(i).map(|t| t.start).unwrap_or(0);
    let verb = match tokens.get(i).map(|t| &t.token) {
        Some(Token::Word(w)) => {
            i += 1;
            // Preserve original spelling for filesystem paths (*.rexx / *.rex); lookup
            // layers compare case-insensitively where needed.
            let lower = w.to_ascii_lowercase();
            if lower.ends_with(".rex")
                || lower.ends_with(".rexx")
                || w.contains('/')
                || w.contains('\\')
            {
                w.clone()
            } else {
                w.to_ascii_uppercase()
            }
        }
        Some(Token::Connector) => {
            i += 1;
            "*:".into()
        }
        other => {
            return Err(ParseError::msg(format!(
                "expected stage verb, found {other:?}"
            )))
        }
    };
    let verb_end = tokens[i.saturating_sub(1)].end;
    let arg_start = i;
    while i < tokens.len() {
        match &tokens[i].token {
            Token::StageSep | Token::PipelineEnd | Token::Label(_) => break,
            // A bare `*` after the verb is the operand (TAKE/DROP `*`), not a
            // connector stage. A `*` that is the whole stage stays a connector
            // in the caller.
            Token::Connector | Token::Word(_) | Token::Delimited(_) | Token::Quoted(_) => {
                i += 1;
            }
        }
    }
    // Argument string after the verb (1.1.11 p.208). Token bodies are the
    // already-unescaped scanner values (`||` → `|`); original whitespace
    // between tokens is kept so LITERAL can honour "starts after exactly
    // one blank".
    let raw_args = reconstruct_raw_args(input, tokens, verb_end, arg_start, i);
    let args = stage_args(&verb, &raw_args);
    let after = if i < tokens.len() {
        tokens[i].start
    } else {
        input.len()
    };
    let running = input
        .get(verb_start..after)
        .unwrap_or("")
        .trim()
        .to_string();
    Ok((verb, args, raw_args, running, i))
}

fn reconstruct_raw_args(
    input: &str,
    tokens: &[SpannedToken],
    verb_end: usize,
    arg_start: usize,
    arg_end: usize,
) -> String {
    let after = if arg_end < tokens.len() {
        tokens[arg_end].start
    } else {
        input.len()
    };
    if arg_start >= arg_end {
        return input.get(verb_end..after).unwrap_or("").to_string();
    }
    let mut out = input
        .get(verb_end..tokens[arg_start].start)
        .unwrap_or("")
        .to_string();
    for k in arg_start..arg_end {
        out.push_str(&token_lexeme(&tokens[k].token));
        let gap_to = if k + 1 < arg_end {
            tokens[k + 1].start
        } else {
            after
        };
        if tokens[k].end < gap_to {
            if let Some(gap) = input.get(tokens[k].end..gap_to) {
                out.push_str(gap);
            }
        }
    }
    out
}

fn token_lexeme(token: &Token) -> String {
    match token {
        Token::Word(s) | Token::Delimited(s) => s.clone(),
        Token::Quoted(s) => format!("\"{s}\""),
        Token::Connector => "*".into(),
        Token::Label(s) => format!("{s}:"),
        Token::StageSep => "|".into(),
        Token::PipelineEnd => String::new(),
    }
}

fn split_options(input: &str) -> Result<(PipeOptions, &str), ParseError> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('(') {
        return Ok((PipeOptions::default(), input));
    }
    let close = trimmed
        .find(')')
        .ok_or_else(|| ParseError::msg("unclosed options group"))?;
    let inside = &trimmed[1..close];
    let rest = trimmed[close + 1..].trim_start();
    let mut opts = PipeOptions::default();
    let parts: Vec<&str> = inside.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        let p = parts[i].to_ascii_uppercase();
        if matches_option(&p, "ENDCHAR", 3) || p == "END" {
            i += 1;
            let ch = parts
                .get(i)
                .ok_or_else(|| ParseError::msg("ENDCHAR requires a character"))?;
            opts.endchar = Some(parse_scanner_xorc(ch, "ENDCHAR")?);
            i += 1;
        } else if matches_option(&p, "SEPARATOR", 3) || matches_option(&p, "STAGESEP", 8) {
            i += 1;
            let ch = parts
                .get(i)
                .ok_or_else(|| ParseError::msg("SEPARATOR requires a character"))?;
            opts.separator = parse_scanner_xorc(ch, "SEPARATOR")?;
            i += 1;
        } else if matches_option(&p, "ESCAPE", 3) {
            i += 1;
            let ch = parts
                .get(i)
                .ok_or_else(|| ParseError::msg("ESCAPE requires a character"))?;
            opts.escape = Some(parse_scanner_xorc(ch, "ESCAPE")?);
            i += 1;
        } else if p == "TRACE" {
            opts.trace = true;
            i += 1;
        } else if p == "NAME" {
            i += 1;
            let word = parts
                .get(i)
                .ok_or_else(|| ParseError::msg("NAME requires a word"))?;
            opts.name = Some((*word).to_string());
            i += 1;
        } else {
            // Unknown option — skip word for forward compatibility.
            i += 1;
        }
    }
    Ok((opts, rest))
}

fn matches_option(s: &str, full: &str, min_len: usize) -> bool {
    s.len() >= min_len && s.len() <= full.len() && full.starts_with(s)
}

/// xorc for SEPARATOR / ENDCHAR / ESCAPE (1.1.11 p.115): one character or two hex digits.
/// Parentheses, `*`, `:`, `.`, and blank are rejected.
fn parse_scanner_xorc(s: &str, what: &str) -> Result<char, ParseError> {
    let ch = if s.len() == 2 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let v = u8::from_str_radix(s, 16)
            .map_err(|_| ParseError::msg(format!("{what} has invalid hex value {s}")))?;
        char::from(v)
    } else if s.chars().count() == 1 {
        s.chars().next().unwrap()
    } else {
        return Err(ParseError::msg(format!(
            "{what} requires a single character or two hex digits"
        )));
    };
    if ch == '(' || ch == ')' || ch == '*' || ch == ':' || ch == '.' || ch.is_whitespace() {
        return Err(ParseError::msg(format!(
            "{what} cannot be {ch:?} (reserved for the scanner)"
        )));
    }
    Ok(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_literal_console() {
        let p = parse_pipeline("literal hello world | console").unwrap();
        assert_eq!(p.stages.len(), 2);
        assert_eq!(p.stages[0].verb, "LITERAL");
        assert_eq!(p.stages[1].verb, "CONSOLE");
    }

    #[test]
    fn parses_endchar_multipipe() {
        let p = parse_pipeline("(ENDCHAR ?) literal a | L: locate /x/ | console ? L: | console")
            .unwrap();
        assert_eq!(p.pipelines.len(), 2);
        assert_eq!(p.pipelines[0][1].label.as_deref(), Some("L"));
        assert_eq!(p.pipelines[1][0].label_occurrence, 1);
        assert_eq!(p.pipelines[0][1].running, "locate /x/");
        assert_eq!(p.pipelines[1][0].running, "L:");
    }

    #[test]
    fn parses_trace_option() {
        let p = parse_pipeline("(TRACE) literal hi | console").unwrap();
        assert!(p.options.trace);
        assert_eq!(p.stages[0].verb, "LITERAL");
    }

    #[test]
    fn parses_trace_with_endchar() {
        let p = parse_pipeline("(TRACE ENDCHAR ?) literal a | console").unwrap();
        assert!(p.options.trace);
        assert_eq!(p.options.endchar, Some('?'));
    }

    #[test]
    fn star_after_verb_is_operand() {
        let p = parse_pipeline("take * | console").unwrap();
        assert_eq!(p.stages.len(), 2);
        assert_eq!(p.stages[0].verb, "TAKE");
        assert_eq!(p.stages[0].args, vec![StageArg::Word("*".into())]);
        assert!(p.stages[0].connector.is_none());

        let last = parse_pipeline("drop last * | hole").unwrap();
        assert_eq!(
            last.stages[0].args,
            vec![StageArg::Word("last".into()), StageArg::Word("*".into())]
        );
    }

    #[test]
    fn bare_star_stage_stays_connector() {
        let p = parse_pipeline("* | hole | *").unwrap();
        assert_eq!(p.stages.len(), 3);
        assert!(p.stages[0].connector == Some(ConnectorKind::Input));
        assert_eq!(p.stages[1].verb, "HOLE");
        assert!(p.stages[1].connector.is_none());
        assert!(p.stages[2].connector == Some(ConnectorKind::Output));
    }

    #[test]
    fn join_blank_insert_round_trips() {
        let p = parse_pipeline("literal x | join * / / 10 | hole").unwrap();
        assert_eq!(p.stages[1].verb, "JOIN");
        assert_eq!(p.stages[1].raw_args.trim(), "* / / 10");
        assert_eq!(
            p.stages[1].args,
            vec![
                StageArg::Word("*".into()),
                StageArg::Delimited("/ /".into()),
                StageArg::Word("10".into()),
            ]
        );
        assert_eq!(p.stages[1].args[1].inner(), " ");
    }

    #[test]
    fn disk_unix_path_is_one_operand() {
        let p = parse_pipeline("disk /tmp/cms-pipelines-phase1-exit.txt | hole").unwrap();
        assert_eq!(p.stages[0].verb, "DISK");
        assert_eq!(
            p.stages[0].args,
            vec![StageArg::Word("/tmp/cms-pipelines-phase1-exit.txt".into())]
        );
    }

    #[test]
    fn literal_keeps_extra_blanks() {
        let p = parse_pipeline("literal  hello  world | hole").unwrap();
        assert_eq!(
            p.stages[0].args,
            vec![StageArg::Word(" hello  world ".into())]
        );
    }

    #[test]
    fn self_escaping_bar_reaches_the_stage() {
        let p = parse_pipeline("locate /a||b/ | hole").unwrap();
        assert_eq!(p.stages[0].verb, "LOCATE");
        assert_eq!(p.stages[0].args[0].as_str(), "/a|b/");
        assert_eq!(p.stages.len(), 2);
    }

    #[test]
    fn separator_and_escape_options() {
        let p = parse_pipeline("(SEPARATOR ?) literal a ? hole").unwrap();
        assert_eq!(p.options.separator, '?');
        assert_eq!(p.stages.len(), 2);
        assert_eq!(p.stages[0].verb, "LITERAL");

        let p = parse_pipeline("(ESCAPE \") locate \"|| hole").unwrap();
        assert_eq!(p.options.escape, Some('"'));
        assert_eq!(p.stages[0].args[0].as_str(), "|");

        let err = parse_pipeline("(SEPARATOR *) literal a").unwrap_err();
        assert!(err.to_string().contains("reserved"), "{err}");
    }

    #[test]
    fn command_slash_flags_stay_words() {
        let p = parse_pipeline("command dir /s /b | hole").unwrap();
        assert_eq!(
            p.stages[0].args,
            vec![
                StageArg::Word("dir".into()),
                StageArg::Word("/s".into()),
                StageArg::Word("/b".into()),
            ]
        );
    }
}
