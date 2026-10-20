//! WILDCARD — select records that match a LISTFILE-style pattern.
//!
//! Author's Edition 1.1.11 pp.597–598.
//!
//! Syntax (diagram all-caps):
//! `WILDCARD [ANYCase] [BLANK chars | ANYCHaracter chars | ANYSTRing chars]...
//!          [inputRange] delimitedString`
//!
//! `chars` is `xorc` or `OFF`. Defaults: blank, `%` any-character, `*` any-string.
//! Metacharacters are case-sensitive even under ANYCASE and must all differ.

use pipe_core::{StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase_min4, is_delimited, matches_abbrev};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, parse_xorc, InputRange,
};

pub fn wildcard(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_wildcard(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("WILDCARD: {e}"));
                return;
            }
        };
        run_selection(ctx, "WILDCARD", |rec| matches_record(rec, &spec));
    })
}

#[derive(Debug)]
struct WildcardSpec {
    anycase: bool,
    /// `None` when BLANK OFF: the whole range is one word.
    blank: Option<u8>,
    anychar: Option<u8>,
    anystring: Option<u8>,
    range: Option<InputRange>,
    pattern: Vec<u8>,
}

fn parse_wildcard(args: &[String]) -> Result<WildcardSpec, String> {
    let mut i = 0;
    let mut anycase = false;
    if i < args.len() && is_anycase_min4(&args[i]) {
        anycase = true;
        i += 1;
    }

    let mut blank = Some(b' ');
    let mut anychar = Some(b'%');
    let mut anystring = Some(b'*');

    while i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "BLANK" {
            i += 1;
            blank = parse_charspec(args, &mut i, "BLANK")?;
        } else if is_anycharacter(&upper) {
            i += 1;
            anychar = parse_charspec(args, &mut i, "ANYCHARACTER")?;
        } else if is_anystring(&upper) {
            i += 1;
            anystring = parse_charspec(args, &mut i, "ANYSTRING")?;
        } else {
            break;
        }
    }

    // Last definition of each role wins; enabled metacharacters must differ.
    check_distinct(blank, anychar, anystring)?;

    let mut range = None;
    if i < args.len() && looks_like_input_range_start(&args[i]) {
        // A range is only consumed when a pattern dstring still follows.
        let save = i;
        match parse_input_range(args, &mut i) {
            Ok(r) if i < args.len() => range = Some(r),
            _ => i = save,
        }
    }

    if i >= args.len() {
        return Err("a delimited string pattern is required".into());
    }
    if !is_delimited(&args[i]) {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let pattern = decode_dstring(&args[i])?;
    i += 1;
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(WildcardSpec {
        anycase,
        blank,
        anychar,
        anystring,
        range,
        pattern,
    })
}

/// `OFF` disables the metacharacter (`None`); otherwise an xorc.
fn parse_charspec(args: &[String], i: &mut usize, kw: &str) -> Result<Option<u8>, String> {
    if *i >= args.len() {
        return Err(format!("{kw} requires an xorc or OFF"));
    }
    let token = &args[*i];
    if token.to_ascii_uppercase() == "OFF" {
        *i += 1;
        return Ok(None);
    }
    let b = parse_xorc(token)?;
    *i += 1;
    Ok(Some(b))
}

fn is_anycharacter(upper: &str) -> bool {
    matches_abbrev(upper, "ANYCHARACTER", 5)
}

fn is_anystring(upper: &str) -> bool {
    matches_abbrev(upper, "ANYSTRING", 6)
}

