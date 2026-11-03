//! JSON object records and `$name` / `$a.b` field paths.
//!
//! Experimental portable extension. A record carries JSON as its byte payload:
//! one object per record. Records that are not a JSON object stay ordinary
//! bytes. SPEC writes plain text, so the JSON shape is only present while a
//! later stage still wants it.

use serde_json::{Map, Number, Value};

/// Parse record bytes as a JSON object.
pub fn parse_json_object(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "record is not a JSON object".to_string())?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("record is not a JSON object".into());
    }
    let value: Value =
        serde_json::from_str(trimmed).map_err(|_| "record is not a JSON object".to_string())?;
    if !value.is_object() {
        return Err("record is not a JSON object".into());
    }
    Ok(value)
}

/// Compact JSON for an object (no extra spaces).
pub fn write_json_object(value: &Value) -> Result<String, String> {
    if !value.is_object() {
        return Err("record is not a JSON object".into());
    }
    serde_json::to_string(value).map_err(|e| format!("cannot write JSON ({e})"))
}

/// `$name` or `$a.b.c` (ASCII identifier segments).
pub fn parse_json_field_ref(token: &str) -> Option<Vec<String>> {
    let rest = token.strip_prefix('$')?;
    parse_field_path(rest)
}

/// `name` or `a.b.c` without a leading `$`.
pub fn parse_field_path(path: &str) -> Option<Vec<String>> {
    if path.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for seg in path.split('.') {
        if !is_field_ident(seg) {
            return None;
        }
        parts.push(seg.to_string());
    }
    Some(parts)
}

fn is_field_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Walk `path` in a JSON object. `Ok(None)` when a key is missing.
/// `Err` when a parent is not an object (or the record is not an object).
pub fn lookup_json_path<'a>(
    value: &'a Value,
    path: &[String],
) -> Result<Option<&'a Value>, String> {
    if path.is_empty() {
        return Ok(Some(value));
    }
    let mut cur = value;
    for (i, key) in path.iter().enumerate() {
        let obj = cur.as_object().ok_or_else(|| {
            if i == 0 {
                "record is not a JSON object".to_string()
            } else {
                format!("${} is not a JSON object", path[..i].join("."))
            }
        })?;
        match obj.get(key) {
            Some(next) => cur = next,
            None => return Ok(None),
        }
    }
    Ok(Some(cur))
}

pub fn lookup_json_field<'a>(record: &'a [u8], path: &[String]) -> Result<Option<Value>, String> {
    let value = parse_json_object(record)?;
    Ok(lookup_json_path(&value, path)?.cloned())
}

/// Bytes SPEC / SUBSTRING write for a JSON value.
///
/// Strings are unquoted. Numbers use the JSON number spelling. Objects and
/// arrays stay compact JSON. `null` is empty. Booleans are `true` / `false`.
pub fn display_json_value(value: &Value) -> Vec<u8> {
    match value {
        Value::Null => Vec::new(),
        Value::Bool(b) => {
            if *b {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }
        }
        Value::Number(n) => n.to_string().into_bytes(),
        Value::String(s) => s.as_bytes().to_vec(),
        other => serde_json::to_string(other)
            .unwrap_or_else(|_| other.to_string())
            .into_bytes(),
    }
}

pub fn extract_json_field_bytes(record: &[u8], path: &[String]) -> Vec<u8> {
    match lookup_json_field(record, path) {
        Ok(Some(v)) => display_json_value(&v),
        Ok(None) | Err(_) => Vec::new(),
    }
}

/// Insert or replace `path`. Intermediate objects are created when `create`
/// is true. `add` uses `create` and fails if the leaf already exists.
pub fn set_json_path(
    value: &mut Value,
    path: &[String],
    new_val: Value,
    replace: bool,
) -> Result<(), String> {
    if path.is_empty() {
        return Err("a field name is required".into());
    }
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "record is not a JSON object".to_string())?;
    set_in_map(obj, path, new_val, replace)
}

