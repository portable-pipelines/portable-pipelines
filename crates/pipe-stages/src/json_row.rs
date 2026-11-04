//! JSON object records and `$name` / `$a.b` / `$items.0` field paths.
//!
//! Experimental portable extension. A record carries JSON as its byte payload:
//! one object per record. Records that are not a JSON object stay ordinary
//! bytes. SPEC writes plain text, so the JSON shape is only present while a
//! later stage still wants it. `JSON ARRAY` turns a top-level array into
//! object records. `JSON SPLIT` expands an array field.

use std::fmt;

use serde_json::{Map, Number, Value};

/// One step in `$items.0.sku` / `$items[0].sku`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldSeg {
    Key(String),
    /// 0-based. Negative counts from the end (`-1` is last).
    Index(i64),
}

impl fmt::Display for FieldSeg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldSeg::Key(k) => f.write_str(k),
            FieldSeg::Index(i) => write!(f, "{i}"),
        }
    }
}

/// Primary spelling: `items.0.sku` (dot plus integer).
pub fn format_path(path: &[FieldSeg]) -> String {
    path.iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

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

/// `$name`, `$a.b`, `$items.0.sku`, or `$items[0].sku`.
pub fn parse_json_field_ref(token: &str) -> Option<Vec<FieldSeg>> {
    let rest = token.strip_prefix('$')?;
    let (path, n) = parse_field_path_prefix(rest)?;
    if n != rest.len() || path.is_empty() {
        return None;
    }
    Some(path)
}

/// `name`, `a.b`, `items.0.sku`, or `items[0].sku` without a leading `$`.
pub fn parse_field_path(path: &str) -> Option<Vec<FieldSeg>> {
    let (parts, n) = parse_field_path_prefix(path)?;
    if n != path.len() || parts.is_empty() {
        return None;
    }
    Some(parts)
}

/// Parse a field path at the start of `s`. Returns the path and the number of
/// bytes consumed. Primary index spelling is dot plus integer (`items.0`).
/// Brackets (`items[0]`) are also accepted. No spaces.
fn parse_field_path_prefix(s: &str) -> Option<(Vec<FieldSeg>, usize)> {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let mut i = 0usize;
    let mut parts = Vec::new();
    let (key, ni) = take_ident(&chars, i)?;
    parts.push(FieldSeg::Key(key));
    i = ni;
    while i < chars.len() {
        if chars[i] == '.' {
            i += 1;
            if i >= chars.len() {
                return None;
            }
            if chars[i].is_ascii_digit() || chars[i] == '-' {
                let (idx, ni) = take_index_number(&chars, i)?;
                parts.push(FieldSeg::Index(idx));
                i = ni;
            } else {
                let (key, ni) = take_ident(&chars, i)?;
                parts.push(FieldSeg::Key(key));
                i = ni;
            }
        } else if chars[i] == '[' {
            i += 1;
            let (idx, ni) = take_index_number(&chars, i)?;
            i = ni;
            if i >= chars.len() || chars[i] != ']' {
                return None;
            }
            i += 1;
            parts.push(FieldSeg::Index(idx));
        } else {
            break;
        }
    }
    let bytes = chars[..i].iter().collect::<String>().len();
    Some((parts, bytes))
}

fn take_ident(chars: &[char], start: usize) -> Option<(String, usize)> {
    let first = *chars.get(start)?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut i = start + 1;
    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
        i += 1;
    }
    Some((chars[start..i].iter().collect(), i))
}

fn take_index_number(chars: &[char], start: usize) -> Option<(i64, usize)> {
    let mut i = start;
    if i < chars.len() && chars[i] == '-' {
        i += 1;
    }
    let digits_at = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_at {
        return None;
    }
    let s: String = chars[start..i].iter().collect();
    let n = s.parse::<i64>().ok()?;
    Some((n, i))
}

