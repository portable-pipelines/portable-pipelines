//! SPEC expression language (Author's Edition Ch.20 subset).
//!
//! Implements the pieces used by old CMS scripts: field identifiers, `#n`
//! counters, `IF`/`THEN`/`ELSE`/`ENDIF`, `SET`, `PRINT`, `PICTURE`, `NOWRITE`,
//! and the operators `==`, `^=` / `/=` / `\=`, `//`, `:=`, `+=`, `+`, `-`,
//! `*`, `/`, comparisons, and parentheses.
//!
//! Not implemented here: SELECT / WHEN / OTHERWISE, ELSEIF, TODCLOCK,
//! extra output streams, packed/float/date conversions, and COBOL pictures
//! beyond a run of `9` digits.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Num(i64),
    Bytes(Vec<u8>),
}

impl Value {
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Num(n) => *n != 0,
            Value::Bytes(b) => !b.is_empty() && b != b"0",
        }
    }

    pub fn as_num(&self) -> i64 {
        match self {
            Value::Num(n) => *n,
            Value::Bytes(b) => parse_number_bytes(b),
        }
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        match self {
            Value::Num(n) => n.to_string().into_bytes(),
            Value::Bytes(b) => b.clone(),
        }
    }
}

pub fn parse_number_bytes(b: &[u8]) -> i64 {
    let s = std::str::from_utf8(b).unwrap_or("").trim();
    if s.is_empty() {
        return 0;
    }
    s.parse().unwrap_or(0)
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(i64),
    Counter(usize),
    Field(String),
    Bytes(Vec<u8>),
    UnaryMinus(Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
}

#[derive(Debug, Clone)]
pub struct Assign {
    pub counter: usize,
    pub op: AssignOp,
    pub value: Expr,
}

#[derive(Debug, Default)]
pub struct SpecEnv {
    pub fields: HashMap<String, Vec<u8>>,
    pub counters: [i64; 10],
}

impl SpecEnv {
    pub fn field_bytes(&self, id: &str) -> Vec<u8> {
        self.fields.get(id).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Num(i64),
    Counter(usize),
    Ident(String),
    Bytes(Vec<u8>),
    Op(String),
}

pub fn parse_expr(input: &str) -> Result<Expr, String> {
    let toks = lex(input)?;
    let mut i = 0;
    let expr = parse_or(&toks, &mut i)?;
    if i < toks.len() {
        return Err(format!(
            "unexpected token in expression at {}",
            tok_name(&toks[i])
        ));
    }
    Ok(expr)
}

pub fn parse_assign(input: &str) -> Result<Assign, String> {
    let s = strip_outer_parens(input.trim());
    let toks = lex(s)?;
    let mut i = 0;
    let counter = match toks.get(i) {
        Some(Tok::Counter(n)) => {
            i += 1;
            *n
        }
        _ => return Err(format!("SET needs a counter such as #0, got {s}")),
    };
    let (op, negate) = match toks.get(i) {
        Some(Tok::Op(o)) if o == ":=" => {
            i += 1;
            (AssignOp::Set, false)
        }
        Some(Tok::Op(o)) if o == "+=" => {
            i += 1;
            (AssignOp::Add, false)
        }
        Some(Tok::Op(o)) if o == "-=" => {
            i += 1;
            (AssignOp::Add, true)
        }
        other => {
            return Err(format!(
                "SET supports := and +=, got {}",
                other
                    .map(tok_name)
                    .unwrap_or_else(|| "end of expression".into())
            ))
        }
    };
    let value = parse_or(&toks, &mut i)?;
    if i < toks.len() {
        return Err(format!("unexpected token in SET at {}", tok_name(&toks[i])));
    }
    let value = if negate {
        Expr::UnaryMinus(Box::new(value))
    } else {
        value
    };
    Ok(Assign { counter, op, value })
}

pub fn eval_expr(expr: &Expr, env: &SpecEnv) -> Result<Value, String> {
    match expr {
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Counter(i) => Ok(Value::Num(env.counters.get(*i).copied().unwrap_or(0))),
        Expr::Field(id) => Ok(Value::Bytes(env.field_bytes(id))),
        Expr::Bytes(b) => Ok(Value::Bytes(b.clone())),
        Expr::UnaryMinus(inner) => Ok(Value::Num(-eval_expr(inner, env)?.as_num())),
        Expr::Binary(l, op, r) => {
            let lv = eval_expr(l, env)?;
            let rv = eval_expr(r, env)?;
            match op {
                BinOp::Add => Ok(Value::Num(lv.as_num().saturating_add(rv.as_num()))),
                BinOp::Sub => Ok(Value::Num(lv.as_num().saturating_sub(rv.as_num()))),
                BinOp::Mul => Ok(Value::Num(lv.as_num().saturating_mul(rv.as_num()))),
                BinOp::Div => {
                    let d = rv.as_num();
                    if d == 0 {
                        return Err("division by zero".into());
                    }
                    Ok(Value::Num(lv.as_num() / d))
                }
                BinOp::Mod => {
                    let d = rv.as_num();
                    if d == 0 {
                        return Err("division by zero".into());
                    }
                    Ok(Value::Num(lv.as_num() % d))
                }
                BinOp::Eq => Ok(Value::Num(i64::from(values_eq(&lv, &rv)))),
                BinOp::Ne => Ok(Value::Num(i64::from(!values_eq(&lv, &rv)))),
                BinOp::Lt => Ok(Value::Num(i64::from(lv.as_num() < rv.as_num()))),
                BinOp::Gt => Ok(Value::Num(i64::from(lv.as_num() > rv.as_num()))),
                BinOp::Le => Ok(Value::Num(i64::from(lv.as_num() <= rv.as_num()))),
                BinOp::Ge => Ok(Value::Num(i64::from(lv.as_num() >= rv.as_num()))),
                BinOp::And => Ok(Value::Num(i64::from(lv.as_bool() && rv.as_bool()))),
                BinOp::Or => Ok(Value::Num(i64::from(lv.as_bool() || rv.as_bool()))),
            }
        }
    }
}

pub fn apply_assign(assign: &Assign, env: &mut SpecEnv) -> Result<(), String> {
    if assign.counter >= env.counters.len() {
        return Err(format!("counter #{} is out of range", assign.counter));
    }
    let rhs = eval_expr(&assign.value, env)?.as_num();
    match assign.op {
        AssignOp::Set => env.counters[assign.counter] = rhs,
        AssignOp::Add => {
            env.counters[assign.counter] = env.counters[assign.counter].saturating_add(rhs);
        }
    }
    Ok(())
}

/// Format a value with a simple picture: a run of `9` digits (rightmost kept).
pub fn format_picture(value: &Value, picture: &str) -> Vec<u8> {
    let width = picture.chars().filter(|&c| c == '9').count();
    if width == 0 {
        return value.as_bytes();
    }
    let n = value.as_num();
    let neg = n < 0;
    let mut body = n.unsigned_abs().to_string();
    if body.len() < width {
        body = format!("{body:0>width$}");
    } else if body.len() > width {
        body = body[body.len() - width..].to_string();
    }
    if neg {
        let mut out = vec![b'-'];
        out.extend(body.bytes());
        out
    } else {
        body.into_bytes()
    }
}

fn values_eq(l: &Value, r: &Value) -> bool {
    match (l, r) {
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        (Value::Bytes(a), Value::Num(_)) | (Value::Num(_), Value::Bytes(a))
            if !looks_numeric(a) =>
        {
            l.as_bytes() == r.as_bytes()
        }
        _ => l.as_num() == r.as_num(),
    }
}

fn looks_numeric(b: &[u8]) -> bool {
    let s = std::str::from_utf8(b).unwrap_or("").trim();
    !s.is_empty() && s.parse::<i64>().is_ok()
}

fn strip_outer_parens(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with('(') && t.ends_with(')') && balanced_wrap(t) {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

fn balanced_wrap(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'(') || bytes.last() != Some(&b')') {
        return false;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i == bytes.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}

fn lex(input: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // `//` is remainder, not an empty `/ /` dstring.
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            out.push(Tok::Op("//".into()));
            i += 2;
            continue;
        }
        if c == '"' || c == '\'' || is_dstring_delim(c) {
            let delim = c;
            if let Some(end) = chars[i + 1..].iter().position(|&ch| ch == delim) {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                out.push(Tok::Bytes(inner.into_bytes()));
                i += 2 + end;
                continue;
            }
            if c == '/' {
                out.push(Tok::Op("/".into()));
                i += 1;
                continue;
            }
        }
        if c == '#' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if start == i {
                return Err("expected a counter number after #".into());
            }
            let n: usize = chars[start..i]
                .iter()
                .collect::<String>()
                .parse()
                .map_err(|_| "counter number is out of range")?;
            if n > 9 {
                return Err(format!("counter #{n} is out of range (use #0 through #9)"));
            }
            out.push(Tok::Counter(n));
            continue;
        }
        if let Some((op, n)) = match_op(&chars, i) {
            out.push(Tok::Op(op.to_string()));
            i += n;
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let n: i64 = chars[start..i]
                .iter()
                .collect::<String>()
                .parse()
                .map_err(|_| "number is out of range")?;
            out.push(Tok::Num(n));
            continue;
        }
        if c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let id: String = chars[start..i].iter().collect();
            out.push(Tok::Ident(id.to_ascii_uppercase()));
            continue;
        }
        return Err(format!("unexpected character {c:?} in expression"));
    }
    Ok(out)
}