fn set_in_map(
    map: &mut Map<String, Value>,
    path: &[String],
    new_val: Value,
    replace: bool,
) -> Result<(), String> {
    let (head, rest) = path.split_first().expect("path not empty");
    if rest.is_empty() {
        if !replace && map.contains_key(head) {
            return Err(format!(
                "field {head} already exists; use SET to replace it"
            ));
        }
        map.insert(head.clone(), new_val);
        return Ok(());
    }
    let entry = map
        .entry(head.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    let child = entry
        .as_object_mut()
        .ok_or_else(|| format!("${head} is not a JSON object"))?;
    set_in_map(child, rest, new_val, replace)
}

/// Delete a path. Missing leaf is a no-op (`Ok(false)`).
pub fn delete_json_path(value: &mut Value, path: &[String]) -> Result<bool, String> {
    if path.is_empty() {
        return Err("a field name is required".into());
    }
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "record is not a JSON object".to_string())?;
    delete_in_map(obj, path)
}

fn delete_in_map(map: &mut Map<String, Value>, path: &[String]) -> Result<bool, String> {
    let (head, rest) = path.split_first().expect("path not empty");
    if rest.is_empty() {
        return Ok(map.remove(head).is_some());
    }
    match map.get_mut(head) {
        None => Ok(false),
        Some(child) => {
            let obj = child
                .as_object_mut()
                .ok_or_else(|| format!("${head} is not a JSON object"))?;
            delete_in_map(obj, rest)
        }
    }
}

pub fn keep_json_fields(value: &mut Value, names: &[String]) -> Result<(), String> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "record is not a JSON object".to_string())?;
    let keep: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
    obj.retain(|k, _| keep.contains(k.as_str()));
    Ok(())
}

/// Evaluate `json add` / `json set` expressions.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Field(Vec<String>),
    Number(String),
    String(String),
    Plus,
    Minus,
    Star,
    Slash,
    Concat,
    LParen,
    RParen,
}

pub fn eval_json_expr(record: &Value, args: &[String]) -> Result<Value, String> {
    let text = args.join(" ");
    if text.trim().is_empty() {
        return Err("an expression is required after =".into());
    }
    let tokens = lex_expr(&text)?;
    let mut i = 0;
    let value = parse_concat(record, &tokens, &mut i)?;
    if i != tokens.len() {
        return Err("unexpected text in expression".into());
    }
    Ok(value)
}

fn lex_expr(text: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '$' {
            let start = i;
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                    i += 1;
                } else {
                    break;
                }
            }
            let token: String = chars[start..i].iter().collect();
            let path = parse_json_field_ref(&token)
                .ok_or_else(|| format!("not a field reference: {token}"))?;
            out.push(Tok::Field(path));
            continue;
        }
        if c == '|' {
            // Pipeline specs write || for a literal bar (self-escape), so the
            // expression usually sees a single |. Accept both.
            if chars.get(i + 1) == Some(&'|') {
                i += 2;
            } else {
                i += 1;
            }
            out.push(Tok::Concat);
            continue;
        }
        match c {
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                // Division. String literals in expressions use "..." or another
                // IBM delimiter, not /.../, so $qty / $price stays arithmetic.
                out.push(Tok::Slash);
                i += 1;
            }
            '"' | '\'' | '%' | '#' | '!' | ';' | ',' => {
                let (s, ni) = take_dstring(&chars, i)?;
                out.push(Tok::String(s));
                i = ni;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ch if ch.is_ascii_digit()
                || (ch == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) =>
            {
                let start = i;
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i < chars.len() && chars[i] == '.' {
                    let next_digit = chars.get(i + 1).is_some_and(|d| d.is_ascii_digit());
                    if next_digit {
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    let mut j = i + 1;
                    if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                        j += 1;
                    }
                    if j < chars.len() && chars[j].is_ascii_digit() {
                        i = j;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                }
                out.push(Tok::Number(chars[start..i].iter().collect()));
            }
            _ => {
                return Err(format!("unexpected {c} in expression"));
            }
        }
    }
    Ok(out)
}

fn take_dstring(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let delim = chars[start];
    let mut i = start + 1;
    while i < chars.len() {
        if chars[i] == delim {
            let inner: String = chars[start + 1..i].iter().collect();
            return Ok((inner, i + 1));
        }
        i += 1;
    }
    Err("unclosed string in expression".into())
}

fn parse_concat(record: &Value, tokens: &[Tok], i: &mut usize) -> Result<Value, String> {
    let mut left = parse_add(record, tokens, i)?;
    while matches!(tokens.get(*i), Some(Tok::Concat)) {
        *i += 1;
        let right = parse_add(record, tokens, i)?;
        left = Value::String(format!(
            "{}{}",
            stringify_concat(&left),
            stringify_concat(&right)
        ));
    }
    Ok(left)
}

