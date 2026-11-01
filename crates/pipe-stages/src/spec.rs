//! SPEC / SPECS — rearrange record contents.
//!
//! Author's Edition 1.1.11 Chapter 19 pp.496–502, Chapter 14 tutorial
//! pp.153–162, and Chapter 20 reference pp.615–628.
//!
//! Field identifiers (`a:1`), counters, IF/THEN/ELSE/ENDIF, SET, PRINT,
//! PICTURE, and NOWRITE are supported. SELECT, TODCLOCK, extra streams,
//! and packed/float/date conversions are not.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_delimited, matches_abbrev};
use crate::input_range::{
    extract_input_range, is_field_separator_kw, is_word_separator_kw, looks_like_input_range_start,
    parse_input_range_with, parse_xorc, InputRange, FIELD_SEP, WORD_SEP,
};
use crate::spec_expr::{
    apply_assign, eval_expr, format_picture, parse_assign, parse_expr, Assign, Expr, SpecEnv,
};
use crate::xrange::parse_snumber;

/// SPEC subset — IBM verb `SPEC` (keyword synonym `SPECS`).
pub fn spec(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let parsed = match parse_spec(&args) {
            Ok(p) => p,
            Err(e) => {
                ctx.fail(format!("SPEC: {e}"));
                return;
            }
        };

        if ctx.output_connected(StreamId::SECONDARY) {
            ctx.fail("SPEC: no output stream other than the primary may be connected");
            return;
        }
        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        run_spec(ctx, parsed);
    })
}

#[derive(Clone, Debug)]
struct ParsedSpec {
    items: Vec<Item>,
}

#[derive(Clone, Debug)]
enum Item {
    Pad(u8),
    Write,
    Read,
    ReadStop,
    Field(DataField),
    If {
        cond: Expr,
        then_items: Vec<Item>,
        else_items: Vec<Item>,
    },
    Set(Assign),
    Print {
        expr: Expr,
        picture: Option<String>,
        dest: Dest,
    },
    NoWrite,
}

#[derive(Clone, Debug)]
struct DataField {
    id: Option<String>,
    source: Source,
    strip: bool,
    conv: Option<Conversion>,
    dest: Dest,
    align: Option<Align>,
}