fn is_dstring_delim(c: char) -> bool {
    matches!(c, '/' | '%' | '$' | '!' | ';' | ',')
}

fn match_op(chars: &[char], i: usize) -> Option<(&'static str, usize)> {
    let rest: String = chars[i..].iter().collect();
    for op in [
        ":=", "+=", "-=", "*=", "==", "^=", "/=", "\\=", "<>", "<=", ">=", "//",
    ] {
        if rest.starts_with(op) {
            return Some((op, op.chars().count()));
        }
    }
    match chars[i] {
        '+' => Some(("+", 1)),
        '-' => Some(("-", 1)),
        '*' => Some(("*", 1)),
        '/' => Some(("/", 1)),
        '=' => Some(("=", 1)),
        '<' => Some(("<", 1)),
        '>' => Some((">", 1)),
        '&' => Some(("&", 1)),
        '|' => Some(("|", 1)),
        '(' => Some(("(", 1)),
        ')' => Some((")", 1)),
        _ => None,
    }
}

fn parse_or(toks: &[Tok], i: &mut usize) -> Result<Expr, String> {
    let mut left = parse_and(toks, i)?;
    while matches!(toks.get(*i), Some(Tok::Op(o)) if o == "|") {
        *i += 1;
        let right = parse_and(toks, i)?;
        left = Expr::Binary(Box::new(left), BinOp::Or, Box::new(right));
    }
    Ok(left)
}