fn parse_add(record: &Value, tokens: &[Tok], i: &mut usize) -> Result<Value, String> {
    let mut left = parse_mul(record, tokens, i)?;
    loop {
        match tokens.get(*i) {
            Some(Tok::Plus) => {
                *i += 1;
                let right = parse_mul(record, tokens, i)?;
                if is_stringy(&left) || is_stringy(&right) {
                    left = Value::String(format!(
                        "{}{}",
                        stringify_concat(&left),
                        stringify_concat(&right)
                    ));
                } else {
                    left = num_binop(&left, &right, '+')?;
                }
            }
            Some(Tok::Minus) => {
                *i += 1;
                let right = parse_mul(record, tokens, i)?;
                left = num_binop(&left, &right, '-')?;
            }
            _ => return Ok(left),
        }
    }
}

fn parse_mul(record: &Value, tokens: &[Tok], i: &mut usize) -> Result<Value, String> {
    let mut left = parse_unary(record, tokens, i)?;
    loop {
        match tokens.get(*i) {
            Some(Tok::Star) => {
                *i += 1;
                let right = parse_unary(record, tokens, i)?;
                left = num_binop(&left, &right, '*')?;
            }
            Some(Tok::Slash) => {
                *i += 1;
                let right = parse_unary(record, tokens, i)?;
                left = num_binop(&left, &right, '/')?;
            }
            _ => return Ok(left),
        }
    }
}

fn parse_unary(record: &Value, tokens: &[Tok], i: &mut usize) -> Result<Value, String> {
    match tokens.get(*i) {
        Some(Tok::Minus) => {
            *i += 1;
            let v = parse_unary(record, tokens, i)?;
            num_neg(&v)
        }
        Some(Tok::Plus) => {
            *i += 1;
            parse_unary(record, tokens, i)
        }
        _ => parse_primary(record, tokens, i),
    }
}

fn parse_primary(record: &Value, tokens: &[Tok], i: &mut usize) -> Result<Value, String> {
    let tok = tokens
        .get(*i)
        .ok_or_else(|| "expression ended early".to_string())?;
    match tok {
        Tok::LParen => {
            *i += 1;
            let v = parse_concat(record, tokens, i)?;
            if !matches!(tokens.get(*i), Some(Tok::RParen)) {
                return Err("missing ) in expression".into());
            }
            *i += 1;
            Ok(v)
        }
        Tok::Field(path) => {
            *i += 1;
            match lookup_json_path(record, path)? {
                Some(v) => Ok(v.clone()),
                None => Err(format!("field ${} is missing", path.join("."))),
            }
        }
        Tok::Number(s) => {
            *i += 1;
            parse_number_token(s)
        }
        Tok::String(s) => {
            *i += 1;
            Ok(Value::String(s.clone()))
        }
        _ => Err("unexpected token in expression".into()),
    }
}

fn parse_number_token(s: &str) -> Result<Value, String> {
    if let Ok(i) = s.parse::<i64>() {
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            return Ok(Value::from(i));
        }
    }
    if let Ok(u) = s.parse::<u64>() {
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            return Ok(Value::from(u));
        }
    }
    let f: f64 = s.parse().map_err(|_| format!("not a number: {s}"))?;
    Number::from_f64(f)
        .map(Value::Number)
        .ok_or_else(|| format!("not a number: {s}"))
}

fn is_stringy(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Null | Value::Bool(_))
}

fn stringify_concat(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn as_f64(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => n
            .as_f64()
            .ok_or_else(|| "number is not a finite floating-point value".to_string()),
        Value::String(s) => s.parse().map_err(|_| format!("not a number: {s}")),
        _ => Err("expression operand is not a number".into()),
    }
}

fn both_integers(a: &Value, b: &Value) -> Option<(i64, i64)> {
    Some((as_i64(a)?, as_i64(b)?))
}

fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

fn num_neg(v: &Value) -> Result<Value, String> {
    if let Some(i) = as_i64(v) {
        return Ok(Value::from(-i));
    }
    let f = as_f64(v)?;
    value_from_f64(-f)
}