#[derive(Clone, Debug)]
enum Source {
    Range(InputRange),
    Literal(Vec<u8>),
    Number { from: i64, by: i64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Conversion {
    C2X,
    X2C,
    C2B,
    B2C,
    X2B,
    B2X,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Left,
    Centre,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dest {
    /// Filled by Note 4 (single field, no placement, no PAD/READ/WRITE) or rejected.
    Pending,
    /// Period: do not insert (p.622).
    Ignore,
    /// 1-based start; `len` is an explicit width (`NEXT.n`, `n.m`, `n-m`).
    At {
        start: usize,
        len: Option<usize>,
    },
    Next {
        len: Option<usize>,
    },
    NextWord {
        len: Option<usize>,
    },
    NextField {
        len: Option<usize>,
    },
}

fn parse_spec(args: &[String]) -> Result<ParsedSpec, String> {
    if args.is_empty() {
        return Err("a specification list of one or more items is required".into());
    }

    let mut i = 0;
    let mut word_sep = WORD_SEP;
    let mut field_sep = FIELD_SEP;
    let mut saw_pad = false;
    let mut saw_rw = false;
    let mut data_fields = 0usize;
    let items = parse_item_list(
        args,
        &mut i,
        &mut word_sep,
        &mut field_sep,
        &mut saw_pad,
        &mut saw_rw,
        &mut data_fields,
        &[],
    )?;
    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    // Note 4 p.502: a single field with no output column and without PAD /
    // SELECT / READ / WRITE defaults to column 1.
    let mut items = items;
    apply_note4_default(&mut items, data_fields, saw_pad, saw_rw)?;
    Ok(ParsedSpec { items })
}

#[allow(clippy::too_many_arguments)]
fn parse_item_list(
    args: &[String],
    i: &mut usize,
    word_sep: &mut u8,
    field_sep: &mut u8,
    saw_pad: &mut bool,
    saw_rw: &mut bool,
    data_fields: &mut usize,
    until: &[&str],
) -> Result<Vec<Item>, String> {
    let mut items = Vec::new();
    while *i < args.len() {
        let upper = args[*i].to_ascii_uppercase();
        if until.iter().any(|s| *s == upper) {
            break;
        }
        if let Some(msg) = later_increment_keyword(&upper) {
            return Err(msg);
        }
        if upper == "THEN" || upper == "ELSE" || is_endif(&upper) {
            return Err(format!("{upper} is not valid here"));
        }
        if upper == "IF" {
            items.push(parse_if(
                args,
                i,
                word_sep,
                field_sep,
                saw_pad,
                saw_rw,
                data_fields,
            )?);
            *saw_rw = true;
            continue;
        }
        if upper == "SET" {
            *i += 1;
            items.push(Item::Set(take_assign(args, i)?));
            *saw_rw = true;
            continue;
        }
        if upper == "PRINT" {
            items.push(parse_print(args, i, *word_sep, *field_sep)?);
            *saw_rw = true;
            continue;
        }
        if upper == "NOWRITE" {
            items.push(Item::NoWrite);
            *i += 1;
            *saw_rw = true;
            continue;
        }
        if upper == "PAD" {
            *i += 1;
            if *i >= args.len() {
                return Err("PAD requires an xorc".into());
            }
            items.push(Item::Pad(parse_xorc(&args[*i])?));
            *i += 1;
            *saw_pad = true;
            continue;
        }
        if upper == "WRITE" {
            items.push(Item::Write);
            *i += 1;
            *saw_rw = true;
            continue;
        }
        if upper == "READ" {
            items.push(Item::Read);
            *i += 1;
            *saw_rw = true;
            continue;
        }
        if upper == "READSTOP" {
            items.push(Item::ReadStop);
            *i += 1;
            *saw_rw = true;
            continue;
        }
        if is_word_separator_kw(&args[*i]) {
            *i += 1;
            if *i >= args.len() {
                return Err("WORDSEPARATOR requires an xorc".into());
            }
            *word_sep = parse_xorc(&args[*i])?;
            *i += 1;
            continue;
        }
        if is_field_separator_kw(&args[*i]) {
            *i += 1;
            if *i >= args.len() {
                return Err("FIELDSEPARATOR requires an xorc".into());
            }
            *field_sep = parse_xorc(&args[*i])?;
            *i += 1;
            continue;
        }

        let field = parse_data_field(args, i, *word_sep, *field_sep)?;
        *data_fields += 1;
        items.push(Item::Field(field));
    }
    Ok(items)
}

fn is_endif(upper: &str) -> bool {
    upper == "ENDIF" || upper == "FI" || upper == "IFEND"
}

fn parse_if(
    args: &[String],
    i: &mut usize,
    word_sep: &mut u8,
    field_sep: &mut u8,
    saw_pad: &mut bool,
    saw_rw: &mut bool,
    data_fields: &mut usize,
) -> Result<Item, String> {
    *i += 1; // IF
    let cond = take_expr_until(args, i, &["THEN"])?;
    expect_kw(args, i, "THEN")?;
    let then_items = parse_item_list(
        args,
        i,
        word_sep,
        field_sep,
        saw_pad,
        saw_rw,
        data_fields,
        &["ELSE", "ENDIF", "FI", "IFEND"],
    )?;
    let mut else_items = Vec::new();
    if *i < args.len() && args[*i].eq_ignore_ascii_case("ELSE") {
        *i += 1;
        else_items = parse_item_list(
            args,
            i,
            word_sep,
            field_sep,
            saw_pad,
            saw_rw,
            data_fields,
            &["ENDIF", "FI", "IFEND"],
        )?;
    }
    if *i >= args.len() || !is_endif(&args[*i].to_ascii_uppercase()) {
        return Err("IF requires ENDIF".into());
    }
    *i += 1;
    Ok(Item::If {
        cond,
        then_items,
        else_items,
    })
}

fn parse_print(
    args: &[String],
    i: &mut usize,
    word_sep: u8,
    field_sep: u8,
) -> Result<Item, String> {
    *i += 1; // PRINT
    let expr = take_expr_until(
        args,
        i,
        &[
            "PICTURE", "NOWRITE", "SET", "IF", "ENDIF", "FI", "IFEND", "ELSE", "THEN",
        ],
    )?;
    let mut picture = None;
    if *i < args.len() && args[*i].eq_ignore_ascii_case("PICTURE") {
        *i += 1;
        if *i >= args.len() {
            return Err("PICTURE requires a picture string".into());
        }
        picture = Some(args[*i].clone());
        *i += 1;
    }
    let dest = if *i < args.len() {
        if args[*i] == "." {
            *i += 1;
            Dest::Ignore
        } else if let Some(d) = parse_dest(&args[*i])? {
            *i += 1;
            d
        } else {
            Dest::Next { len: None }
        }
    } else {
        Dest::Next { len: None }
    };
    let _ = (word_sep, field_sep);
    Ok(Item::Print {
        expr,
        picture,
        dest,
    })
}

fn expect_kw(args: &[String], i: &mut usize, kw: &str) -> Result<(), String> {
    if *i < args.len() && args[*i].eq_ignore_ascii_case(kw) {
        *i += 1;
        Ok(())
    } else {
        Err(format!("IF requires {kw}"))
    }
}

fn take_expr_until(args: &[String], i: &mut usize, stop: &[&str]) -> Result<Expr, String> {
    let start = *i;
    while *i < args.len() {
        let upper = args[*i].to_ascii_uppercase();
        if stop.iter().any(|s| *s == upper) {
            break;
        }
        *i += 1;
    }
    if start == *i {
        return Err("expression is missing".into());
    }
    let joined = args[start..*i].join(" ");
    parse_expr(&joined)
}

fn take_assign(args: &[String], i: &mut usize) -> Result<Assign, String> {
    if *i >= args.len() {
        return Err("SET requires an assignment".into());
    }
    // One token (`#0:=0` or `(#0+=a)`) or a short run until the next keyword.
    let start = *i;
    *i += 1;
    while *i < args.len() {
        let upper = args[*i].to_ascii_uppercase();
        if is_item_keyword(&upper)
            || looks_like_input_range_start(&args[*i])
            || split_field_id(&args[*i]).is_some()
        {
            break;
        }
        if args[*i] == "." || parse_dest(&args[*i]).ok().flatten().is_some() {
            break;
        }
        *i += 1;
    }
    parse_assign(&args[start..*i].join(" "))
}

fn is_item_keyword(upper: &str) -> bool {
    matches!(
        upper,
        "IF" | "THEN"
            | "ELSE"
            | "ENDIF"
            | "FI"
            | "IFEND"
            | "SET"
            | "PRINT"
            | "PICTURE"
            | "NOWRITE"
            | "PAD"
            | "WRITE"
            | "READ"
            | "READSTOP"
    ) || later_increment_keyword(upper).is_some()
}

/// `a:1` → (`A`, `1`); `a:` → (`A`, ``).
fn split_field_id(tok: &str) -> Option<(String, String)> {
    let colon = tok.find(':')?;
    if colon == 0 {
        return None;
    }
    let id = &tok[..colon];
    if !is_field_id_name(id) {
        return None;
    }
    Some((id.to_ascii_uppercase(), tok[colon + 1..].to_string()))
}

fn is_field_id_name(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && chars.all(|c| c.is_ascii_alphanumeric())
}

fn apply_note4_default(
    items: &mut [Item],
    data_fields: usize,
    saw_pad: bool,
    saw_rw: bool,
) -> Result<(), String> {
    let mut pending = None;
    for (idx, item) in items.iter().enumerate() {
        if let Item::Field(f) = item {
            if matches!(f.dest, Dest::Pending) {
                pending = Some(idx);
            }
        }
    }
    if let Some(idx) = pending {
        if data_fields == 1 && !saw_pad && !saw_rw {
            if let Item::Field(f) = &mut items[idx] {
                f.dest = Dest::At {
                    start: 1,
                    len: None,
                };
            }
            return Ok(());
        }
        return Err("output placement is required".into());
    }
    Ok(())
}

fn parse_data_field(
    args: &[String],
    i: &mut usize,
    word_sep: u8,
    field_sep: u8,
) -> Result<DataField, String> {
    if *i >= args.len() {
        return Err("incomplete specification item".into());
    }

    let mut field_id = None;
    if let Some((id, rest)) = split_field_id(&args[*i]) {
        *i += 1;
        field_id = Some(id);
        if !rest.is_empty() {
            let mut tmp = vec![rest];
            tmp.extend(args[*i..].iter().cloned());
            let mut j = 0;
            let field = parse_data_field_body(&tmp, &mut j, word_sep, field_sep, field_id)?;
            *i += j.saturating_sub(1);
            return Ok(field);
        }
    }

    parse_data_field_body(args, i, word_sep, field_sep, field_id)
}

fn parse_data_field_body(
    args: &[String],
    i: &mut usize,
    word_sep: u8,
    field_sep: u8,
    field_id: Option<String>,
) -> Result<DataField, String> {
    if *i >= args.len() {
        return Err("incomplete specification item".into());
    }

    let source = parse_source(args, i, word_sep, field_sep)?;

    let mut strip = false;
    if *i < args.len() && args[*i].eq_ignore_ascii_case("STRIP") {
        strip = true;
        *i += 1;
    }

    let mut conv = None;
    if *i < args.len() {
        if let Some(c) = parse_conversion(&args[*i]) {
            conv = Some(c);
            *i += 1;
        } else if let Some(msg) = unimplemented_conversion(&args[*i]) {
            return Err(msg);
        }
    }

    let dest = if *i < args.len() {
        if args[*i] == "." {
            *i += 1;
            Dest::Ignore
        } else if let Some(d) = parse_dest(&args[*i])? {
            *i += 1;
            d
        } else {
            Dest::Pending
        }
    } else {
        Dest::Pending
    };

    let mut align = None;
    if *i < args.len() && !matches!(dest, Dest::Pending | Dest::Ignore) {
        if let Some(a) = parse_align(&args[*i]) {
            align = Some(a);
            *i += 1;
        }
    }

    if matches!(dest, Dest::Pending) && *i < args.len() && parse_align(&args[*i]).is_some() {
        return Err("output placement is required before LEFT/CENTRE/RIGHT".into());
    }

    Ok(DataField {
        id: field_id,
        source,
        strip,
        conv,
        dest,
        align,
    })
}

fn parse_source(
    args: &[String],
    i: &mut usize,
    word_sep: u8,
    field_sep: u8,
) -> Result<Source, String> {
    let tok = &args[*i];
    let upper = tok.to_ascii_uppercase();
    if upper == "NUMBER" || upper == "RECNO" {
        *i += 1;
        let mut from = 1i64;
        let mut by = 1i64;
        if *i < args.len() && args[*i].eq_ignore_ascii_case("FROM") {
            *i += 1;
            if *i >= args.len() {
                return Err("NUMBER FROM requires an snumber".into());
            }
            from = i64::from(parse_snumber(&args[*i])?);
            *i += 1;
        }
        if *i < args.len() && args[*i].eq_ignore_ascii_case("BY") {
            *i += 1;
            if *i >= args.len() {
                return Err("NUMBER BY requires an snumber".into());
            }
            by = i64::from(parse_snumber(&args[*i])?);
            *i += 1;
        }
        return Ok(Source::Number { from, by });
    }
    if is_delimited(tok) {
        let bytes = decode_dstring(tok)?;
        *i += 1;
        return Ok(Source::Literal(bytes));
    }
    if looks_like_input_range_start(tok) {
        let (range, _, _) = parse_input_range_with(args, i, word_sep, field_sep)?;
        return Ok(Source::Range(range));
    }
    Err(unexpected_operand(tok))
}

fn parse_conversion(s: &str) -> Option<Conversion> {
    match s.to_ascii_uppercase().as_str() {
        "C2X" => Some(Conversion::C2X),
        "X2C" => Some(Conversion::X2C),
        "C2B" => Some(Conversion::C2B),
        "B2C" => Some(Conversion::B2C),
        "X2B" => Some(Conversion::X2B),
        "B2X" => Some(Conversion::B2X),
        _ => None,
    }
}

fn unimplemented_conversion(s: &str) -> Option<String> {
    let u = s.to_ascii_uppercase();
    if u.len() >= 3 && u.as_bytes().get(1) == Some(&b'2') {
        let from = u.as_bytes()[0];
        let to = u.as_bytes()[2];
        const SET: &[u8] = b"BCDFIPVXT";
        if SET.contains(&from) && SET.contains(&to) {
            return Some(format!("{u} conversion is not implemented"));
        }
    }
    None
}

fn parse_align(s: &str) -> Option<Align> {
    let u = s.to_ascii_uppercase();
    if matches_abbrev(&u, "LEFT", 1) {
        Some(Align::Left)
    } else if matches_abbrev(&u, "CENTRE", 1) || matches_abbrev(&u, "CENTER", 1) {
        Some(Align::Centre)
    } else if matches_abbrev(&u, "RIGHT", 1) {
        Some(Align::Right)
    } else {
        None
    }
}

fn parse_dest(tok: &str) -> Result<Option<Dest>, String> {
    if let Some(d) = parse_next_dest(tok)? {
        return Ok(Some(d));
    }
    parse_output_range(tok)
}

fn parse_next_dest(tok: &str) -> Result<Option<Dest>, String> {
    let (head, len) = match tok.split_once('.') {
        Some((h, n)) => {
            let len = n
                .parse::<usize>()
                .map_err(|_| format!("output field length must be a number, got {n}"))?;
            if len == 0 {
                return Err("output field length must be positive".into());
            }
            (h, Some(len))
        }
        None => (tok, None),
    };
    let u = head.to_ascii_uppercase();
    if is_nextfield(&u) {
        Ok(Some(Dest::NextField { len }))
    } else if is_nextword(&u) {
        Ok(Some(Dest::NextWord { len }))
    } else if is_next(&u) {
        Ok(Some(Dest::Next { len }))
    } else if len.is_some() {
        // `5.3` is an output range, not NEXT.n.
        Ok(None)
    } else {
        Ok(None)
    }
}

fn is_nextfield(u: &str) -> bool {
    matches_abbrev(u, "NEXTFIELD", 5) || matches_abbrev(u, "NFIELD", 2)
}

fn is_nextword(u: &str) -> bool {
    matches_abbrev(u, "NEXTWORD", 5) || matches_abbrev(u, "NWORD", 2)
}

fn is_next(u: &str) -> bool {
    matches_abbrev(u, "NEXT", 1)
}

fn parse_output_range(tok: &str) -> Result<Option<Dest>, String> {
    if tok.contains('*') {
        // Note 6 p.502: asterisk is rejected for the ending column; we reject
        // `*` in any output range so `1-*` cannot be an output placement.
        if tok.contains('-') || tok.contains('.') {
            return Err("an asterisk is rejected in an output specification".into());
        }
    }
    if let Some((left, right)) = tok.split_once('-') {
        if left.is_empty() || right.is_empty() {
            return Ok(None);
        }
        let start = parse_output_column(left)?;
        let end = parse_output_column(right)?;
        if end < start {
            return Err(format!(
                "ending output column is before the beginning ({start}-{end})"
            ));
        }
        return Ok(Some(Dest::At {
            start,
            len: Some(end - start + 1),
        }));
    }
    if let Some((left, right)) = tok.split_once('.') {
        if left.is_empty() || right.is_empty() {
            return Ok(None);
        }
        if !left.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(None);
        }
        let start = parse_output_column(left)?;
        let len = right
            .parse::<usize>()
            .map_err(|_| format!("output field length must be a number, got {right}"))?;
        if len == 0 {
            return Err("output field length must be positive".into());
        }
        return Ok(Some(Dest::At {
            start,
            len: Some(len),
        }));
    }
    if tok.bytes().all(|b| b.is_ascii_digit()) && !tok.is_empty() {
        let start = parse_output_column(tok)?;
        return Ok(Some(Dest::At { start, len: None }));
    }
    Ok(None)
}

fn parse_output_column(s: &str) -> Result<usize, String> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("output column must be a positive number, got {s}"));
    }
    let n: usize = s
        .parse()
        .map_err(|_| format!("output column {s} is out of range"))?;
    if n == 0 {
        return Err("output column must be a positive number".into());
    }
    Ok(n)
}

