//! JSON: experimental portable stage (not IBM CMS Pipelines).
//!
//! Operates on one JSON object per record, except `JSON ARRAY` which reads a
//! top-level JSON array (one line or pretty-printed over many records) and
//! writes one object record per element. Syntax:
//! `JSON ADD name = expr` / `JSON SET name = expr` / `JSON DELETE names...`
//! / `JSON KEEP names...` / `JSON GET name` / `JSON SPLIT name [KEEP]`
//! / `JSON COLLECT name` / `JSON ARRAY`.

use pipe_core::{Record, StageCtx};
use serde_json::{Map, Value};

use crate::json_row::{
    delete_json_path, display_json_value, eval_json_expr, format_path, keep_json_fields,
    lookup_json_path, merge_json_objects, object_record_from_element, parse_field_path,
    parse_json_field_ref, parse_json_object, set_json_path, take_complete_json, write_json_object,
    FieldSeg,
};

pub fn json(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_json(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("JSON: {e}"));
                return;
            }
        };
        run_json(ctx, spec);
    })
}

enum JsonOp {
    Add {
        path: Vec<FieldSeg>,
        expr: Vec<String>,
    },
    Set {
        path: Vec<FieldSeg>,
        expr: Vec<String>,
    },
    Delete {
        paths: Vec<Vec<FieldSeg>>,
    },
    Keep {
        names: Vec<String>,
    },
    Get {
        path: Vec<FieldSeg>,
    },
    Split {
        path: Vec<FieldSeg>,
        keep: bool,
    },
    Collect {
        name: String,
    },
    Array,
}

fn parse_json(args: &[String]) -> Result<JsonOp, String> {
    if args.is_empty() {
        return Err("ADD, SET, DELETE, KEEP, GET, SPLIT, COLLECT, or ARRAY is required".into());
    }
    let verb = args[0].to_ascii_uppercase();
    let rest = &args[1..];
    match verb.as_str() {
        "ADD" => {
            let (path, expr) = take_assign(rest)?;
            Ok(JsonOp::Add { path, expr })
        }
        "SET" => {
            let (path, expr) = take_assign(rest)?;
            Ok(JsonOp::Set { path, expr })
        }
        "DELETE" => {
            if rest.is_empty() {
                return Err("DELETE requires a field name".into());
            }
            let mut paths = Vec::new();
            for tok in rest {
                paths.push(field_name_token(tok)?);
            }
            Ok(JsonOp::Delete { paths })
        }
        "KEEP" => {
            if rest.is_empty() {
                return Err("KEEP requires at least one field name".into());
            }
            let mut names = Vec::new();
            for tok in rest {
                let path = field_name_token(tok)?;
                match path.as_slice() {
                    [FieldSeg::Key(name)] => names.push(name.clone()),
                    _ => return Err("KEEP takes top-level field names (not $a.b)".into()),
                }
            }
            Ok(JsonOp::Keep { names })
        }
        "GET" => {
            if rest.len() != 1 {
                return Err("GET requires one field name".into());
            }
            Ok(JsonOp::Get {
                path: field_name_token(&rest[0])?,
            })
        }
        "SPLIT" => parse_split(rest),
        "COLLECT" => parse_collect(rest),
        "ARRAY" => {
            if !rest.is_empty() {
                return Err("ARRAY takes no operands".into());
            }
            Ok(JsonOp::Array)
        }
        _ => Err(format!("unrecognized operand {}", args[0])),
    }
}

fn parse_split(rest: &[String]) -> Result<JsonOp, String> {
    if rest.is_empty() {
        return Err("SPLIT requires a field name".into());
    }
    let path = field_name_token(&rest[0])?;
    let mut keep = false;
    let mut i = 1;
    while i < rest.len() {
        if rest[i].eq_ignore_ascii_case("KEEP") {
            if keep {
                return Err("KEEP was already specified".into());
            }
            keep = true;
            i += 1;
        } else {
            return Err(format!("unrecognized operand {}", rest[i]));
        }
    }
    Ok(JsonOp::Split { path, keep })
}