fn num_binop(left: &Value, right: &Value, op: char) -> Result<Value, String> {
    if op != '/' {
        if let Some((a, b)) = both_integers(left, right) {
            let r = match op {
                '+' => a.checked_add(b),
                '-' => a.checked_sub(b),
                '*' => a.checked_mul(b),
                _ => None,
            };
            if let Some(i) = r {
                return Ok(Value::from(i));
            }
        }
    }
    let a = as_f64(left)?;
    let b = as_f64(right)?;
    let r = match op {
        '+' => a + b,
        '-' => a - b,
        '*' => a * b,
        '/' => {
            if b == 0.0 {
                return Err("division by zero".into());
            }
            a / b
        }
        _ => return Err("unknown operator".into()),
    };
    value_from_f64(r)
}

fn value_from_f64(n: f64) -> Result<Value, String> {
    if !n.is_finite() {
        return Err("numeric result is not finite".into());
    }
    if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
        return Ok(Value::from(n as i64));
    }
    Number::from_f64(n)
        .map(Value::Number)
        .ok_or_else(|| "numeric result is not finite".to_string())
}

/// CSV number rule: optional sign, `0` or `[1-9][0-9]*`, optional `.` digits,
/// optional `e`/`E` exponent. Leading zeros such as `007` stay strings.
pub fn csv_detect_number(s: &str) -> Option<Number> {
    if !looks_like_csv_number(s) {
        return None;
    }
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        if let Ok(i) = s.parse::<i64>() {
            return Some(Number::from(i));
        }
        if let Ok(u) = s.parse::<u64>() {
            return Some(Number::from(u));
        }
    }
    let f: f64 = s.parse().ok()?;
    Number::from_f64(f)
}

fn looks_like_csv_number(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
        if i == bytes.len() {
            return false;
        }
    }
    if bytes[i] == b'.' {
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    } else if bytes[i] == b'0' {
        i += 1;
        if i < bytes.len() && bytes[i].is_ascii_digit() {
            return false;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return false;
            }
        }
    } else if bytes[i].is_ascii_digit() {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return false;
            }
        }
    } else {
        return false;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_object_and_reject_plain_text() {
        let v = parse_json_object(br#"{"a":1}"#).unwrap();
        assert_eq!(v["a"], 1);
        assert!(parse_json_object(b"hello").is_err());
        assert!(parse_json_object(b"[1,2]").is_err());
        assert!(parse_json_object(b"").is_err());
    }

    #[test]
    fn nested_lookup() {
        let v = parse_json_object(br#"{"user":{"name":"Ann"}}"#).unwrap();
        let path = parse_json_field_ref("$user.name").unwrap();
        assert_eq!(
            lookup_json_path(&v, &path).unwrap().unwrap().as_str(),
            Some("Ann")
        );
        let missing = parse_json_field_ref("$user.city").unwrap();
        assert!(lookup_json_path(&v, &missing).unwrap().is_none());
    }

    #[test]
    fn display_preserves_types() {
        assert_eq!(display_json_value(&Value::from(120)), b"120");
        assert_eq!(display_json_value(&Value::String("Ann".into())), b"Ann");
        assert_eq!(display_json_value(&Value::Null), b"");
    }

    #[test]
    fn expr_multiply_and_concat() {
        let rec = parse_json_object(br#"{"qty":3,"price":40,"name":"Ann"}"#).unwrap();
        let total = eval_json_expr(&rec, &["$qty".into(), "*".into(), "$price".into()]).unwrap();
        assert_eq!(total, Value::from(120));
        let greet = eval_json_expr(&rec, &["\"hi \"".into(), "+".into(), "$name".into()]).unwrap();
        assert_eq!(greet, Value::String("hi Ann".into()));
    }

    #[test]
    fn csv_number_rule() {
        assert!(csv_detect_number("3").is_some());
        assert!(csv_detect_number("-12").is_some());
        assert!(csv_detect_number("0.5").is_some());
        assert!(csv_detect_number("1e6").is_some());
        assert!(csv_detect_number("007").is_none());
        assert!(csv_detect_number("").is_none());
        assert!(csv_detect_number("East").is_none());
        assert!(csv_detect_number(" 3").is_none());
    }

    #[test]
    fn field_ref_tokens() {
        assert_eq!(
            parse_json_field_ref("$qty").as_deref(),
            Some(["qty".to_string()].as_slice())
        );
        assert_eq!(
            parse_json_field_ref("$a.b").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(parse_json_field_ref("$").is_none());
        assert!(parse_json_field_ref("qty").is_none());
        assert!(parse_json_field_ref("$1ab").is_none());
    }
}