fn check_distinct(
    blank: Option<u8>,
    anychar: Option<u8>,
    anystring: Option<u8>,
) -> Result<(), String> {
    let mut seen: Vec<(u8, &'static str)> = Vec::new();
    push_meta(&mut seen, blank, "word delimiter")?;
    push_meta(&mut seen, anychar, "any character")?;
    push_meta(&mut seen, anystring, "any string")?;
    Ok(())
}

fn push_meta(
    seen: &mut Vec<(u8, &'static str)>,
    b: Option<u8>,
    name: &'static str,
) -> Result<(), String> {
    let Some(b) = b else {
        return Ok(());
    };
    if let Some((_, other)) = seen.iter().find(|(c, _)| *c == b) {
        return Err(format!(
            "metacharacters must all be different ({name} collides with {other})"
        ));
    }
    seen.push((b, name));
    Ok(())
}

fn matches_record(record: &[u8], spec: &WildcardSpec) -> bool {
    let bytes = match &spec.range {
        Some(range) => extract_input_range(record, range),
        None => record.to_vec(),
    };
    match_pattern(&spec.pattern, &bytes, spec)
}

struct Words {
    leading: bool,
    trailing: bool,
    words: Vec<Vec<u8>>,
}

fn split_words(bytes: &[u8], delim: Option<u8>) -> Words {
    let Some(d) = delim else {
        return Words {
            leading: false,
            trailing: false,
            words: if bytes.is_empty() {
                Vec::new()
            } else {
                vec![bytes.to_vec()]
            },
        };
    };
    if bytes.is_empty() {
        return Words {
            leading: false,
            trailing: false,
            words: Vec::new(),
        };
    }
    let leading = bytes[0] == d;
    let trailing = *bytes.last().unwrap() == d;
    let words = bytes
        .split(|&b| b == d)
        .filter(|w| !w.is_empty())
        .map(|w| w.to_vec())
        .collect();
    Words {
        leading,
        trailing,
        words,
    }
}

fn match_pattern(pattern: &[u8], input: &[u8], spec: &WildcardSpec) -> bool {
    let p = split_words(pattern, spec.blank);
    let t = split_words(input, spec.blank);
    // Left margin is rigorous; trailing delimiters on the input are allowed
    // unless the pattern itself ends with a delimiter (p.598).
    if p.leading != t.leading {
        return false;
    }
    if p.trailing && !t.trailing {
        return false;
    }
    if p.words.len() != t.words.len() {
        return false;
    }
    p.words
        .iter()
        .zip(t.words.iter())
        .all(|(pw, tw)| match_word(pw, tw, spec))
}

fn match_word(pat: &[u8], text: &[u8], spec: &WildcardSpec) -> bool {
    if let Some(star) = spec.anystring {
        if !pat.is_empty() && pat.iter().all(|&c| c == star) {
            // Any-strings only: one character, not empty (LISTFILE, p.598).
            return text.len() == 1;
        }
    }
    glob(pat, text, spec.anychar, spec.anystring, spec.anycase)
}

fn glob(pat: &[u8], text: &[u8], any: Option<u8>, star: Option<u8>, anycase: bool) -> bool {
    let m = pat.len();
    let n = text.len();
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;
    for i in 1..=m {
        if star == Some(pat[i - 1]) && dp[i - 1][0] {
            dp[i][0] = true;
        } else {
            break;
        }
    }
    for i in 1..=m {
        for j in 1..=n {
            let pc = pat[i - 1];
            if star == Some(pc) {
                dp[i][j] = dp[i][j - 1] || dp[i - 1][j] || dp[i - 1][j - 1];
            } else if any == Some(pc) {
                dp[i][j] = dp[i - 1][j - 1];
            } else if eq_lit(pc, text[j - 1], anycase) {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }
    dp[m][n]
}

fn eq_lit(a: u8, b: u8, anycase: bool) -> bool {
    if anycase {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
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

    fn spec(args: &[&str]) -> WildcardSpec {
        parse_wildcard(&args.iter().map(|s| (*s).to_string()).collect::<Vec<_>>()).unwrap()
    }

    fn hit(rec: &[u8], args: &[&str]) -> bool {
        matches_record(rec, &spec(args))
    }

    #[test]
    fn star_and_percent_defaults() {
        assert!(hit(b"abc", &["/a*/"]));
        assert!(hit(b"a", &["/a*/"]));
        assert!(hit(b"axc", &["/a%c/"]));
        assert!(!hit(b"ac", &["/a%c/"]));
        assert!(!hit(b"abxc", &["/a%c/"]));
    }

    #[test]
    fn star_only_word_is_one_character() {
        assert!(hit(b"a", &["/*/"]));
        assert!(!hit(b"", &["/*/"]));
        assert!(!hit(b"ab", &["/*/"]));
    }

    #[test]
    fn star_with_literal_can_be_null() {
        assert!(hit(b"ab", &["/a*b/"]));
        assert!(hit(b"axxxb", &["/a*b/"]));
    }

    #[test]
    fn word_count_must_agree() {
        assert!(hit(b"a b", &["/a b/"]));
        assert!(hit(b"a  b", &["/a b/"])); // one-or-more delimiters
        assert!(!hit(b"ab", &["/a b/"]));
        assert!(!hit(b"a b c", &["/a b/"]));
    }

    #[test]
    fn left_margin_is_rigorous() {
        assert!(!hit(b" a", &["/a/"]));
        assert!(hit(b" a", &["/ a/"]));
        assert!(!hit(b"a", &["/ a/"]));
    }

    #[test]
    fn trailing_delimiters_are_allowed() {
        assert!(hit(b"a  ", &["/a/"]));
        assert!(hit(b"a", &["/a/"]));
        assert!(!hit(b"a", &["/a /"]));
        assert!(hit(b"a ", &["/a /"]));
    }

    #[test]
    fn anycase_folds_literals_not_metacharacters() {
        assert!(!hit(b"ABC", &["/a*/"]));
        assert!(hit(b"ABC", &["ANYCASE", "/a*/"]));
        assert!(hit(b"AbC", &["CASEANY", "/a%c/"]));
        // Delimiter stays case-sensitive (p.598).
        assert!(!hit(b"aXb", &["ANYCASE", "BLANK", "X", "/a b/"]));
        assert!(hit(b"aXb", &["BLANK", "X", "/aXb/"]));
    }

    #[test]
    fn book_swap_metacharacters() {
        // p.598: `wildcard anychar off anystring % anychar * /*bcd%/`
        let args = [
            "anychar",
            "off",
            "anystring",
            "%",
            "anychar",
            "*",
            "/*bcd%/",
        ];
        assert!(hit(b"Abcd", &args));
        assert!(hit(b"Xbcdrest", &args));
        assert!(!hit(b"bcd", &args));
        assert!(!hit(b"ABcd", &args));
    }

    #[test]
    fn colliding_metacharacters_are_rejected() {
        let err = parse_wildcard(&["ANYSTRING".into(), "%".into(), "/*%*/".into()]).unwrap_err();
        assert!(err.contains("different"), "{err}");
    }

    #[test]
    fn input_range_restricts_the_match() {
        assert!(hit(b"xxabcyy", &["3-5", "/abc/"]));
        assert!(!hit(b"xxabcyy", &["1-2", "/abc/"]));
    }

    #[test]
    fn blank_off_treats_spaces_as_literals() {
        assert!(hit(b"a b", &["BLANK", "OFF", "/a b/"]));
        assert!(!hit(b"a  b", &["BLANK", "OFF", "/a b/"]));
    }
}