fn later_increment_keyword(upper: &str) -> Option<String> {
    const LATER: &[&str] = &[
        "SELECT",
        "OUTSTREAM",
        "TODCLOCK",
        "ELSEIF",
        "ELSIF",
        "OTHERWISE",
        "BREAK",
        "EOF",
        "PRINTONLY",
        "STOP",
        "ID",
    ];
    if LATER.contains(&upper) {
        Some(format!("{upper} is not implemented"))
    } else {
        None
    }
}

fn unexpected_operand(arg: &str) -> String {
    format!("unexpected operand {arg}")
}

fn run_spec(ctx: &mut StageCtx, parsed: ParsedSpec) {
    let mut numbers: Vec<i64> = Vec::new();
    collect_number_froms(&parsed.items, &mut numbers);
    let mut env = SpecEnv::default();

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    loop {
        let rec = match ctx.read() {
            Ok(Some(r)) => r,
            Ok(None) | Err(_) => return,
        };
        if !apply_cycle(ctx, &parsed.items, rec.as_bytes(), &mut numbers, &mut env) {
            // Downstream TAKE (or other SHORT) closed output; drop leftover input.
            if ctx.input_connected(StreamId::PRIMARY) {
                ctx.short_input();
            }
            return;
        }
        bump_numbers(&parsed.items, &mut numbers);
    }
}