/// Walk `path` in a JSON value. `Ok(None)` when a key is missing, an index is
/// out of range, or an index is applied to a non-array. `Err` when a key is
/// applied to a non-object (or the record is not an object).
pub fn lookup_json_path<'a>(
    value: &'a Value,
    path: &[FieldSeg],
) -> Result<Option<&'a Value>, String> {
    if path.is_empty() {
        return Ok(Some(value));
    }
    let mut cur = value;
    for (i, seg) in path.iter().enumerate() {
        match seg {
            FieldSeg::Key(key) => {
                let obj = cur.as_object().ok_or_else(|| {
                    if i == 0 {
                        "record is not a JSON object".to_string()
                    } else {
                        format!("${} is not a JSON object", format_path(&path[..i]))
                    }
                })?;
                match obj.get(key) {
                    Some(next) => cur = next,
                    None => return Ok(None),
                }
            }
            FieldSeg::Index(idx) => {
                let Some(arr) = cur.as_array() else {
                    return Ok(None);
                };
                match index_array(arr, *idx) {
                    Some(next) => cur = next,
                    None => return Ok(None),
                }
            }
        }
    }
    Ok(Some(cur))
}

pub fn lookup_json_field(record: &[u8], path: &[FieldSeg]) -> Result<Option<Value>, String> {
    let value = parse_json_object(record)?;
    Ok(lookup_json_path(&value, path)?.cloned())
}

fn index_array(arr: &[Value], idx: i64) -> Option<&Value> {
    let i = resolve_array_index(arr.len(), idx)?;
    arr.get(i)
}

fn resolve_array_index(len: usize, idx: i64) -> Option<usize> {
    let j = (len as i128) + (idx as i128);
    let j = if idx >= 0 { idx as i128 } else { j };
    if j < 0 || j >= len as i128 {
        None
    } else {
        Some(j as usize)
    }
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

pub fn extract_json_field_bytes(record: &[u8], path: &[FieldSeg]) -> Vec<u8> {
    match lookup_json_field(record, path) {
        Ok(Some(v)) => display_json_value(&v),
        Ok(None) | Err(_) => Vec::new(),
    }
}

/// Wrap a non-object array element as `{"value": x}` so it can be a record.
pub fn object_record_from_element(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        other => {
            let mut map = Map::new();
            map.insert("value".into(), other);
            Value::Object(map)
        }
    }
}

/// Insert or replace `path`. Intermediate objects are created for missing
/// keys when the next step is a key. Arrays are not grown; a missing array
/// or out-of-range index is an error. `add` fails if the leaf already exists.
pub fn set_json_path(
    value: &mut Value,
    path: &[FieldSeg],
    new_val: Value,
    replace: bool,
) -> Result<(), String> {
    if path.is_empty() {
        return Err("a field name is required".into());
    }
    set_in_value(value, path, new_val, replace, 0)
}

fn set_in_value(
    cur: &mut Value,
    path: &[FieldSeg],
    new_val: Value,
    replace: bool,
    depth: usize,
) -> Result<(), String> {
    let (head, rest) = path.split_first().expect("path not empty");
    match head {
        FieldSeg::Key(key) => {
            if !cur.is_object() {
                return Err(if depth == 0 {
                    "record is not a JSON object".into()
                } else {
                    format!("${} is not a JSON object", key)
                });
            }
            let map = cur.as_object_mut().expect("object");
            if rest.is_empty() {
                if !replace && map.contains_key(key) {
                    return Err(format!("field {key} already exists; use SET to replace it"));
                }
                map.insert(key.clone(), new_val);
                return Ok(());
            }
            if !map.contains_key(key) {
                if matches!(rest.first(), Some(FieldSeg::Index(_))) {
                    return Err(format!("${key} is not a JSON array"));
                }
                map.insert(key.clone(), Value::Object(Map::new()));
            }
            let child = map.get_mut(key).expect("just inserted or present");
            set_in_value(child, rest, new_val, replace, depth + 1)
        }
        FieldSeg::Index(idx) => {
            let arr = cur.as_array_mut().ok_or_else(|| {
                if depth == 0 {
                    "record is not a JSON array".to_string()
                } else {
                    format!("index {idx} is not a JSON array")
                }
            })?;
            let i = resolve_array_index(arr.len(), *idx)
                .ok_or_else(|| format!("index {idx} is out of range"))?;
            if rest.is_empty() {
                if !replace {
                    return Err("field already exists; use SET to replace it".into());
                }
                arr[i] = new_val;
                return Ok(());
            }
            set_in_value(&mut arr[i], rest, new_val, replace, depth + 1)
        }
    }
}

