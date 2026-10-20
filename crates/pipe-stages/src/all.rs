//! ALL — select records that match a boolean string expression.
//!
//! Author's Edition 1.1.11 pp.259–260.
//!
//! Syntax (diagram all-caps): `ALL expression`
//!
//! An expression is alternatives (`!` or `|`) of terms (`&`) of an optional
//! not (`¬`) and a `delimitedString` or a parenthesised expression.
//! Precedence: `¬` highest, `&`, then `!`/`|` lowest. Blanks are optional.
//! Matching is LOCATE substring search (contains), including a null string
//! matching any non-null record (p.414).

use pipe_core::{StageCtx, StreamId};

use crate::common::{decode_dstring, is_delimited};

pub fn all(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let expr = match parse_all(&args) {
            Ok(expr) => expr,
            Err(e) => {
                ctx.fail(format!("ALL: {e}"));
                return;
            }
        };
        run_selection(ctx, "ALL", |rec| eval(rec, &expr));
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Contains(Vec<u8>),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    String(Vec<u8>),
    Not,
    And,
    Or,
    LParen,
    RParen,
}

fn parse_all(args: &[String]) -> Result<Expr, String> {
    if args.is_empty() {
        return Err("an expression is required".into());
    }
    let joined = args.join("");
    let trimmed = joined.trim_start();
    if trimmed.starts_with("%debug") || trimmed.starts_with("%dump") {
        return Err("Note 2 %debug/%dump is a CMS diagnostic and is not implemented".into());
    }
    let tokens = lex(trimmed)?;
    let mut i = 0;
    let expr = parse_expr(&tokens, &mut i)?;
    if i < tokens.len() {
        return Err("unexpected extra operands after the expression".into());
    }
    Ok(expr)
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let ch = match s[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        if let Some((bytes, n)) = try_dstring_at(s, i)? {
            out.push(Tok::String(bytes));
            i += n;
            continue;
        }
        match ch {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '&' => {
                out.push(Tok::And);
                i += 1;
            }
            '!' | '|' => {
                out.push(Tok::Or);
                i += 1;
            }
            // UTF-8 not sign (U+00AC). Portable alternatives from PICK Note 1
            // (p.456): caret (terminal-emulator mapping) and backslash.
            '\u{00AC}' | '^' | '\\' => {
                out.push(Tok::Not);
                i += ch.len_utf8();
            }
            _ => {
                return Err(format!(
                    "unrecognized operand starting at {}",
                    &s[i..].chars().take(16).collect::<String>()
                ));
            }
        }
    }
    if out.is_empty() {
        return Err("an expression is required".into());
    }
    Ok(out)
}

/// Parse a `delimitedString` (pair / hex / binary, p.194) at `i`.
fn try_dstring_at(s: &str, i: usize) -> Result<Option<(Vec<u8>, usize)>, String> {
    if i >= s.len() {
        return Ok(None);
    }
    let rest = &s[i..];
    let first = rest.chars().next().unwrap();

    // Pair form. Expression operators are never dstring delimiters: `!` / `|`
    // / `&` / `¬` would otherwise swallow later operators (`/abc/!/def/)&/!/`
    // must be OR, not a `!`-delimited string). Use `/` `%` `#` etc. (p.194).
    if !is_expr_punct(first)
        && !first.is_alphanumeric()
        && !first.is_whitespace()
        && first != '-'
        && first != '.'
    {
        let dlen = first.len_utf8();
        if let Some(rel) = rest[dlen..].find(first) {
            let token = &rest[..dlen + rel + dlen];
            if is_delimited(token) {
                return Ok(Some((decode_dstring(token)?, token.len())));
            }
        }
    }

    // Hex (`x`/`h`) or binary (`b`) literal: word of digits, stopped by an
    // operator or parenthesis so `!x4f` lexes as OR then hex.
    if matches!(first, 'X' | 'x' | 'H' | 'h' | 'B' | 'b') {
        let mut n = first.len_utf8();
        while n < rest.len() {
            let c = rest[n..].chars().next().unwrap();
            if c.is_whitespace() || is_expr_punct(c) {
                break;
            }
            n += c.len_utf8();
        }
        let token = &rest[..n];
        if is_delimited(token) {
            return Ok(Some((decode_dstring(token)?, n)));
        }
    }
    Ok(None)
}