fn collect_number_froms(items: &[Item], numbers: &mut Vec<i64>) {
    for item in items {
        match item {
            Item::Field(DataField {
                source: Source::Number { from, .. },
                ..
            }) => numbers.push(*from),
            Item::If {
                then_items,
                else_items,
                ..
            } => {
                collect_number_froms(then_items, numbers);
                collect_number_froms(else_items, numbers);
            }
            _ => {}
        }
    }
}

fn bump_numbers(items: &[Item], numbers: &mut [i64]) {
    let mut n = 0;
    bump_numbers_inner(items, numbers, &mut n);
}

fn bump_numbers_inner(items: &[Item], numbers: &mut [i64], n: &mut usize) {
    for item in items {
        match item {
            Item::Field(DataField {
                source: Source::Number { by, .. },
                ..
            }) => {
                if let Some(slot) = numbers.get_mut(*n) {
                    *slot = slot.saturating_add(*by);
                }
                *n += 1;
            }
            Item::If {
                then_items,
                else_items,
                ..
            } => {
                bump_numbers_inner(then_items, numbers, n);
                bump_numbers_inner(else_items, numbers, n);
            }
            _ => {}
        }
    }
}

struct CycleState<'a> {
    current: Vec<u8>,
    out: Vec<u8>,
    pad: u8,
    wrote_via_write: bool,
    number_i: usize,
    skip_final: bool,
    nowrite: bool,
    numbers: &'a mut [i64],
    env: &'a mut SpecEnv,
}