/// Delete a path. Missing leaf is a no-op (`Ok(false)`). An index removes
/// that element (later elements shift down).
pub fn delete_json_path(value: &mut Value, path: &[FieldSeg]) -> Result<bool, String> {
    if path.is_empty() {
        return Err("a field name is required".into());
    }
    delete_in_value(value, path, 0)
}

fn delete_in_value(cur: &mut Value, path: &[FieldSeg], depth: usize) -> Result<bool, String> {
    let (head, rest) = path.split_first().expect("path not empty");
    match head {
        FieldSeg::Key(key) => {
            if rest.is_empty() {
                let map = cur.as_object_mut().ok_or_else(|| {
                    if depth == 0 {
                        "record is not a JSON object".to_string()
                    } else {
                        format!("${key} is not a JSON object")
                    }
                })?;
                return Ok(map.remove(key).is_some());
            }
            match cur.as_object_mut().and_then(|m| m.get_mut(key)) {
                None => Ok(false),
                Some(child) => delete_in_value(child, rest, depth + 1),
            }
        }
        FieldSeg::Index(idx) => {
            let arr = match cur.as_array_mut() {
                Some(a) => a,
                None => {
                    if depth == 0 {
                        return Err("record is not a JSON array".into());
                    }
                    return Ok(false);
                }
            };
            let Some(i) = resolve_array_index(arr.len(), *idx) else {
                return Ok(false);
            };
            if rest.is_empty() {
                arr.remove(i);
                return Ok(true);
            }
            delete_in_value(&mut arr[i], rest, depth + 1)
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
    Field(Vec<FieldSeg>),
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
            let rest: String = chars[i + 1..].iter().collect();
            let (path, n) = parse_field_path_prefix(&rest)
                .filter(|(_, n)| *n > 0)
                .ok_or_else(|| {
                    let token: String = chars[i..]
                        .iter()
                        .take_while(|ch| !ch.is_whitespace())
                        .collect();
                    format!("not a field reference: {token}")
                })?;
            i = i + 1 + rest[..n].chars().count();
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
                // After a value, `/` is division so `$qty / $price` stays
                // arithmetic. Otherwise `/.../` is a string, the same IBM
                // delimiter SPEC uses (`json add $shout = $city + /!/`).
                if slash_starts_string(out.last()) {
                    let (s, ni) = take_dstring(&chars, i)?;
                    out.push(Tok::String(s));
                    i = ni;
                } else {
                    out.push(Tok::Slash);
                    i += 1;
                }
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

/// `/.../` is a string unless the previous token is a value (`$qty / $price`).
fn slash_starts_string(prev: Option<&Tok>) -> bool {
    match prev {
        None | Some(Tok::Plus) | Some(Tok::Minus) | Some(Tok::Star) | Some(Tok::Slash)
        | Some(Tok::Concat) | Some(Tok::LParen) => true,
        Some(Tok::Field(_)) | Some(Tok::Number(_)) | Some(Tok::String(_)) | Some(Tok::RParen) => {
            false
        }
    }
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
                None => Err(format!("field ${} is missing", format_path(path))),
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

/// Overlay `extra` keys onto `base` (extra wins). Both must be objects.
pub fn merge_json_objects(base: Value, extra: Value) -> Result<Value, String> {
    let Value::Object(mut map) = base else {
        return Err("record is not a JSON object".into());
    };
    let Value::Object(extra) = extra else {
        return Err("record is not a JSON object".into());
    };
    for (k, v) in extra {
        map.insert(k, v);
    }
    Ok(Value::Object(map))
}

/// If `buf` starts with a complete JSON value (leading blanks allowed),
/// return that value and the unparsed suffix. `Ok(None)` means more input
/// is needed (unclosed array, object, or string). The whole document is
/// held in `buf`; this is not a streaming parser.
pub fn take_complete_json(buf: &str) -> Result<Option<(Value, &str)>, String> {
    match json_value_end(buf)? {
        None => Ok(None),
        Some(end) => {
            let slice = &buf[..end];
            let value: Value =
                serde_json::from_str(slice).map_err(|_| "invalid JSON".to_string())?;
            Ok(Some((value, &buf[end..])))
        }
    }
}

/// Byte end index of the first JSON value in `s`, or `None` if it is not
/// finished. Structural `[` `]` `{` `}` inside strings are ignored.
fn json_value_end(s: &str) -> Result<Option<usize>, String> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i == bytes.len() {
        return Ok(None);
    }
    let mut stack: Vec<u8> = Vec::new();
    let mut in_string = false;
    let mut escape = false;
    let mut saw_container = false;
    let start = i;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' | b'{' => {
                stack.push(b);
                saw_container = true;
            }
            b']' => {
                if stack.last() != Some(&b'[') {
                    return Err("invalid JSON".into());
                }
                stack.pop();
            }
            b'}' => {
                if stack.last() != Some(&b'{') {
                    return Err("invalid JSON".into());
                }
                stack.pop();
            }
            _ => {}
        }
        i += 1;
        if saw_container && stack.is_empty() && !in_string {
            return Ok(Some(i));
        }
    }
    if in_string || escape || !stack.is_empty() {
        return Ok(None);
    }
    // A top-level primitive (`true`, `1`, `"x"`) is complete when the token
    // ends. JSON ARRAY rejects those after parse; this still needs an end.
    if !saw_container {
        let mut j = start;
        if bytes[j] == b'"' {
            // finished string would have returned via in_string; unclosed
            // already returned None.
            return Ok(Some(i));
        }
        while j < bytes.len()
            && !bytes[j].is_ascii_whitespace()
            && bytes[j] != b','
            && bytes[j] != b']'
            && bytes[j] != b'}'
        {
            j += 1;
        }
        if j < bytes.len() || looks_complete_primitive(&bytes[start..j]) {
            return Ok(Some(j));
        }
        return Ok(None);
    }
    Ok(None)
}

fn looks_complete_primitive(tok: &[u8]) -> bool {
    matches!(tok, b"true" | b"false" | b"null")
        || tok.iter().all(|b| {
            b.is_ascii_digit() || *b == b'-' || *b == b'+' || *b == b'.' || *b == b'e' || *b == b'E'
        })
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
        let v = parse_json_object(br#"{"user":{"customer":"Ann"}}"#).unwrap();
        let path = parse_json_field_ref("$user.customer").unwrap();
        assert_eq!(
            lookup_json_path(&v, &path).unwrap().unwrap().as_str(),
            Some("Ann")
        );
        let missing = parse_json_field_ref("$user.city").unwrap();
        assert!(lookup_json_path(&v, &missing).unwrap().is_none());
    }

    #[test]
    fn array_index_lookup_dot_and_brackets() {
        let v = parse_json_object(br#"{"items":[{"sku":"NUT"},{"sku":"BOLT"}],"tags":["a","b"]}"#)
            .unwrap();
        let sku = parse_json_field_ref("$items.0.sku").unwrap();
        assert_eq!(
            lookup_json_path(&v, &sku).unwrap().unwrap().as_str(),
            Some("NUT")
        );
        let sku_b = parse_json_field_ref("$items[1].sku").unwrap();
        assert_eq!(
            lookup_json_path(&v, &sku_b).unwrap().unwrap().as_str(),
            Some("BOLT")
        );
        let last = parse_json_field_ref("$tags.-1").unwrap();
        assert_eq!(
            lookup_json_path(&v, &last).unwrap().unwrap().as_str(),
            Some("b")
        );
        let last_b = parse_json_field_ref("$tags[-1]").unwrap();
        assert_eq!(
            lookup_json_path(&v, &last_b).unwrap().unwrap().as_str(),
            Some("b")
        );
        let oob = parse_json_field_ref("$items.9.sku").unwrap();
        assert!(lookup_json_path(&v, &oob).unwrap().is_none());
        let key_on_string = parse_json_field_ref("$tags.0.sku").unwrap();
        let err = lookup_json_path(&v, &key_on_string).unwrap_err();
        assert!(err.contains("not a JSON object"), "{err}");
        let not_array = parse_json_object(br#"{"qty":3}"#).unwrap();
        let idx = parse_json_field_ref("$qty.0").unwrap();
        assert!(lookup_json_path(&not_array, &idx).unwrap().is_none());
    }

    #[test]
    fn display_preserves_types() {
        assert_eq!(display_json_value(&Value::from(120)), b"120");
        assert_eq!(display_json_value(&Value::String("Ann".into())), b"Ann");
        assert_eq!(display_json_value(&Value::Null), b"");
    }

    #[test]
    fn expr_multiply_and_concat() {
        let rec = parse_json_object(br#"{"qty":3,"price":40,"customer":"Ann"}"#).unwrap();
        let total = eval_json_expr(&rec, &["$qty".into(), "*".into(), "$price".into()]).unwrap();
        assert_eq!(total, Value::from(120));
        let greet =
            eval_json_expr(&rec, &["\"hi \"".into(), "+".into(), "$customer".into()]).unwrap();
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
            parse_json_field_ref("$qty"),
            Some(vec![FieldSeg::Key("qty".into())])
        );
        assert_eq!(
            parse_json_field_ref("$a.b").unwrap(),
            vec![FieldSeg::Key("a".into()), FieldSeg::Key("b".into())]
        );
        assert_eq!(
            parse_json_field_ref("$items.0.sku").unwrap(),
            vec![
                FieldSeg::Key("items".into()),
                FieldSeg::Index(0),
                FieldSeg::Key("sku".into())
            ]
        );
        assert_eq!(
            parse_json_field_ref("$items[0].sku").unwrap(),
            parse_json_field_ref("$items.0.sku").unwrap()
        );
        assert_eq!(
            parse_json_field_ref("$tags[-1]").unwrap(),
            vec![FieldSeg::Key("tags".into()), FieldSeg::Index(-1)]
        );
        assert!(parse_json_field_ref("$").is_none());
        assert!(parse_json_field_ref("qty").is_none());
        assert!(parse_json_field_ref("$1ab").is_none());
        assert!(parse_json_field_ref("$items[]").is_none());
        assert!(parse_json_field_ref("$items[1.5]").is_none());
    }

    #[test]
    fn expr_slash_string_vs_division() {
        let rec = parse_json_object(br#"{"city":"Oslo","qty":8,"price":2}"#).unwrap();
        let shout = eval_json_expr(&rec, &["$city".into(), "+".into(), "/!/".into()]).unwrap();
        assert_eq!(shout, Value::String("Oslo!".into()));
        let quoted = eval_json_expr(&rec, &["$city".into(), "+".into(), "\"!\"".into()]).unwrap();
        assert_eq!(quoted, Value::String("Oslo!".into()));
        let div = eval_json_expr(&rec, &["$qty".into(), "/".into(), "$price".into()]).unwrap();
        assert_eq!(div, Value::from(4));
    }

    #[test]
    fn take_complete_json_array_and_need_more() {
        let (v, rest) = take_complete_json("[1,2] leftover").unwrap().unwrap();
        assert_eq!(v, serde_json::json!([1, 2]));
        assert_eq!(rest, " leftover");
        assert!(take_complete_json("[1,2").unwrap().is_none());
        assert!(take_complete_json("").unwrap().is_none());
        let pretty = "[\n  {\"a\": 1}\n]";
        let (v, rest) = take_complete_json(pretty).unwrap().unwrap();
        assert_eq!(v, serde_json::json!([{"a": 1}]));
        assert!(rest.is_empty());
        assert!(take_complete_json("[1,}]").is_err());
    }

    #[test]
    fn wrap_scalar_element() {
        let v = object_record_from_element(Value::from(3));
        assert_eq!(v["value"], 3);
        let obj = object_record_from_element(serde_json::json!({"title":"Dune"}));
        assert_eq!(obj["title"], "Dune");
        assert!(obj.get("value").is_none());
    }
}
