//! CSV: experimental portable stage (not IBM CMS Pipelines).
//!
//! Reader: each CSV row becomes one JSON object. The first row is the header.
//! Writer (`CSV WRITE`): each JSON object becomes one CSV text record.
//!
//! RFC 4180 quoting (`"` and doubled `""`). Quoted commas work in one record.
//! Quoted newlines work when a quoted field is split across input records
//! (the reader joins them with a newline). A file read by `<` is one record
//! per line, so that join is how an embedded newline stays one field.

use pipe_core::{Record, StageCtx};
use serde_json::{Map, Value};

use crate::json_row::{csv_detect_number, parse_json_object, write_json_object};

pub fn csv(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_csv(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("CSV: {e}"));
                return;
            }
        };
        match spec {
            CsvOp::Read { strings } => run_read(ctx, strings),
            CsvOp::Write => run_write(ctx),
        }
    })
}

enum CsvOp {
    Read { strings: bool },
    Write,
}

fn parse_csv(args: &[String]) -> Result<CsvOp, String> {
    let mut i = 0;
    let mut write = false;
    let mut strings = false;
    while i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u == "WRITE" {
            if write {
                return Err("WRITE was already specified".into());
            }
            write = true;
            i += 1;
        } else if u == "STRINGS" {
            if strings {
                return Err("STRINGS was already specified".into());
            }
            strings = true;
            i += 1;
        } else {
            return Err(format!("unrecognized operand {}", args[i]));
        }
    }
    if write && strings {
        return Err("STRINGS applies to the reader, not WRITE".into());
    }
    if write {
        Ok(CsvOp::Write)
    } else {
        Ok(CsvOp::Read { strings })
    }
}

fn run_read(ctx: &mut StageCtx, strings: bool) {
    let mut pending = String::new();
    let mut header: Option<Vec<String>> = None;
    while let Ok(Some(rec)) = ctx.read() {
        if !pending.is_empty() {
            pending.push('\n');
        }
        pending.push_str(&rec.to_string_lossy());
        loop {
            match take_csv_row(&pending) {
                Ok(Some((row, rest))) => {
                    pending = rest;
                    if let Err(e) = emit_row(ctx, &mut header, row, strings) {
                        ctx.fail(format!("CSV: {e}"));
                        return;
                    }
                    if pending.is_empty() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    ctx.fail(format!("CSV: {e}"));
                    return;
                }
            }
        }
    }
    if !pending.is_empty() {
        match take_csv_row(&pending) {
            Ok(Some((row, rest))) => {
                if !rest.trim().is_empty() {
                    ctx.fail("CSV: leftover text after the last row");
                    return;
                }
                if let Err(e) = emit_row(ctx, &mut header, row, strings) {
                    ctx.fail(format!("CSV: {e}"));
                }
            }
            Ok(None) => ctx.fail("CSV: unclosed quoted field"),
            Err(e) => ctx.fail(format!("CSV: {e}")),
        }
    }
}

fn emit_row(
    ctx: &mut StageCtx,
    header: &mut Option<Vec<String>>,
    mut row: Vec<String>,
    strings: bool,
) -> Result<(), String> {
    if header.is_none() {
        if !row.is_empty() && row[0].starts_with('\u{feff}') {
            row[0] = row[0].trim_start_matches('\u{feff}').to_string();
        }
        let mut seen = std::collections::HashSet::new();
        for name in &row {
            if !seen.insert(name.clone()) {
                return Err(format!("duplicate header name {name}"));
            }
        }
        *header = Some(row);
        return Ok(());
    }
    let names = header.as_ref().unwrap();
    if row.len() > names.len() {
        return Err(format!(
            "row has {} fields; header has {}",
            row.len(),
            names.len()
        ));
    }
    let mut map = Map::new();
    for (i, name) in names.iter().enumerate() {
        let cell = row.get(i).cloned().unwrap_or_default();
        let value = if strings {
            Value::String(cell)
        } else if let Some(n) = csv_detect_number(&cell) {
            Value::Number(n)
        } else {
            Value::String(cell)
        };
        map.insert(name.clone(), value);
    }
    let rec = write_json_object(&Value::Object(map))?;
    if ctx.write(Record::from(rec)).is_err() {
        return Ok(());
    }
    Ok(())
}