fn apply_cycle(
    ctx: &mut StageCtx,
    items: &[Item],
    first: &[u8],
    numbers: &mut [i64],
    env: &mut SpecEnv,
) -> bool {
    let mut state = CycleState {
        current: first.to_vec(),
        out: Vec::new(),
        pad: b' ',
        wrote_via_write: false,
        number_i: 0,
        skip_final: false,
        nowrite: false,
        numbers,
        env,
    };

    if !apply_items(ctx, items, &mut state) {
        return false;
    }

    if state.skip_final || state.nowrite {
        return true;
    }
    if state.wrote_via_write && state.out.is_empty() {
        return true;
    }
    ctx.write(Record::from(state.out)).is_ok()
}

fn apply_items(ctx: &mut StageCtx, items: &[Item], state: &mut CycleState<'_>) -> bool {
    for item in items {
        match item {
            Item::Pad(p) => state.pad = *p,
            Item::Write => {
                if ctx
                    .write(Record::from(std::mem::take(&mut state.out)))
                    .is_err()
                {
                    return false;
                }
                state.wrote_via_write = true;
            }
            Item::Read => match ctx.read() {
                Ok(Some(r)) => state.current = r.as_bytes().to_vec(),
                Ok(None) | Err(_) => state.current = Vec::new(),
            },
            Item::ReadStop => match ctx.read() {
                Ok(Some(r)) => state.current = r.as_bytes().to_vec(),
                Ok(None) | Err(_) => {
                    if ctx
                        .write(Record::from(std::mem::take(&mut state.out)))
                        .is_err()
                    {
                        return false;
                    }
                    state.skip_final = true;
                    break;
                }
            },
            Item::Field(field) => {
                let src = match &field.source {
                    Source::Range(range) => extract_input_range(&state.current, range),
                    Source::Literal(b) => b.clone(),
                    Source::Number { .. } => {
                        let n = state.numbers.get(state.number_i).copied().unwrap_or(1);
                        state.number_i += 1;
                        format_number(n)
                    }
                };
                if let Some(id) = &field.id {
                    state.env.fields.insert(id.clone(), src.clone());
                }
                let is_literal = matches!(field.source, Source::Literal(_));
                if let Err(e) = place_field(&mut state.out, field, src, is_literal, state.pad) {
                    ctx.fail(format!("SPEC: {e}"));
                    return false;
                }
                if !matches!(field.dest, Dest::Ignore | Dest::Pending) {
                    state.wrote_via_write = false;
                }
            }
            Item::If {
                cond,
                then_items,
                else_items,
            } => {
                let yes = match eval_expr(cond, state.env) {
                    Ok(v) => v.as_bool(),
                    Err(e) => {
                        ctx.fail(format!("SPEC: {e}"));
                        return false;
                    }
                };
                let branch = if yes { then_items } else { else_items };
                if !apply_items(ctx, branch, state) {
                    return false;
                }
            }
            Item::Set(assign) => {
                if let Err(e) = apply_assign(assign, state.env) {
                    ctx.fail(format!("SPEC: {e}"));
                    return false;
                }
            }
            Item::Print {
                expr,
                picture,
                dest,
            } => {
                let value = match eval_expr(expr, state.env) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.fail(format!("SPEC: {e}"));
                        return false;
                    }
                };
                let bytes = match picture {
                    Some(pic) => format_picture(&value, pic),
                    None => value.as_bytes(),
                };
                let field = DataField {
                    id: None,
                    source: Source::Literal(bytes.clone()),
                    strip: false,
                    conv: None,
                    dest: *dest,
                    align: None,
                };
                if let Err(e) = place_field(&mut state.out, &field, bytes, true, state.pad) {
                    ctx.fail(format!("SPEC: {e}"));
                    return false;
                }
                if !matches!(dest, Dest::Ignore | Dest::Pending) {
                    state.wrote_via_write = false;
                }
            }
            Item::NoWrite => state.nowrite = true,
        }
    }
    true
}