fn parse_and(toks: &[Tok], i: &mut usize) -> Result<Expr, String> {
    let mut left = parse_cmp(toks, i)?;
    while matches!(toks.get(*i), Some(Tok::Op(o)) if o == "&") {
        *i += 1;
        let right = parse_cmp(toks, i)?;
        left = Expr::Binary(Box::new(left), BinOp::And, Box::new(right));
    }
    Ok(left)
}

fn parse_cmp(toks: &[Tok], i: &mut usize) -> Result<Expr, String> {
    let left = parse_add(toks, i)?;
    let op = match toks.get(*i) {
        Some(Tok::Op(o)) if o == "==" || o == "=" => BinOp::Eq,
        Some(Tok::Op(o)) if o == "^=" || o == "/=" || o == "\\=" || o == "<>" => BinOp::Ne,
        Some(Tok::Op(o)) if o == "<" => BinOp::Lt,
        Some(Tok::Op(o)) if o == ">" => BinOp::Gt,
        Some(Tok::Op(o)) if o == "<=" => BinOp::Le,
        Some(Tok::Op(o)) if o == ">=" => BinOp::Ge,
        _ => return Ok(left),
    };
    *i += 1;
    let right = parse_add(toks, i)?;
    Ok(Expr::Binary(Box::new(left), op, Box::new(right)))
}

fn parse_add(toks: &[Tok], i: &mut usize) -> Result<Expr, String> {
    let mut left = parse_mul(toks, i)?;
    loop {
        let op = match toks.get(*i) {
            Some(Tok::Op(o)) if o == "+" => BinOp::Add,
            Some(Tok::Op(o)) if o == "-" => BinOp::Sub,
            _ => break,
        };
        *i += 1;
        let right = parse_mul(toks, i)?;
        left = Expr::Binary(Box::new(left), op, Box::new(right));
    }
    Ok(left)
}

fn parse_mul(toks: &[Tok], i: &mut usize) -> Result<Expr, String> {
    let mut left = parse_unary(toks, i)?;
    loop {
        let op = match toks.get(*i) {
            Some(Tok::Op(o)) if o == "*" => BinOp::Mul,
            Some(Tok::Op(o)) if o == "/" => BinOp::Div,
            Some(Tok::Op(o)) if o == "//" => BinOp::Mod,
            _ => break,
        };
        *i += 1;
        let right = parse_unary(toks, i)?;
        left = Expr::Binary(Box::new(left), op, Box::new(right));
    }
    Ok(left)
}

fn parse_unary(toks: &[Tok], i: &mut usize) -> Result<Expr, String> {
    match toks.get(*i) {
        Some(Tok::Op(o)) if o == "-" => {
            *i += 1;
            Ok(Expr::UnaryMinus(Box::new(parse_unary(toks, i)?)))
        }
        Some(Tok::Op(o)) if o == "+" => {
            *i += 1;
            parse_unary(toks, i)
        }
        _ => parse_primary(toks, i),
    }
}

