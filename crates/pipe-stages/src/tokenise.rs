//! TOKENISE — split a record into tokens.
//!
//! Author's Edition 1.1.11 p.558. Synonym `TOKENIZE`.
//!
//! Syntax (diagram all-caps; no verb prefix):
//! `TOKENISE | TOKENIZE  dstring  [dstring]`
//!
//! Blanks always delimit tokens and are discarded. Characters in the first
//! dstring are single-character tokens written on their own (and they also
//! delimit other tokens). The optional second dstring is written as a
//! separate record after each input line is processed.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_delimited};
use crate::xrange::ByteSet;

pub fn tokenise(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_tokenise(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("TOKENISE: {e}"));
                return;
            }
        };

        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::PRIMARY.0 {
                ctx.fail("TOKENISE: no input stream other than the primary may be connected");
                return;
            }
        }

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);
        run_tokenise(ctx, &spec);
    })
}

#[derive(Debug)]
struct TokeniseSpec {
    singles: ByteSet,
    after_line: Option<Vec<u8>>,
}

fn parse_tokenise(args: &[String]) -> Result<TokeniseSpec, String> {
    if args.is_empty() {
        return Err("delimitedString is required".into());
    }
    if !is_delimited(&args[0]) {
        return Err(not_dstring(&args[0]));
    }
    let first = decode_dstring(&args[0])?;
    let mut singles = ByteSet::new();
    for &b in &first {
        // Blanks always delimit and are discarded (p.558). A blank in the
        // first dstring is not written as a single-character token.
        if b != b' ' {
            singles.insert(b);
        }
    }

    let mut i = 1;
    let mut after_line = None;
    if i < args.len() {
        if !is_delimited(&args[i]) {
            return Err(not_dstring(&args[i]));
        }
        after_line = Some(decode_dstring(&args[i])?);
        i += 1;
    }
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(TokeniseSpec {
        singles,
        after_line,
    })
}

fn not_dstring(arg: &str) -> String {
    let u = arg.to_ascii_uppercase();
    if u == "BLANK"
        || u == "SPACE"
        || u == "STRING"
        || u == "ANYOF"
        || u == "ANYCASE"
        || u == "PAD"
        || u == "NOT"
    {
        format!("unexpected operand {arg}")
    } else {
        format!("not a delimited string: {arg}")
    }
}

fn run_tokenise(ctx: &mut StageCtx, spec: &TokeniseSpec) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let tokens = tokenise_record(rec.as_bytes(), &spec.singles);
            for token in tokens {
                if ctx.write(Record::from(token)).is_err() {
                    return;
                }
                if !ctx.output_connected(StreamId::PRIMARY) {
                    return;
                }
            }
            if let Some(sep) = &spec.after_line {
                if ctx.write(Record::from(sep.clone())).is_err() {
                    return;
                }
            }
            let _ = ctx.read();
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

fn tokenise_record(record: &[u8], singles: &ByteSet) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < record.len() {
        if record[i] == b' ' {
            i += 1;
            continue;
        }
        if singles.contains(record[i]) {
            out.push(vec![record[i]]);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < record.len() && record[i] != b' ' && !singles.contains(record[i]) {
            i += 1;
        }
        out.push(record[start..i].to_vec());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<TokeniseSpec, String> {
        parse_tokenise(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn toks(record: &str, first: &str) -> Vec<String> {
        let spec = parse(&[first]).unwrap();
        tokenise_record(record.as_bytes(), &spec.singles)
            .into_iter()
            .map(|b| String::from_utf8(b).unwrap())
            .collect()
    }

    #[test]
    fn book_example_apples() {
        // p.558: `apples = bananas(cherries+dates)` with `/()=+/`.
        assert_eq!(
            toks("apples = bananas(cherries+dates)", "/()=+/"),
            vec!["apples", "=", "bananas", "(", "cherries", "+", "dates", ")"]
        );
    }

    #[test]
    fn blanks_always_delimit_and_are_discarded() {
        assert_eq!(toks("  a   b  ", "//"), vec!["a", "b"]);
        assert_eq!(toks("a  b", "/()/"), vec!["a", "b"]);
        assert_eq!(toks("   ", "/()=+/"), Vec::<String>::new());
    }

    #[test]
    fn null_record_has_no_tokens() {
        assert_eq!(toks("", "/()=+/"), Vec::<String>::new());
        let spec = parse(&["/()=+/", "/ /"]).unwrap();
        assert!(tokenise_record(b"", &spec.singles).is_empty());
        assert_eq!(spec.after_line.as_deref(), Some(b" ".as_slice()));
    }

    #[test]
    fn null_first_dstring_splits_on_blanks_only() {
        assert_eq!(toks("apples=bananas", "//"), vec!["apples=bananas"]);
        assert_eq!(toks("apples=bananas", ",,"), vec!["apples=bananas"]);
        assert_eq!(toks("a = b", "//"), vec!["a", "=", "b"]);
        assert_eq!(toks("a=b", "/=/"), vec!["a", "=", "b"]);
    }

    #[test]
    fn blank_in_first_dstring_is_not_written() {
        // `/ /` is a one-blank first dstring. Blanks are discarded, not emitted.
        assert_eq!(toks("a b", "/ /"), vec!["a", "b"]);
        assert_eq!(toks("a  b", "/ /"), vec!["a", "b"]);
    }

    #[test]
    fn consecutive_single_char_tokens() {
        assert_eq!(toks("a()", "/()/"), vec!["a", "(", ")"]);
        assert_eq!(toks("==", "/=/"), vec!["=", "="]);
        assert_eq!(toks("()", "/()/"), vec!["(", ")"]);
    }

    #[test]
    fn second_dstring_is_optional() {
        assert!(parse(&["/()/"]).unwrap().after_line.is_none());
        let spec = parse(&["/()/", "/ /"]).unwrap();
        assert_eq!(spec.after_line.as_deref(), Some(b" ".as_slice()));
        let spec = parse(&["/()/", "//"]).unwrap();
        assert_eq!(spec.after_line.as_deref(), Some(b"".as_slice()));
    }

    #[test]
    fn first_dstring_is_required() {
        let e = parse(&[]).unwrap_err();
        assert!(e.contains("delimitedString is required"), "{e}");
    }

    #[test]
    fn invented_and_non_dstring_rejected() {
        for w in ["BLANK", "SPACE", "ANYCASE", "STRING", "ANYOF", "PAD", "NOT"] {
            let e = parse(&[w]).unwrap_err();
            assert!(e.contains("unexpected operand"), "{w}: {e}");
        }
        let e = parse(&["hello"]).unwrap_err();
        assert!(e.contains("not a delimited string"), "{e}");
        let e = parse(&["/()/", "extra"]).unwrap_err();
        assert!(e.contains("not a delimited string"), "{e}");
        let e = parse(&["/()/", "/ /", "/x/"]).unwrap_err();
        assert!(e.contains("unexpected operand"), "{e}");
    }

    #[test]
    fn hex_dstring_is_accepted() {
        // Portable ASCII: x28293d2b is `()=+` (book EBCDIC hex would differ).
        assert_eq!(toks("a=b", "x3d"), vec!["a", "=", "b"]);
    }
}