fn place_field(
    out: &mut Vec<u8>,
    field: &DataField,
    mut data: Vec<u8>,
    is_literal: bool,
    pad: u8,
) -> Result<(), String> {
    if field.strip {
        data = strip_blanks(&data);
    }
    let data = match field.conv {
        Some(c) => convert(&data, c)?,
        None => data,
    };

    if matches!(field.dest, Dest::Ignore | Dest::Pending) {
        return Ok(());
    }

    let explicit = dest_len(&field.dest);
    let default_len = data.len();
    // Null input (and STRIP-to-null) with no explicit length is ignored,
    // except a null *literal* is stored (p.499).
    if default_len == 0 && explicit.is_none() && !is_literal {
        return Ok(());
    }

    let field_len = explicit.unwrap_or(default_len);
    let to_place = if field.align.is_some() && !keep_blanks_for_conversion(field.conv) {
        strip_blanks(&data)
    } else {
        data
    };
    let placed = align_into(
        field_len,
        &to_place,
        field.align.unwrap_or(Align::Left),
        pad,
    );
    put_dest(out, field.dest, &placed, pad);
    Ok(())
}

fn dest_len(dest: &Dest) -> Option<usize> {
    match dest {
        Dest::At { len, .. }
        | Dest::Next { len }
        | Dest::NextWord { len }
        | Dest::NextField { len } => *len,
        Dest::Ignore | Dest::Pending => None,
    }
}

