//! Minimal REXX subset interpreter for pipeline stages.
//!
//! Supports enough syntax to write real filters:
//! - assignments: `x = 'value'`
//! - `DO FOREVER` / `DO WHILE` / `END`
//! - `IF … THEN` / `ELSE`
//! - `LEAVE`, `EXIT [n]`
//! - `PARSE VAR name rest`
//! - pipeline commands (IBM names): `READTO var`, `OUTPUT expr`, `CALLPIPE 'spec'`,
//!   `SHORT`, `SELECT INPUT/OUTPUT n`
//! - string ops: concatenation with `||`, `SUBSTR`, `LENGTH`, `POS`
//! - `SAY` (debug to stderr)

use std::cell::RefCell;
use std::collections::HashMap;

use pipe_core::{PipeError, PipeResult};

use crate::subcommands::RexxPipeHost;

pub fn run_rexx_source(
    host: &RefCell<RexxPipeHost<'_>>,
    source: &str,
    args: &[String],
) -> PipeResult<()> {
    let mut vars: HashMap<String, String> = HashMap::new();
    vars.insert("RC".into(), "0".into());
    for (i, a) in args.iter().enumerate() {
        vars.insert(format!("ARG.{}", i + 1), a.clone());
    }
    vars.insert("ARG.0".into(), args.len().to_string());

    let lines = preprocess(source);
    let mut pc = 0usize;
    let mut call_stack: Vec<Frame> = Vec::new();

    while pc < lines.len() {
        let line = lines[pc].trim();
        pc += 1;
        if line.is_empty() || line.starts_with("/*") {
            continue;
        }
        let upper = line.to_ascii_uppercase();

        if upper.starts_with("DO FOREVER") {
            call_stack.push(Frame::DoForever { start: pc });
            continue;
        }
        if upper.starts_with("DO WHILE") {
            let cond = line[8..].trim(); // after "DO WHILE" roughly
            let cond = strip_prefix_ci(line, "DO WHILE").unwrap_or(cond);
            if !eval_bool(host, &vars, cond)? {
                pc = find_matching_end(&lines, pc - 1)? + 1;
            } else {
                call_stack.push(Frame::DoWhile {
                    start: pc,
                    cond: cond.to_string(),
                });
            }
            continue;
        }
        if upper == "END" {
            match call_stack.last() {
                Some(Frame::DoForever { start }) => {
                    pc = *start;
                }
                Some(Frame::DoWhile { start, cond }) => {
                    if eval_bool(host, &vars, cond)? {
                        pc = *start;
                    } else {
                        call_stack.pop();
                    }
                }
                _ => {}
            }
            continue;
        }
        if upper == "LEAVE" {
            // Jump to matching END and pop.
            let end = find_matching_end(&lines, pc - 1)?;
            call_stack.pop();
            pc = end + 1;
            continue;
        }
        if upper.starts_with("EXIT") {
            let rest = strip_prefix_ci(line, "EXIT").unwrap_or("").trim();
            let code = if rest.is_empty() {
                0
            } else {
                eval_expr(host, &vars, rest)?.parse().unwrap_or(0)
            };
            let mut h = host.borrow_mut();
            h.rc = code;
            h.exit_rc = Some(code);
            return Ok(());
        }
        if upper.starts_with("IF ") {
            let (cond, then_part) = split_if(line)?;
            let branch = if eval_bool(host, &vars, &cond)? {
                then_part
            } else if pc < lines.len() {
                let n = lines[pc].trim();
                if n.to_ascii_uppercase().starts_with("ELSE") {
                    pc += 1;
                    let else_part = strip_prefix_ci(n, "ELSE").unwrap_or("").trim();
                    strip_prefix_ci(else_part, "THEN")
                        .unwrap_or(else_part)
                        .trim()
                        .to_string()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            if !branch.is_empty() {
                match exec_control(host, &mut vars, &mut call_stack, &lines, &mut pc, &branch)? {
                    Flow::Continue => {}
                    Flow::Exit => return Ok(()),
                }
            }
            continue;
        }
        if upper.starts_with("SAY ") {
            let expr = &line[4..];
            let v = eval_expr(host, &vars, expr)?;
            if host.borrow().implied_output() {
                host.borrow_mut().output(&v)?;
                vars.insert("RC".into(), host.borrow().rc.to_string());
            } else {
                eprintln!("{v}");
            }
            continue;
        }

        match exec_control(host, &mut vars, &mut call_stack, &lines, &mut pc, line)? {
            Flow::Continue => {
                vars.insert("RC".into(), host.borrow().rc.to_string());
            }
            Flow::Exit => return Ok(()),
        }
    }
    Ok(())
}

enum Flow {
    Continue,
    Exit,
}

enum Frame {
    DoForever { start: usize },
    DoWhile { start: usize, cond: String },
}

fn exec_control(
    host: &RefCell<RexxPipeHost<'_>>,
    vars: &mut HashMap<String, String>,
    call_stack: &mut Vec<Frame>,
    lines: &[String],
    pc: &mut usize,
    line: &str,
) -> PipeResult<Flow> {
    let upper = line.trim().to_ascii_uppercase();
    if upper == "LEAVE" {
        // Leave the nearest DO group.
        let do_start = call_stack
            .last()
            .map(|f| match f {
                Frame::DoForever { start } | Frame::DoWhile { start, .. } => *start - 1,
            })
            .unwrap_or(0);
        let end = find_matching_end(lines, do_start)?;
        call_stack.pop();
        *pc = end + 1;
        return Ok(Flow::Continue);
    }
    if upper.starts_with("EXIT") {
        let rest = strip_prefix_ci(line, "EXIT").unwrap_or("").trim();
        let code = if rest.is_empty() {
            0
        } else {
            eval_expr(host, vars, rest)?.parse().unwrap_or(0)
        };
        let mut h = host.borrow_mut();
        h.rc = code;
        h.exit_rc = Some(code);
        return Ok(Flow::Exit);
    }
    exec_simple(host, vars, line)?;
    Ok(Flow::Continue)
}

fn preprocess(source: &str) -> Vec<String> {
    // CMS `/=` and `^=` become `\=` first so the subset matches Regina.
    let source = crate::cms_ops::normalize_cms_rexx_operators(source);
    // Strip block comments /* … */ and split into lines; join continuations `,`
    let mut out = String::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(c2) = chars.next() {
                if c2 == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out.lines().map(|l| l.trim_end().to_string()).collect()
}

fn find_matching_end(lines: &[String], do_idx: usize) -> PipeResult<usize> {
    let mut depth = 0;
    for i in do_idx..lines.len() {
        let u = lines[i].trim().to_ascii_uppercase();
        if u.starts_with("DO ")
            || u == "DO"
            || u.starts_with("DO FOREVER")
            || u.starts_with("DO WHILE")
        {
            depth += 1;
        } else if u == "END" {
            depth -= 1;
            if depth == 0 {
                return Ok(i);
            }
        }
    }
    Err(PipeError::stage("REXX: unmatched DO/END"))
}

fn split_if(line: &str) -> PipeResult<(String, String)> {
    let rest = strip_prefix_ci(line, "IF").unwrap().trim();
    let upper = rest.to_ascii_uppercase();
    if let Some(idx) = upper.find(" THEN") {
        let cond = rest[..idx].trim().to_string();
        let then_part = rest[idx + 5..].trim().to_string();
        Ok((cond, then_part))
    } else {
        Err(PipeError::stage(format!("REXX: IF without THEN: {line}")))
    }
}

/// Normalize IBM-style quoted pipeline clauses for the subset:
/// `'READTO LINE'` → `READTO LINE`, `'OUTPUT' expr` → `OUTPUT expr`.
fn normalize_pipeline_clause(line: &str) -> String {
    let t = line.trim();
    // Single quoted string as the whole clause: 'READTO LINE'
    // (Reject if another quote appears inside — that is `'VERB' operand`.)
    if t.len() >= 2 {
        let q = t.chars().next().unwrap();
        if (q == '\'' || q == '"') && t.ends_with(q) {
            let inner = &t[q.len_utf8()..t.len() - q.len_utf8()];
            if !inner.contains(q) {
                return inner.trim().to_string();
            }
        }
    }
    // Verb quoted, operand expression follows: 'OUTPUT' TRANSLATE(LINE)
    for q in ['\'', '"'] {
        for verb in [
            "OUTPUT",
            "CALLPIPE",
            "ADDPIPE",
            "SHORT",
            "SELECT",
            "COMMIT",
            "PEEKTO",
            "READTO",
            "ADDSTREAM",
            "BEGOUTPUT",
            "EOFREPORT",
            "GETRANGE",
            "ISSUEMSG",
            "MAXSTREAM",
            "MESSAGE",
            "NOCOMMIT",
            "RESOLVE",
            "REXX",
            "SCANRANGE",
            "SCANSTRING",
            "SETRC",
            "SEVER",
            "STAGENUM",
            "STREAMNUM",
            "STREAMSTATE",
            "SUSPEND",
        ] {
            let prefix = format!("{q}{verb}{q}");
            if t.len() >= prefix.len() && t[..prefix.len()].eq_ignore_ascii_case(&prefix) {
                let rest = t[prefix.len()..].trim();
                return if rest.is_empty() {
                    verb.to_string()
                } else {
                    format!("{verb} {rest}")
                };
            }
        }
    }
    t.to_string()
}

fn exec_simple(
    host: &RefCell<RexxPipeHost<'_>>,
    vars: &mut HashMap<String, String>,
    line: &str,
) -> PipeResult<()> {
    let line = normalize_pipeline_clause(line);
    let line = line.as_str();
    let upper = line.to_ascii_uppercase();

    if upper == "READTO" || upper.starts_with("READTO ") {
        let result = host.borrow_mut().dispatch(line).map_err(PipeError::stage)?;
        for (k, v) in result.assigns {
            vars.insert(k, v);
        }
        vars.insert("RC".into(), result.rc.to_string());
        return Ok(());
    }
    if upper == "OUTPUT" {
        host.borrow_mut().output("")?;
        return Ok(());
    }
    if upper.starts_with("OUTPUT ") {
        let expr = line[7..].trim();
        let v = eval_expr(host, vars, expr)?;
        host.borrow_mut().output(&v)?;
        return Ok(());
    }
    if upper.starts_with("CALLPIPE ") {
        let expr = line[9..].trim();
        let spec = eval_expr(host, vars, expr)?;
        host.borrow_mut().callpipe(&spec)?;
        return Ok(());
    }
    if upper.starts_with("ADDPIPE ") {
        let expr = line[8..].trim();
        let spec = eval_expr(host, vars, expr)?;
        host.borrow_mut().addpipe(&spec)?;
        return Ok(());
    }
    if is_pipeline_command(&upper) {
        let result = host.borrow_mut().dispatch(line).map_err(PipeError::stage)?;
        for (k, v) in result.assigns {
            vars.insert(k, v);
        }
        vars.insert("RC".into(), result.rc.to_string());
        return Ok(());
    }
    if upper.starts_with("PARSE ARG ") {
        // PARSE ARG v1 v2 … — IBM stage args into variables (from ARG.1…).
        let rest = strip_prefix_ci(line, "PARSE ARG").unwrap_or("").trim();
        for (i, tok) in rest.split_whitespace().enumerate() {
            let key = tok.to_ascii_uppercase();
            let val = vars
                .get(&format!("ARG.{}", i + 1))
                .cloned()
                .unwrap_or_default();
            vars.insert(key, val);
        }
        return Ok(());
    }
    if upper.starts_with("PARSE VAR ") {
        // PARSE VAR name restvar  — put remainder after first word into restvar
        let rest = &line[10..];
        let mut parts = rest.split_whitespace();
        let src = parts
            .next()
            .ok_or_else(|| PipeError::stage("PARSE VAR needs a variable"))?
            .to_ascii_uppercase();
        let target = parts.next().unwrap_or("REST").to_ascii_uppercase();
        let val = vars.get(&src).cloned().unwrap_or_default();
        let mut sp = val.splitn(2, char::is_whitespace);
        let first = sp.next().unwrap_or("").to_string();
        let rem = sp.next().unwrap_or("").trim_start().to_string();
        vars.insert(src, first);
        vars.insert(target, rem);
        return Ok(());
    }

    // Assignment: name = expr
    if let Some(eq) = find_assign(line) {
        let name = line[..eq].trim().to_ascii_uppercase();
        let expr = line[eq + 1..].trim();
        let v = eval_expr(host, vars, expr)?;
        vars.insert(name, v);
        return Ok(());
    }

    let upper = line.trim().to_ascii_uppercase();
    if upper.starts_with("TRACE") {
        return Err(PipeError::stage(
            "REXX: TRACE is not implemented in the subset interpreter; \
             install Regina for interactive TRACE ?R (IBM REXX)",
        ));
    }
    Err(PipeError::stage(format!(
        "REXX: unrecognized statement: {line}"
    )))
}

fn is_pipeline_command(upper: &str) -> bool {
    let verb = upper.split_whitespace().next().unwrap_or(upper);
    matches!(
        verb,
        "SHORT"
            | "SELECT"
            | "COMMIT"
            | "PEEKTO"
            | "ADDSTREAM"
            | "BEGOUTPUT"
            | "EOFREPORT"
            | "GETRANGE"
            | "ISSUEMSG"
            | "MAXSTREAM"
            | "MESSAGE"
            | "NOCOMMIT"
            | "RESOLVE"
            | "REXX"
            | "SCANRANGE"
            | "SCANSTRING"
            | "SETRC"
            | "SEVER"
            | "STAGENUM"
            | "STREAMNUM"
            | "STREAMSTATE"
            | "SUSPEND"
    ) || (verb.len() >= 8 && "ADDSTREAM".starts_with(verb))
        || (verb.len() >= 8 && "STREAMNUM".starts_with(verb))
        || (verb.len() >= 8 && "STREAMSTATE".starts_with(verb))
        || (verb.len() >= 8 && "MAXSTREAM".starts_with(verb))
}

fn find_assign(line: &str) -> Option<usize> {
    // Avoid `==` and `= ` inside strings — simple scan.
    let mut in_str = None;
    for (i, c) in line.char_indices() {
        if c == '\'' || c == '"' {
            if in_str == Some(c) {
                in_str = None;
            } else if in_str.is_none() {
                in_str = Some(c);
            }
            continue;
        }
        if in_str.is_none() && c == '=' {
            // not ==
            let next = line[i + c.len_utf8()..].chars().next();
            if next != Some('=') {
                return Some(i);
            }
        }
    }
    None
}

fn eval_bool(
    host: &RefCell<RexxPipeHost<'_>>,
    vars: &HashMap<String, String>,
    expr: &str,
) -> PipeResult<bool> {
    let e = expr.trim();
    let upper = e.to_ascii_uppercase();
    if upper == "1" || upper == "YES" || upper == "TRUE" {
        return Ok(true);
    }
    if upper == "0" || upper == "NO" || upper == "FALSE" || upper.is_empty() {
        return Ok(false);
    }
    // Comparisons
    for op in [
        "==", "¬=", "\\=", "/=", "^=", "<>", "<=", ">=", "=", "<", ">",
    ] {
        if let Some(idx) = find_op(e, op) {
            let l = eval_expr(host, vars, e[..idx].trim())?;
            let r = eval_expr(host, vars, e[idx + op.len()..].trim())?;
            return Ok(match op {
                "==" | "=" => l == r,
                "¬=" | "\\=" | "/=" | "^=" | "<>" => l != r,
                "<" => l < r,
                ">" => l > r,
                "<=" => l <= r,
                ">=" => l >= r,
                _ => false,
            });
        }
    }
    let v = eval_expr(host, vars, e)?;
    Ok(!v.is_empty() && v != "0")
}

fn find_op(s: &str, op: &str) -> Option<usize> {
    let mut in_str = None;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + op.len() <= s.len() {
        let c = s[i..].chars().next().unwrap();
        if c == '\'' || c == '"' {
            if in_str == Some(c) {
                in_str = None;
            } else if in_str.is_none() {
                in_str = Some(c);
            }
            i += c.len_utf8();
            continue;
        }
        if in_str.is_none() && s[i..].starts_with(op) {
            return Some(i);
        }
        i += c.len_utf8();
        let _ = bytes;
    }
    None
}

fn eval_expr(
    host: &RefCell<RexxPipeHost<'_>>,
    vars: &HashMap<String, String>,
    expr: &str,
) -> PipeResult<String> {
    let expr = expr.trim();
    // Concatenation ||
    if let Some(idx) = find_op(expr, "||") {
        let l = eval_expr(host, vars, &expr[..idx])?;
        let r = eval_expr(host, vars, &expr[idx + 2..])?;
        return Ok(format!("{l}{r}"));
    }
    // Function calls
    if let Some(paren) = expr.find('(') {
        if expr.ends_with(')') {
            let fname = expr[..paren].trim().to_ascii_uppercase();
            let inside = &expr[paren + 1..expr.len() - 1];
            let args = split_args(inside);
            let evaled: PipeResult<Vec<String>> =
                args.iter().map(|a| eval_expr(host, vars, a)).collect();
            let evaled = evaled?;
            return match fname.as_str() {
                "LENGTH" => Ok(evaled
                    .first()
                    .map(|s| s.len().to_string())
                    .unwrap_or("0".into())),
                "SUBSTR" => {
                    let s = evaled.first().cloned().unwrap_or_default();
                    let start: usize = evaled.get(1).and_then(|x| x.parse().ok()).unwrap_or(1);
                    let len = evaled.get(2).and_then(|x| x.parse().ok());
                    let start = start.saturating_sub(1);
                    Ok(match len {
                        Some(n) => s.chars().skip(start).take(n).collect(),
                        None => s.chars().skip(start).collect(),
                    })
                }
                "POS" => {
                    let needle = evaled.first().cloned().unwrap_or_default();
                    let hay = evaled.get(1).cloned().unwrap_or_default();
                    Ok(hay
                        .find(&needle)
                        .map(|i| (i + 1).to_string())
                        .unwrap_or_else(|| "0".into()))
                }
                "STRIP" => Ok(evaled
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .trim()
                    .to_string()),
                "TRANSLATE" => {
                    // TRANSLATE(str) → uppercase
                    Ok(evaled
                        .first()
                        .cloned()
                        .unwrap_or_default()
                        .to_ascii_uppercase())
                }
                _ => Err(PipeError::stage(format!("REXX: unknown function {fname}"))),
            };
        }
    }
    // Quoted string
    if (expr.starts_with('\'') && expr.ends_with('\''))
        || (expr.starts_with('"') && expr.ends_with('"'))
    {
        return Ok(expr[1..expr.len() - 1].to_string());
    }
    // Number
    if expr.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Ok(expr.to_string());
    }
    // Variable
    let key = expr.to_ascii_uppercase();
    if let Some(v) = vars.get(&key) {
        return Ok(v.clone());
    }
    // Special: RC from host
    if key == "RC" {
        return Ok(host.borrow().rc.to_string());
    }
    Ok(String::new())
}

fn split_args(inside: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let mut in_str = None;
    for c in inside.chars() {
        if c == '\'' || c == '"' {
            if in_str == Some(c) {
                in_str = None;
            } else if in_str.is_none() {
                in_str = Some(c);
            }
            cur.push(c);
            continue;
        }
        if in_str.is_some() {
            cur.push(c);
            continue;
        }
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        args.push(cur.trim().to_string());
    }
    args
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let sl: String = s
        .chars()
        .take(prefix.len())
        .flat_map(|c| c.to_uppercase())
        .collect();
    let pl: String = prefix.chars().flat_map(|c| c.to_uppercase()).collect();
    if sl == pl {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_core::{PipelineBuilder, Record};
    use std::rc::Rc;

    #[test]
    fn rexx_uppercases_records() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);

        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "src",
            Box::new(|ctx| {
                ctx.write(Record::from("hello")).unwrap();
                ctx.write(Record::from("world")).unwrap();
            }),
        );
        let r = b.add_stage(
            "rexx",
            Box::new(|ctx| {
                let host = RefCell::new(RexxPipeHost::new(ctx));
                run_rexx_source(
                    &host,
                    r#"
                    DO FOREVER
                      'READTO LINE'
                      IF RC = 12 THEN LEAVE
                      'OUTPUT' TRANSLATE(LINE)
                    END
                    "#,
                    &[],
                )
                .unwrap();
            }),
        );
        let s = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(rec)) = ctx.read() {
                    out.borrow_mut().push(rec.to_string_lossy());
                }
            }),
        );
        b.connect_primary(a, r);
        b.connect_primary(r, s);
        b.run().unwrap();
        assert_eq!(
            *collected.borrow(),
            vec!["HELLO".to_string(), "WORLD".to_string()]
        );
    }

    #[test]
    fn cms_not_equal_operators() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "src",
            Box::new(|ctx| {
                ctx.write(Record::from("yes")).unwrap();
            }),
        );
        let r = b.add_stage(
            "rexx",
            Box::new(|ctx| {
                let host = RefCell::new(RexxPipeHost::new(ctx));
                run_rexx_source(
                    &host,
                    r#"
                    'READTO LINE'
                    IF LINE /= 'no' THEN 'OUTPUT' 'slash'
                    IF LINE ^= 'no' THEN 'OUTPUT' 'hat'
                    "#,
                    &[],
                )
                .unwrap();
            }),
        );
        let s = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(rec)) = ctx.read() {
                    out.borrow_mut().push(rec.to_string_lossy());
                }
            }),
        );
        b.connect_primary(a, r);
        b.connect_primary(r, s);
        b.run().unwrap();
        assert_eq!(
            *collected.borrow(),
            vec!["slash".to_string(), "hat".to_string()]
        );
    }

    #[test]
    fn readto_output_keeps_surrounding_blanks() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);

        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "src",
            Box::new(|ctx| {
                ctx.write(Record::from("  spaced  ")).unwrap();
                ctx.write(Record::from("  lead")).unwrap();
                ctx.write(Record::from("trail  ")).unwrap();
                ctx.write(Record::from("")).unwrap();
            }),
        );
        let r = b.add_stage(
            "rexx",
            Box::new(|ctx| {
                let host = RefCell::new(RexxPipeHost::new(ctx));
                run_rexx_source(
                    &host,
                    r#"
                    DO FOREVER
                      'READTO LINE'
                      IF RC = 12 THEN LEAVE
                      'OUTPUT' LINE
                    END
                    "#,
                    &[],
                )
                .unwrap();
            }),
        );
        let s = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(rec)) = ctx.read() {
                    out.borrow_mut().push(rec.to_string_lossy());
                }
            }),
        );
        b.connect_primary(a, r);
        b.connect_primary(r, s);
        b.run().unwrap();
        assert_eq!(
            *collected.borrow(),
            vec![
                "  spaced  ".to_string(),
                "  lead".to_string(),
                "trail  ".to_string(),
                String::new()
            ]
        );
    }

    #[test]
    fn falling_off_the_end_does_not_use_readto_eof_rc() {
        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "src",
            Box::new(|ctx| {
                ctx.write(Record::from("x")).unwrap();
            }),
        );
        let r = b.add_stage(
            "rexx",
            Box::new(|ctx| {
                let host = RefCell::new(RexxPipeHost::new(ctx));
                run_rexx_source(
                    &host,
                    r#"
                    'READTO LINE'
                    'READTO LINE'
                    "#,
                    &[],
                )
                .unwrap();
                host.borrow_mut().apply_stage_exit();
            }),
        );
        let s = b.add_stage(
            "sink",
            Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {}),
        );
        b.connect_primary(a, r);
        b.connect_primary(r, s);
        let rc = b.run_with_return_code().unwrap();
        assert_eq!(rc, 0);
    }

    #[test]
    fn exit_n_sets_stage_return_code() {
        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "src",
            Box::new(|ctx| {
                ctx.write(Record::from("x")).unwrap();
            }),
        );
        let r = b.add_stage(
            "rexx",
            Box::new(|ctx| {
                let host = RefCell::new(RexxPipeHost::new(ctx));
                run_rexx_source(&host, "EXIT 7\n", &[]).unwrap();
                host.borrow_mut().apply_stage_exit();
            }),
        );
        let s = b.add_stage("sink", Box::new(|_ctx| {}));
        b.connect_primary(a, r);
        b.connect_primary(r, s);
        let rc = b.run_with_return_code().unwrap();
        assert_eq!(rc, 7);
    }
}