fn parse_collect(rest: &[String]) -> Result<JsonOp, String> {
    if rest.len() != 1 {
        return Err("COLLECT requires one field name".into());
    }
    let path = field_name_token(&rest[0])?;
    match path.as_slice() {
        [FieldSeg::Key(name)] => Ok(JsonOp::Collect { name: name.clone() }),
        _ => Err("COLLECT takes a top-level field name".into()),
    }
}

fn field_name_token(tok: &str) -> Result<Vec<FieldSeg>, String> {
    if let Some(path) = parse_json_field_ref(tok) {
        return Ok(path);
    }
    parse_field_path(tok).ok_or_else(|| format!("not a field name: {tok}"))
}

fn take_assign(args: &[String]) -> Result<(Vec<FieldSeg>, Vec<String>), String> {
    if args.is_empty() {
        return Err("a field name and = expression are required".into());
    }
    let mut name_toks = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "=" {
            i += 1;
            break;
        }
        if let Some((left, right)) = args[i].split_once('=') {
            if !left.is_empty() {
                name_toks.push(left.to_string());
            }
            let mut expr = Vec::new();
            if !right.is_empty() {
                expr.push(right.to_string());
            }
            expr.extend(args[i + 1..].iter().cloned());
            if name_toks.is_empty() {
                return Err("a field name is required before =".into());
            }
            if name_toks.len() != 1 {
                return Err("ADD/SET takes one field name".into());
            }
            return Ok((field_name_token(&name_toks[0])?, expr));
        }
        name_toks.push(args[i].clone());
        i += 1;
    }
    if name_toks.len() != 1 {
        return Err("ADD/SET takes one field name before =".into());
    }
    if i > args.len() {
        return Err("an expression is required after =".into());
    }
    let expr = args[i..].to_vec();
    if expr.is_empty() && !args.iter().any(|a| a.contains('=')) {
        return Err("an expression is required after =".into());
    }
    Ok((field_name_token(&name_toks[0])?, expr))
}

fn run_json(ctx: &mut StageCtx, spec: JsonOp) {
    match spec {
        JsonOp::Array => run_array(ctx),
        JsonOp::Collect { name } => run_collect(ctx, name),
        JsonOp::Split { path, keep } => run_split(ctx, &path, keep),
        other => run_per_record(ctx, &other),
    }
}

fn run_per_record(ctx: &mut StageCtx, spec: &JsonOp) {
    while let Ok(Some(rec)) = ctx.read() {
        let out = match apply_op(spec, rec.as_bytes()) {
            Ok(bytes) => Record::from(bytes),
            Err(e) => {
                ctx.fail(format!("JSON: {e}"));
                return;
            }
        };
        if ctx.write(out).is_err() {
            break;
        }
    }
}

fn apply_op(spec: &JsonOp, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match spec {
        JsonOp::Get { path } => {
            let obj = parse_json_object(bytes)?;
            match lookup_json_path(&obj, path)? {
                Some(v) => Ok(display_json_value(v)),
                None => Err(format!("field ${} is missing", format_path(path))),
            }
        }
        JsonOp::Add { path, expr } => rewrite(bytes, |obj| {
            let val = eval_json_expr(obj, expr)?;
            set_json_path(obj, path, val, false)
        }),
        JsonOp::Set { path, expr } => rewrite(bytes, |obj| {
            let val = eval_json_expr(obj, expr)?;
            set_json_path(obj, path, val, true)
        }),
        JsonOp::Delete { paths } => rewrite(bytes, |obj| {
            for path in paths {
                delete_json_path(obj, path)?;
            }
            Ok(())
        }),
        JsonOp::Keep { names } => rewrite(bytes, |obj| keep_json_fields(obj, names)),
        JsonOp::Split { .. } | JsonOp::Collect { .. } | JsonOp::Array => {
            Err("internal error: this JSON verb is not per-record".into())
        }
    }
}

fn rewrite(
    bytes: &[u8],
    f: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let mut obj = parse_json_object(bytes)?;
    f(&mut obj)?;
    Ok(write_json_object(&obj)?.into_bytes())
}