fn keep_blanks_for_conversion(conv: Option<Conversion>) -> bool {
    // p.499 / p.628: D2C F2C I2C P2C V2C skip the placement strip. Those
    // conversions are not in this increment.
    let _ = conv;
    false
}

fn put_dest(out: &mut Vec<u8>, dest: Dest, data: &[u8], pad: u8) {
    match dest {
        Dest::Ignore | Dest::Pending => {}
        Dest::At { start, .. } => put_at(out, start, data, pad),
        Dest::Next { .. } => {
            out.extend_from_slice(data);
        }
        Dest::NextWord { .. } => {
            if !out.is_empty() {
                out.push(b' ');
            }
            out.extend_from_slice(data);
        }
        Dest::NextField { .. } => {
            if !out.is_empty() {
                out.push(b'\t');
            }
            out.extend_from_slice(data);
        }
    }
}

fn put_at(out: &mut Vec<u8>, start_col: usize, data: &[u8], pad: u8) {
    if start_col == 0 {
        return;
    }
    let start = start_col - 1;
    if out.len() < start {
        out.resize(start, pad);
    }
    let end = start + data.len();
    if out.len() < end {
        out.resize(end, pad);
    }
    out[start..end].copy_from_slice(data);
}

fn align_into(field_len: usize, data: &[u8], align: Align, pad: u8) -> Vec<u8> {
    if field_len == 0 {
        return Vec::new();
    }
    let mut out = vec![pad; field_len];
    match align {
        Align::Left => {
            let n = data.len().min(field_len);
            out[..n].copy_from_slice(&data[..n]);
        }
        Align::Right => {
            let n = data.len().min(field_len);
            out[field_len - n..].copy_from_slice(&data[data.len() - n..]);
        }
        Align::Centre => {
            if data.len() <= field_len {
                let extra = field_len - data.len();
                let right = extra.div_ceil(2);
                let left = extra - right;
                out[left..left + data.len()].copy_from_slice(data);
            } else {
                let extra = data.len() - field_len;
                let left_cut = extra.div_ceil(2);
                out.copy_from_slice(&data[left_cut..left_cut + field_len]);
            }
        }
    }
    out
}

