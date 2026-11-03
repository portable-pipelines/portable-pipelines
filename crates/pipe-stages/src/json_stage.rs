//! JSON: experimental portable stage (not IBM CMS Pipelines).
//!
//! Operates on one JSON object per record. Syntax:
//! `JSON ADD name = expr` / `JSON SET name = expr` / `JSON DELETE names...`
//! / `JSON KEEP names...` / `JSON GET name`.

use pipe_core::{Record, StageCtx};

use crate::json_row::{
    delete_json_path, display_json_value, eval_json_expr, keep_json_fields, lookup_json_path,
    parse_field_path, parse_json_field_ref, parse_json_object, set_json_path, write_json_object,
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
        path: Vec<String>,
        expr: Vec<String>,
    },
    Set {
        path: Vec<String>,
        expr: Vec<String>,
    },
    Delete {
        paths: Vec<Vec<String>>,
    },
    Keep {
        names: Vec<String>,
    },
    Get {
        path: Vec<String>,
    },
}

fn parse_json(args: &[String]) -> Result<JsonOp, String> {
    if args.is_empty() {
        return Err("ADD, SET, DELETE, KEEP, or GET is required".into());
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
                if path.len() != 1 {
                    return Err("KEEP takes top-level field names (not $a.b)".into());
                }
                names.push(path[0].clone());
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
        _ => Err(format!("unrecognized operand {}", args[0])),
    }
}

fn field_name_token(tok: &str) -> Result<Vec<String>, String> {
    if let Some(path) = parse_json_field_ref(tok) {
        return Ok(path);
    }
    parse_field_path(tok).ok_or_else(|| format!("not a field name: {tok}"))
}

fn take_assign(args: &[String]) -> Result<(Vec<String>, Vec<String>), String> {
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
    while let Ok(Some(rec)) = ctx.read() {
        let out = match apply_op(&spec, rec.as_bytes()) {
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
                None => Err(format!("field ${} is missing", path.join("."))),
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
    }
}

fn rewrite(
    bytes: &[u8],
    f: impl FnOnce(&mut serde_json::Value) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let mut obj = parse_json_object(bytes)?;
    f(&mut obj)?;
    Ok(write_json_object(&obj)?.into_bytes())
}