fn run_split(ctx: &mut StageCtx, path: &[FieldSeg], keep: bool) {
    while let Ok(Some(rec)) = ctx.read() {
        match split_record(rec.as_bytes(), path, keep) {
            Ok(outs) => {
                for bytes in outs {
                    if ctx.write(Record::from(bytes)).is_err() {
                        return;
                    }
                }
            }
            Err(e) => {
                ctx.fail(format!("JSON: {e}"));
                return;
            }
        }
    }
}

fn split_record(bytes: &[u8], path: &[FieldSeg], keep: bool) -> Result<Vec<Vec<u8>>, String> {
    let parent = parse_json_object(bytes)?;
    let arr = match lookup_json_path(&parent, path)? {
        None => return Err(format!("field ${} is missing", format_path(path))),
        Some(Value::Array(a)) => a.clone(),
        Some(_) => return Err(format!("${} is not a JSON array", format_path(path))),
    };
    let mut base = parent.clone();
    if keep {
        delete_json_path(&mut base, path)?;
    }
    let mut out = Vec::with_capacity(arr.len());
    for elem in arr {
        let wrapped = object_record_from_element(elem);
        let rec = if keep {
            merge_json_objects(base.clone(), wrapped)?
        } else {
            wrapped
        };
        out.push(write_json_object(&rec)?.into_bytes());
    }
    Ok(out)
}

fn run_collect(ctx: &mut StageCtx, name: String) {
    let mut items = Vec::new();
    while let Ok(Some(rec)) = ctx.read() {
        match parse_json_object(rec.as_bytes()) {
            Ok(obj) => items.push(obj),
            Err(e) => {
                ctx.fail(format!("JSON: {e}"));
                return;
            }
        }
    }
    let mut map = Map::new();
    map.insert(name, Value::Array(items));
    match write_json_object(&Value::Object(map)) {
        Ok(text) => {
            let _ = ctx.write(Record::from(text));
        }
        Err(e) => ctx.fail(format!("JSON: {e}")),
    }
}

fn run_array(ctx: &mut StageCtx) {
    let mut buf = String::new();
    while let Ok(Some(rec)) = ctx.read() {
        let text = match std::str::from_utf8(rec.as_bytes()) {
            Ok(t) => t,
            Err(_) => {
                ctx.fail("JSON: input is not valid UTF-8");
                return;
            }
        };
        if !buf.is_empty() {
            buf.push('\n');
        }
        buf.push_str(text);
        if !drain_complete_arrays(ctx, &mut buf) {
            return;
        }
    }
    if buf.trim().is_empty() {
        return;
    }
    match take_complete_json(&buf) {
        Ok(Some((value, rest))) => {
            if !rest.trim().is_empty() {
                ctx.fail("JSON: leftover text after the JSON array");
                return;
            }
            match emit_array_elements(ctx, value) {
                Ok(_) => {}
                Err(e) => ctx.fail(format!("JSON: {e}")),
            }
        }
        Ok(None) => ctx.fail("JSON: unclosed JSON array"),
        Err(e) => ctx.fail(format!("JSON: {e}")),
    }
}

/// Parse and emit every complete JSON value at the front of `buf`.
/// Returns false when the stage has failed or the output closed.
fn drain_complete_arrays(ctx: &mut StageCtx, buf: &mut String) -> bool {
    loop {
        match take_complete_json(buf) {
            Ok(None) => return true,
            Ok(Some((value, rest))) => {
                let rest = rest.to_string();
                match emit_array_elements(ctx, value) {
                    Ok(true) => {}
                    Ok(false) => return false,
                    Err(e) => {
                        ctx.fail(format!("JSON: {e}"));
                        return false;
                    }
                }
                *buf = rest;
                if buf.trim().is_empty() {
                    buf.clear();
                    return true;
                }
            }
            Err(e) => {
                ctx.fail(format!("JSON: {e}"));
                return false;
            }
        }
    }
}

fn emit_array_elements(ctx: &mut StageCtx, value: Value) -> Result<bool, String> {
    let Value::Array(items) = value else {
        return Err("input is not a JSON array".into());
    };
    for elem in items {
        let rec = object_record_from_element(elem);
        let text = write_json_object(&rec)?;
        if ctx.write(Record::from(text)).is_err() {
            return Ok(false);
        }
    }
    Ok(true)
}
