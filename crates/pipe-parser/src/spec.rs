use std::collections::HashMap;
use std::fmt;

use crate::operands::stage_args;
use crate::scanner::{scan_spanned, SpannedToken, Token};

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
#[derive(Debug, Clone, Default)]
pub struct PipeOptions {
    pub endchar: Option<char>,
    /// Emit dispatcher TRACE lines (CMS-inspired).
    pub trace: bool,
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
    let tokens = scan_spanned(input, options.endchar);
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
                    let (verb, args, raw_args, ni) = parse_stage_body(input, &tokens, i)?;
                    i = ni;
                    current.push(ParsedStage {
                        label: Some(name),
                        label_occurrence: 0,
                        verb,
                        args,
                        raw_args,
                        connector: None,
                    });
                } else {
                    // Reference: creates another stream attachment point.
                    // May be followed by a verb (rare) or stand alone.
                    if i < tokens.len() && matches!(tokens[i].token, Token::Word(_)) {
                        let (verb, args, raw_args, ni) = parse_stage_body(input, &tokens, i)?;
                        i = ni;
                        current.push(ParsedStage {
                            label: Some(name),
                            label_occurrence: occ,
                            verb,
                            args,
                            raw_args,
                            connector: None,
                        });
                    } else {
                        current.push(ParsedStage {
                            label: Some(name),
                            label_occurrence: occ,
                            verb: String::new(),
                            args: Vec::new(),
                            raw_args: String::new(),
                            connector: None,
                        });
                    }
                }
            }
            Token::Word(_) | Token::Delimited(_) | Token::Quoted(_) => {
                let (verb, args, raw_args, ni) = parse_stage_body(input, &tokens, i)?;
                i = ni;
                current.push(ParsedStage {
                    label: None,
                    label_occurrence: 0,
                    verb,
                    args,
                    raw_args,
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
) -> Result<(String, Vec<StageArg>, String, usize), ParseError> {
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
    let raw_args = if arg_start < i {
        let from = tokens[arg_start].start;
        let to = tokens[i - 1].end;
        input.get(from..to).unwrap_or("").to_string()
    } else {
        String::new()
    };
    let args = stage_args(&verb, &raw_args);
    Ok((verb, args, raw_args, i))
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
        if p == "ENDCHAR" || p == "END" {
            i += 1;
            let ch = parts
                .get(i)
                .ok_or_else(|| ParseError::msg("ENDCHAR requires a character"))?;
            opts.endchar = ch.chars().next();
            i += 1;
        } else if p == "TRACE" {
            opts.trace = true;
            i += 1;
        } else {
            // Unknown option — skip word for forward compatibility.
            i += 1;
        }
    }
    Ok((opts, rest))
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
        assert_eq!(p.stages[1].raw_args, "* / / 10");
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