fn parse_primary(toks: &[Tok], i: &mut usize) -> Result<Expr, String> {
    match toks.get(*i) {
        Some(Tok::Num(n)) => {
            *i += 1;
            Ok(Expr::Num(*n))
        }
        Some(Tok::Counter(n)) => {
            *i += 1;
            Ok(Expr::Counter(*n))
        }
        Some(Tok::Ident(s)) => {
            *i += 1;
            Ok(Expr::Field(s.clone()))
        }
        Some(Tok::Bytes(b)) => {
            *i += 1;
            Ok(Expr::Bytes(b.clone()))
        }
        Some(Tok::Op(o)) if o == "(" => {
            *i += 1;
            let inner = parse_or(toks, i)?;
            match toks.get(*i) {
                Some(Tok::Op(c)) if c == ")" => {
                    *i += 1;
                    Ok(inner)
                }
                _ => Err("missing ) in expression".into()),
            }
        }
        Some(other) => Err(format!(
            "unexpected token in expression: {}",
            tok_name(other)
        )),
        None => Err("incomplete expression".into()),
    }
}

fn tok_name(t: &Tok) -> String {
    match t {
        Tok::Num(n) => n.to_string(),
        Tok::Counter(n) => format!("#{n}"),
        Tok::Ident(s) => s.clone(),
        Tok::Bytes(_) => "string".into(),
        Tok::Op(o) => o.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(a: &str, n0: i64) -> SpecEnv {
        let mut env = SpecEnv::default();
        env.fields.insert("A".into(), a.as_bytes().to_vec());
        env.counters[0] = n0;
        env
    }

    #[test]
    fn not_equal_and_mod() {
        let env = env_with(" ", 27);
        let e = parse_expr("0^=(#0//10)").unwrap();
        assert_eq!(eval_expr(&e, &env).unwrap(), Value::Num(1));
        let env = env_with(" ", 20);
        assert_eq!(eval_expr(&e, &env).unwrap(), Value::Num(0));
    }

    #[test]
    fn field_eq_space() {
        let env = env_with(" ", 0);
        let e = parse_expr(r#"(a==" ")"#).unwrap();
        assert!(eval_expr(&e, &env).unwrap().as_bool());
        let env = env_with("5", 0);
        assert!(!eval_expr(&e, &env).unwrap().as_bool());
    }

    #[test]
    fn set_add_and_assign() {
        let mut env = env_with("7", 3);
        apply_assign(&parse_assign("(#0+=a)").unwrap(), &mut env).unwrap();
        assert_eq!(env.counters[0], 10);
        apply_assign(&parse_assign("#0:=0").unwrap(), &mut env).unwrap();
        assert_eq!(env.counters[0], 0);
    }

    #[test]
    fn picture_one_digit() {
        assert_eq!(format_picture(&Value::Num(1), "9"), b"1");
        assert_eq!(format_picture(&Value::Num(0), "9"), b"0");
        assert_eq!(format_picture(&Value::Num(12), "9"), b"2");
        assert_eq!(format_picture(&Value::Num(3), "99"), b"03");
        assert_eq!(format_picture(&Value::Num(7), "999"), b"007");
        assert_eq!(format_picture(&Value::Num(1234), "99"), b"34");
    }

    #[test]
    fn division_and_modulo_by_zero() {
        let env = SpecEnv::default();
        let div = parse_expr("8/0").unwrap();
        assert_eq!(eval_expr(&div, &env).unwrap_err(), "division by zero");
        let modulo = parse_expr("8//0").unwrap();
        assert_eq!(eval_expr(&modulo, &env).unwrap_err(), "division by zero");
    }

    #[test]
    fn nested_comparison_in_expression() {
        let env = env_with("5", 20);
        let e = parse_expr("0/=(#0//10)").unwrap();
        assert_eq!(eval_expr(&e, &env).unwrap(), Value::Num(0));
        let e = parse_expr("(a==\"5\")").unwrap();
        assert!(eval_expr(&e, &env).unwrap().as_bool());
    }

    #[test]
    fn bad_set_syntax() {
        assert!(parse_assign("nope").is_err());
        assert!(parse_assign("#0").is_err());
        let err = parse_expr("1+").unwrap_err();
        assert!(!err.is_empty(), "{err}");
    }

    #[test]
    fn counters_zero_through_nine_parse() {
        for n in 0..=9 {
            parse_expr(&format!("#{n}")).unwrap();
        }
        let err = parse_expr("#10").unwrap_err();
        assert!(err.contains("out of range"), "{err}");
    }
}