fn run_write(ctx: &mut StageCtx) {
    let mut header: Option<Vec<String>> = None;
    while let Ok(Some(rec)) = ctx.read() {
        let obj = match parse_json_object(rec.as_bytes()) {
            Ok(v) => v,
            Err(e) => {
                ctx.fail(format!("CSV: {e}"));
                return;
            }
        };
        let map = obj.as_object().unwrap();
        if header.is_none() {
            let names: Vec<String> = map.keys().cloned().collect();
            let line = write_csv_row(&names.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            if ctx.write(Record::from(line)).is_err() {
                return;
            }
            header = Some(names);
        }
        let names = header.as_ref().unwrap();
        let cells: Vec<String> = names
            .iter()
            .map(|k| match map.get(k) {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                Some(Value::Bool(b)) => b.to_string(),
                Some(Value::Number(n)) => n.to_string(),
                Some(other) => serde_json::to_string(other).unwrap_or_default(),
            })
            .collect();
        let line = write_csv_row(&cells.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        if ctx.write(Record::from(line)).is_err() {
            return;
        }
    }
}

/// Take one complete CSV row. `Ok(None)` when a quoted field is unfinished.
fn take_csv_row(input: &str) -> Result<Option<(Vec<String>, String)>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    if chars.is_empty() {
        return Ok(None);
    }
    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    field.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
                i += 1;
                continue;
            }
            field.push(c);
            i += 1;
            continue;
        }
        match c {
            '"' => {
                if !field.is_empty() {
                    return Err("quote is only allowed at the start of a field".into());
                }
                in_quotes = true;
                i += 1;
            }
            ',' => {
                fields.push(std::mem::take(&mut field));
                i += 1;
            }
            '\n' => {
                fields.push(std::mem::take(&mut field));
                i += 1;
                let rest: String = chars[i..].iter().collect();
                return Ok(Some((fields, rest)));
            }
            '\r' => {
                fields.push(std::mem::take(&mut field));
                i += 1;
                if chars.get(i) == Some(&'\n') {
                    i += 1;
                }
                let rest: String = chars[i..].iter().collect();
                return Ok(Some((fields, rest)));
            }
            _ => {
                field.push(c);
                i += 1;
            }
        }
    }
    if in_quotes {
        return Ok(None);
    }
    fields.push(field);
    Ok(Some((fields, String::new())))
}

fn write_csv_row(fields: &[&str]) -> String {
    let mut out = String::new();
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&escape_csv_field(f));
    }
    out
}

fn escape_csv_field(s: &str) -> String {
    let needs_quote = s.contains(',')
        || s.contains('"')
        || s.contains('\n')
        || s.contains('\r')
        || s.starts_with(' ')
        || s.ends_with(' ');
    if !needs_quote {
        return s.to_string();
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        if c == '"' {
            out.push_str("\"\"");
        } else {
            out.push(c);
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_quoted_comma_and_doubled_quote() {
        let (row, rest) = take_csv_row(r#"Ann,"x,y","say ""hi""""#).unwrap().unwrap();
        assert!(rest.is_empty());
        assert_eq!(row, vec!["Ann", "x,y", "say \"hi\""]);
    }

    #[test]
    fn unfinished_quote_needs_more() {
        assert!(take_csv_row(r#"a,"b"#).unwrap().is_none());
    }

    #[test]
    fn escape_quotes_commas_and_spaces() {
        assert_eq!(escape_csv_field("Ann"), "Ann");
        assert_eq!(escape_csv_field("x,y"), "\"x,y\"");
        assert_eq!(escape_csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape_csv_field(" a"), "\" a\"");
    }
}
