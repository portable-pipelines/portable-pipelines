//! Normalize CMS REXX not-equal operators for Regina.
//!
//! Old CMS REXX accepts `/=` and `^=` as "not equal". Regina on Linux wants
//! `\=`. This rewrite is applied to stage source before Regina (and the
//! subset interpreter) sees it. Quoted strings and `/* */` comments are left
//! alone.

/// Replace CMS `/=` and `^=` with Regina `\=` outside strings and comments.
pub fn normalize_cms_rexx_operators(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut out = String::with_capacity(source.len());
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            out.push('/');
            out.push('*');
            i += 2;
            while i < chars.len() {
                let c2 = chars[i];
                out.push(c2);
                i += 1;
                if c2 == '*' && chars.get(i) == Some(&'/') {
                    out.push('/');
                    i += 1;
                    break;
                }
            }
            continue;
        }
        if c == '\'' || c == '"' {
            let q = c;
            out.push(q);
            i += 1;
            while i < chars.len() {
                let c2 = chars[i];
                out.push(c2);
                i += 1;
                if c2 == q {
                    if chars.get(i) == Some(&q) {
                        out.push(q);
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            continue;
        }
        if (c == '/' || c == '^') && chars.get(i + 1) == Some(&'=') {
            out.push('\\');
            out.push('=');
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_operators_outside_strings() {
        assert_eq!(
            normalize_cms_rexx_operators("if a /= b then"),
            "if a \\= b then"
        );
        assert_eq!(
            normalize_cms_rexx_operators("if a ^= b then"),
            "if a \\= b then"
        );
        assert_eq!(
            normalize_cms_rexx_operators("if a /= b then say c ^= d"),
            "if a \\= b then say c \\= d"
        );
    }

    #[test]
    fn leaves_strings_and_comments_alone() {
        assert_eq!(normalize_cms_rexx_operators("say 'a /= b'"), "say 'a /= b'");
        assert_eq!(
            normalize_cms_rexx_operators("say \"a ^= b\""),
            "say \"a ^= b\""
        );
        assert_eq!(
            normalize_cms_rexx_operators("/* a /= b */ x /= y"),
            "/* a /= b */ x \\= y"
        );
        assert_eq!(
            normalize_cms_rexx_operators("say 'it''s /= fine'"),
            "say 'it''s /= fine'"
        );
    }

    #[test]
    fn leaves_division_and_equals_apart() {
        assert_eq!(normalize_cms_rexx_operators("x = y / z"), "x = y / z");
    }

    #[test]
    fn leaves_regina_not_equal_unchanged() {
        assert_eq!(
            normalize_cms_rexx_operators("if a \\= b then"),
            "if a \\= b then"
        );
        assert_eq!(
            normalize_cms_rexx_operators("if a \\= b then say c /= d"),
            "if a \\= b then say c \\= d"
        );
    }

    #[test]
    fn rewrites_inside_nested_expressions() {
        assert_eq!(
            normalize_cms_rexx_operators("if (a /= b) & (c ^= d) then"),
            "if (a \\= b) & (c \\= d) then"
        );
        assert_eq!(
            normalize_cms_rexx_operators("x = (n /= 0)"),
            "x = (n \\= 0)"
        );
    }

    #[test]
    fn leaves_operators_inside_comments_and_quotes() {
        assert_eq!(
            normalize_cms_rexx_operators("/* a /= b */ still /= x"),
            "/* a /= b */ still \\= x"
        );
        assert_eq!(
            normalize_cms_rexx_operators("say 'a /= \"b ^= c\" d'"),
            "say 'a /= \"b ^= c\" d'"
        );
        assert_eq!(
            normalize_cms_rexx_operators("say \"keep /= this\" /* and /= this */ x ^= y"),
            "say \"keep /= this\" /* and /= this */ x \\= y"
        );
    }
}