fn is_expr_punct(c: char) -> bool {
    matches!(c, '(' | ')' | '&' | '!' | '|' | '^' | '\\' | '\u{00AC}')
}

fn parse_expr(tokens: &[Tok], i: &mut usize) -> Result<Expr, String> {
    let mut alts = vec![parse_alt(tokens, i)?];
    while *i < tokens.len() && tokens[*i] == Tok::Or {
        *i += 1;
        alts.push(parse_alt(tokens, i)?);
    }
    Ok(if alts.len() == 1 {
        alts.pop().unwrap()
    } else {
        Expr::Or(alts)
    })
}

fn parse_alt(tokens: &[Tok], i: &mut usize) -> Result<Expr, String> {
    let mut terms = vec![parse_factor(tokens, i)?];
    while *i < tokens.len() && tokens[*i] == Tok::And {
        *i += 1;
        terms.push(parse_factor(tokens, i)?);
    }
    Ok(if terms.len() == 1 {
        terms.pop().unwrap()
    } else {
        Expr::And(terms)
    })
}

fn parse_factor(tokens: &[Tok], i: &mut usize) -> Result<Expr, String> {
    if *i >= tokens.len() {
        return Err("missing string or parenthesised expression".into());
    }
    let negated = if tokens[*i] == Tok::Not {
        *i += 1;
        true
    } else {
        false
    };
    let inner = parse_string(tokens, i)?;
    if negated {
        Ok(Expr::Not(Box::new(inner)))
    } else {
        Ok(inner)
    }
}

fn parse_string(tokens: &[Tok], i: &mut usize) -> Result<Expr, String> {
    if *i >= tokens.len() {
        return Err("missing string or parenthesised expression".into());
    }
    match &tokens[*i] {
        Tok::String(bytes) => {
            let bytes = bytes.clone();
            *i += 1;
            Ok(Expr::Contains(bytes))
        }
        Tok::LParen => {
            *i += 1;
            let expr = parse_expr(tokens, i)?;
            if *i >= tokens.len() || tokens[*i] != Tok::RParen {
                return Err("missing closing parenthesis".into());
            }
            *i += 1;
            Ok(expr)
        }
        Tok::Not => {
            // Railroad: one optional ¬ per factor; a second ¬ needs grouping.
            Err("a second ¬ must be parenthesised (¬(¬ /string/))".into())
        }
        Tok::And | Tok::Or | Tok::RParen => {
            Err("missing string or parenthesised expression".into())
        }
    }
}

fn eval(record: &[u8], expr: &Expr) -> bool {
    match expr {
        Expr::Contains(needle) => contains(record, needle),
        Expr::Not(inner) => !eval(record, inner),
        Expr::And(terms) => terms.iter().all(|t| eval(record, t)),
        Expr::Or(alts) => alts.iter().any(|t| eval(record, t)),
    }
}