fn strip_blanks(data: &[u8]) -> Vec<u8> {
    let start = data.iter().position(|&b| b != b' ').unwrap_or(data.len());
    let end = data
        .iter()
        .rposition(|&b| b != b' ')
        .map(|i| i + 1)
        .unwrap_or(0);
    if start >= end {
        Vec::new()
    } else {
        data[start..end].to_vec()
    }
}

fn format_number(n: i64) -> Vec<u8> {
    let body = n.to_string();
    if body.len() >= 10 {
        body.as_bytes()[body.len() - 10..].to_vec()
    } else {
        let mut out = vec![b' '; 10];
        let start = 10 - body.len();
        out[start..].copy_from_slice(body.as_bytes());
        out
    }
}

fn convert(data: &[u8], conv: Conversion) -> Result<Vec<u8>, String> {
    match conv {
        Conversion::C2X => Ok(c2x(data)),
        Conversion::X2C => x2c(data),
        Conversion::C2B => Ok(c2b(data)),
        Conversion::B2C => b2c(data),
        Conversion::X2B => x2c(data).map(|b| c2b(&b)),
        Conversion::B2X => b2c(data).map(|b| c2x(&b)),
    }
}

fn c2x(data: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(data.len() * 2);
    for &b in data {
        out.push(HEX[(b >> 4) as usize]);
        out.push(HEX[(b & 0x0f) as usize]);
    }
    out
}

fn x2c(data: &[u8]) -> Result<Vec<u8>, String> {
    // Blanks allowed at byte boundaries internally, not at either end (p.626).
    if data.first() == Some(&b' ') || data.last() == Some(&b' ') {
        return Err("X2C does not allow leading or trailing blanks".into());
    }
    let mut hex = Vec::new();
    let mut i = 0;
    while i < data.len() {
        if data[i] == b' ' {
            if hex.len() % 2 != 0 {
                return Err("X2C allows blanks only at byte boundaries".into());
            }
            i += 1;
            continue;
        }
        hex.push(data[i]);
        i += 1;
    }
    if hex.len() % 2 != 0 {
        return Err("X2C requires an even number of hex digits".into());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.chunks(2) {
        let s = std::str::from_utf8(chunk).map_err(|_| "X2C requires hex digits")?;
        out.push(u8::from_str_radix(s, 16).map_err(|_| "X2C requires hex digits")?);
    }
    Ok(out)
}

fn c2b(data: &[u8]) -> Vec<u8> {
    // Book C2B writes EBCDIC 0/1 (X'F0'/X'F1'); portable ASCII uses '0'/'1'.
    let mut out = Vec::with_capacity(data.len() * 8);
    for &b in data {
        for bit in (0..8).rev() {
            out.push(if (b >> bit) & 1 == 1 { b'1' } else { b'0' });
        }
    }
    out
}

fn b2c(data: &[u8]) -> Result<Vec<u8>, String> {
    if !data.len().is_multiple_of(8) {
        return Err("B2C length must be a multiple of eight".into());
    }
    if !data.iter().all(|&b| b == b'0' || b == b'1') {
        return Err("B2C input must be the characters 0 and 1".into());
    }
    let mut out = Vec::with_capacity(data.len() / 8);
    for chunk in data.chunks(8) {
        let mut b = 0u8;
        for &bit in chunk {
            b = (b << 1) | (bit - b'0');
        }
        out.push(b);
    }
    Ok(out)
}