/// LOCATE substring search (p.414): a null needle matches a non-null record.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return !haystack.is_empty();
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn run_selection(ctx: &mut StageCtx, verb: &str, selected: impl Fn(&[u8]) -> bool) {
    if ctx.input_connected(StreamId::SECONDARY) {
        ctx.fail(format!(
            "{verb}: secondary input stream must not be connected"
        ));
        return;
    }
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::SECONDARY.0 {
            ctx.fail(format!(
                "{verb}: no input stream other than the primary may be connected"
            ));
            return;
        }
    }

    if !outputs_live(ctx) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    loop {
        if !outputs_live(ctx) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let stream = if selected(rec.as_bytes()) {
                StreamId::PRIMARY
            } else {
                StreamId::SECONDARY
            };
            if ctx.output_connected(stream) {
                ctx.select_output(stream);
                let write_failed = ctx.write(rec).is_err();
                let _ = ctx.read();
                if write_failed && !outputs_live(ctx) {
                    return;
                }
            } else {
                let _ = ctx.read();
            }
            continue;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(args: &[&str]) -> Expr {
        parse_all(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn hit(rec: &[u8], args: &[&str]) -> bool {
        eval(rec, &spec(args))
    }

    #[test]
    fn book_or_of_string_and_hex_bar() {
        // p.259: `all /abc/ ! x4f` — x4f is EBCDIC `|`; portable ASCII uses x7c.
        assert!(hit(b"xxabcxx", &["/abc/", "!", "x7c"]));
        assert!(hit(b"a|b", &["/abc/", "!", "x7c"]));
        assert!(!hit(b"xyz", &["/abc/", "!", "x7c"]));
        // The book's x4f is byte 0x4F ('O' on ASCII), not a vertical bar.
        assert!(hit(b"O", &["/abc/", "!", "x4f"]));
        assert!(!hit(b"|", &["/abc/", "!", "x4f"]));
    }

    #[test]
    fn book_grouped_or_and_exclamation() {
        // p.260: `all (/abc/ ! /def/) & /!/`
        let args = ["(", "/abc/", "!", "/def/", ")", "&", "/!/"];
        assert!(hit(b"abc!", &args));
        assert!(hit(b"def!", &args));
        assert!(hit(b"!abcdef", &args));
        assert!(!hit(b"abc", &args));
        assert!(!hit(b"!", &args));
        assert!(!hit(b"xyz!", &args));
    }

    #[test]
    fn not_sign_and_portable_alternatives() {
        for not in ["\u{00AC}", "^", "\\"] {
            assert!(!hit(b"abc", &[not, "/abc/"]));
            assert!(hit(b"xyz", &[not, "/abc/"]));
        }
    }

    #[test]
    fn vertical_bar_or_from_self_escape() {
        assert!(hit(b"abc", &["/abc/", "|", "/def/"]));
        assert!(hit(b"def", &["/abc/", "|", "/def/"]));
        assert!(!hit(b"xyz", &["/abc/", "|", "/def/"]));
    }

    #[test]
    fn glued_operators_without_blanks() {
        assert!(hit(b"abc", &["/abc/!/def/"]));
        assert!(hit(b"abxy", &["/ab/&/xy/"]));
        assert!(!hit(b"ab", &["/ab/&/xy/"]));
        assert!(!hit(b"abc", &["\u{00AC}/abc/"]));
    }

    #[test]
    fn null_dstring_matches_non_null_like_locate() {
        assert!(hit(b"a", &["//"]));
        assert!(!hit(b"", &["//"]));
        assert!(hit(b"", &["\u{00AC}", "//"]));
        assert!(!hit(b"a", &["\u{00AC}", "//"]));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        // a & b ! c  ≡  (a & b) ! c
        let args = ["/a/", "&", "/b/", "!", "/c/"];
        assert!(hit(b"ab", &args));
        assert!(hit(b"c", &args));
        assert!(hit(b"cxy", &args));
        assert!(!hit(b"a", &args));
        assert!(!hit(b"b", &args));
    }

    #[test]
    fn grouping_overrides_precedence() {
        let args = ["(", "/a/", "!", "/b/", ")", "&", "/c/"];
        assert!(hit(b"ac", &args));
        assert!(hit(b"bc", &args));
        assert!(!hit(b"ab", &args));
        assert!(!hit(b"c", &args));
    }

    #[test]
    fn stacked_not_without_parens_is_rejected() {
        let err = parse_all(&["\u{00AC}".into(), "\u{00AC}".into(), "/abc/".into()]).unwrap_err();
        assert!(err.contains("parenthes") || err.contains("second"), "{err}");
    }

    #[test]
    fn debug_dump_are_rejected() {
        let err = parse_all(&["%debug".into(), "/a/".into()]).unwrap_err();
        assert!(err.contains("%debug"), "{err}");
        let err = parse_all(&["%dump".into(), "/a/".into()]).unwrap_err();
        assert!(err.contains("%dump"), "{err}");
    }

    #[test]
    fn missing_expression_is_rejected() {
        assert!(parse_all(&[]).unwrap_err().contains("required"));
    }
}
