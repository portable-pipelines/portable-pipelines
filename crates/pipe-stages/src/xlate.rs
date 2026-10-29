//! XLATE — transliterate contents of records.
//!
//! Author's Edition 1.1.11 pp.604–607 (tutorial pp.44–46).
//!
//! Syntax (fixed order; diagram p.604):
//! `XLATE | TRANSlate [inputRange | ( inputRange … )] [default-table …]
//!        [xrange xrange …]`
//!
//! `default-table` is `INput` / `OUTput` / `UPper` / `LOWer`, or the rejected
//! code-page forms `TO`/`FROM` `[CODEPAGE] number` and the Note 1 names
//! `A2E` / `E2A` (p.607).

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::matches_abbrev;
use crate::input_range::{
    input_range_positions, looks_like_input_range_start, parse_input_range, InputRange,
};
use crate::xrange::expand_xrange;

pub fn xlate(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_xlate(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("XLATE: {e}"));
                return;
            }
        };

        if ctx.output_connected(StreamId::SECONDARY) {
            ctx.fail("XLATE: secondary output stream must not be connected");
            return;
        }
        for stream in ctx.connected_inputs() {
            if stream.0 > StreamId::SECONDARY.0 {
                ctx.fail(format!(
                    "XLATE: input stream {stream} must not be connected"
                ));
                return;
            }
        }

        let table = match build_table(ctx, &spec) {
            Ok(t) => t,
            Err(e) => {
                ctx.fail(format!("XLATE: {e}"));
                return;
            }
        };

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        ctx.select_output(StreamId::PRIMARY);
        run_xlate(ctx, &spec, &table);
    })
}

struct XlateSpec {
    ranges: Vec<InputRange>,
    /// Default-table keywords in diagram order (`UPPER` / `LOWER` / `INPUT` /
    /// `OUTPUT`). Composed left to right (p.604).
    tables: Vec<NamedTable>,
    pairs: Vec<(Vec<u8>, Vec<u8>)>,
    /// UPPER when there are no table keywords and no pairs, and the secondary
    /// input is not defined (p.604).
    default_upper: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NamedTable {
    Upper,
    Lower,
    /// CMS SET INPUT / SET OUTPUT; no SET is in effect, so identity (p.604).
    Identity,
}

fn parse_xlate(args: &[String]) -> Result<XlateSpec, String> {
    let mut i = 0;
    let ranges = parse_ranges(args, &mut i)?;
    let tables = parse_named_tables(args, &mut i)?;
    let pairs = parse_pairs(args, &mut i)?;
    if i < args.len() {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let default_upper = tables.is_empty() && pairs.is_empty();
    Ok(XlateSpec {
        ranges,
        tables,
        pairs,
        default_upper,
    })
}

fn parse_ranges(args: &[String], i: &mut usize) -> Result<Vec<InputRange>, String> {
    if *i >= args.len() {
        return Ok(Vec::new());
    }
    if args[*i].starts_with('(') || args[*i] == "(" {
        return parse_paren_ranges(args, i);
    }
    if is_named_table_start(&args[*i]) {
        return Ok(Vec::new());
    }
    if looks_like_input_range_start(&args[*i]) {
        let range = parse_input_range(args, i)?;
        return Ok(vec![range]);
    }
    Ok(Vec::new())
}

fn parse_paren_ranges(args: &[String], i: &mut usize) -> Result<Vec<InputRange>, String> {
    let first = &args[*i];
    let rest = first
        .strip_prefix('(')
        .ok_or_else(|| format!("expected '(' , got {first}"))?;
    *i += 1;

    let mut inner: Vec<String> = Vec::new();
    let mut closed = false;
    if rest.ends_with(')') {
        let body = &rest[..rest.len() - 1];
        if !body.is_empty() {
            inner.push(body.to_string());
        }
        closed = true;
    } else {
        if !rest.is_empty() {
            inner.push(rest.to_string());
        }
        while *i < args.len() {
            let tok = &args[*i];
            *i += 1;
            if tok == ")" {
                closed = true;
                break;
            }
            if tok.starts_with('(') {
                return Err("nested parentheses are not allowed".into());
            }
            if let Some(body) = tok.strip_suffix(')') {
                if !body.is_empty() {
                    inner.push(body.to_string());
                }
                closed = true;
                break;
            }
            inner.push(tok.clone());
        }
    }
    if !closed {
        return Err("missing closing parenthesis".into());
    }
    if inner.is_empty() {
        return Err("input range list is empty".into());
    }
    let mut j = 0;
    let mut ranges = Vec::new();
    while j < inner.len() {
        if !looks_like_input_range_start(&inner[j]) {
            return Err(format!("unrecognized operand {}", inner[j]));
        }
        ranges.push(parse_input_range(&inner, &mut j)?);
    }
    Ok(ranges)
}

fn parse_named_tables(args: &[String], i: &mut usize) -> Result<Vec<NamedTable>, String> {
    let mut tables = Vec::new();
    while *i < args.len() {
        match take_named_table(args, i)? {
            Some(t) => tables.push(t),
            None => break,
        }
    }
    Ok(tables)
}

fn is_named_table_start(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    is_input_kw(&u)
        || is_output_kw(&u)
        || is_upper_kw(&u)
        || is_lower_kw(&u)
        || u == "TO"
        || u == "FROM"
        || u == "A2E"
        || u == "E2A"
}

fn take_named_table(args: &[String], i: &mut usize) -> Result<Option<NamedTable>, String> {
    if *i >= args.len() {
        return Ok(None);
    }
    let u = args[*i].to_ascii_uppercase();
    if is_input_kw(&u) {
        *i += 1;
        return Ok(Some(NamedTable::Identity));
    }
    if is_output_kw(&u) {
        *i += 1;
        return Ok(Some(NamedTable::Identity));
    }
    if is_upper_kw(&u) {
        *i += 1;
        return Ok(Some(NamedTable::Upper));
    }
    if is_lower_kw(&u) {
        *i += 1;
        return Ok(Some(NamedTable::Lower));
    }
    if u == "TO" {
        *i += 1;
        return Err(reject_codepage("TO", args, i));
    }
    if u == "FROM" {
        *i += 1;
        return Err(reject_codepage("FROM", args, i));
    }
    if u == "A2E" {
        *i += 1;
        return Err(
            "A2E is not implemented (equivalent to FROM 819, Author's Edition 1.1.11 p.607)".into(),
        );
    }
    if u == "E2A" {
        *i += 1;
        return Err(
            "E2A is not implemented (equivalent to TO 819, Author's Edition 1.1.11 p.607)".into(),
        );
    }
    Ok(None)
}

fn reject_codepage(kw: &str, args: &[String], i: &mut usize) -> String {
    if *i < args.len() && args[*i].eq_ignore_ascii_case("CODEPAGE") {
        *i += 1;
    }
    if *i >= args.len() || !is_codepage_number(&args[*i]) {
        return format!("{kw} requires a code page number");
    }
    let number = args[*i].clone();
    *i += 1;
    format!(
        "{kw} {number} is not implemented (EBCDIC/code-page table, Author's Edition 1.1.11 p.604)"
    )
}

/// Decimal code-page number, including Figure 339's `37-2` (p.605).
fn is_codepage_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    let mut parts = s.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), None) => {
            !a.is_empty()
                && !b.is_empty()
                && a.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

fn is_input_kw(upper: &str) -> bool {
    matches_abbrev(upper, "INPUT", 2)
}

fn is_output_kw(upper: &str) -> bool {
    matches_abbrev(upper, "OUTPUT", 3)
}

fn is_upper_kw(upper: &str) -> bool {
    matches_abbrev(upper, "UPPER", 2)
}

fn is_lower_kw(upper: &str) -> bool {
    matches_abbrev(upper, "LOWER", 3)
}

fn parse_pairs(args: &[String], i: &mut usize) -> Result<Vec<(Vec<u8>, Vec<u8>)>, String> {
    let rest = &args[*i..];
    if rest.is_empty() {
        return Ok(Vec::new());
    }
    let mut seqs = Vec::new();
    for tok in rest {
        seqs.push(take_xrange_seq(tok)?);
    }
    if seqs.len() % 2 != 0 {
        return Err("Odd number of translate pairs".into());
    }
    let mut pairs = Vec::new();
    for chunk in seqs.chunks(2) {
        let to = &chunk[1];
        if to.is_empty() {
            return Err("empty \"to\" range".into());
        }
        pairs.push((chunk[0].clone(), to.clone()));
        *i += 2;
    }
    Ok(pairs)
}

fn take_xrange_seq(tok: &str) -> Result<Vec<u8>, String> {
    match expand_xrange(tok)? {
        Some(v) if !v.is_empty() => Ok(v),
        Some(_) => Err(format!("empty xrange {tok}")),
        None => Err(format!("unrecognized operand {tok}")),
    }
}

fn build_table(ctx: &mut StageCtx, spec: &XlateSpec) -> Result<[u8; 256], String> {
    let mut table = identity_table();
    let mut from_secondary = false;
    if ctx.input_connected(StreamId::SECONDARY) {
        ctx.select_input(StreamId::SECONDARY);
        match ctx.read() {
            Ok(Some(rec)) => {
                load_secondary_table(&mut table, rec.as_bytes());
                from_secondary = true;
            }
            Ok(None) => {
                // Connected but empty: identity initial table.
            }
            Err(_) => return Err("cannot read secondary input".into()),
        }
        ctx.short_input();
        ctx.select_input(StreamId::PRIMARY);
    }

    if spec.default_upper && !from_secondary {
        table = upper_table();
    }
    for named in &spec.tables {
        compose_table(&mut table, &named_bytes(*named));
    }
    for (from, to) in &spec.pairs {
        patch_pairs(&mut table, from, to);
    }
    Ok(table)
}

fn identity_table() -> [u8; 256] {
    let mut t = [0u8; 256];
    for (i, slot) in t.iter_mut().enumerate() {
        *slot = i as u8;
    }
    t
}

/// ASCII A–Z case: `a`–`z` (X'61'–X'7A') map to `A`–`Z`. Other bytes unchanged.
fn upper_table() -> [u8; 256] {
    let mut t = identity_table();
    for b in b'a'..=b'z' {
        t[b as usize] = b.to_ascii_uppercase();
    }
    t
}

/// ASCII A–Z case: `A`–`Z` (X'41'–X'5A') map to `a`–`z`. Other bytes unchanged.
///
/// The book builds LOWER as the inverse of the CMS uppercase table, breaking
/// ties by lower hex value (p.604). On EBCDIC, lowercase letters have smaller
/// code points than uppercase, so that inverse maps A–Z to a–z. On ASCII the
/// hex order is reversed; a literal inverse-by-lower-hex would leave A–Z
/// unchanged. We keep the intended case mapping.
fn lower_table() -> [u8; 256] {
    let mut t = identity_table();
    for b in b'A'..=b'Z' {
        t[b as usize] = b.to_ascii_lowercase();
    }
    t
}

fn named_bytes(named: NamedTable) -> [u8; 256] {
    match named {
        NamedTable::Upper => upper_table(),
        NamedTable::Lower => lower_table(),
        NamedTable::Identity => identity_table(),
    }
}

fn compose_table(current: &mut [u8; 256], next: &[u8; 256]) {
    let src = *current;
    for (i, slot) in current.iter_mut().enumerate() {
        *slot = next[src[i] as usize];
    }
}

fn load_secondary_table(table: &mut [u8; 256], rec: &[u8]) {
    let n = rec.len().min(256);
    table[..n].copy_from_slice(&rec[..n]);
}

fn patch_pairs(table: &mut [u8; 256], from: &[u8], to: &[u8]) {
    let last = *to.last().expect("non-empty to range");
    for (i, &f) in from.iter().enumerate() {
        table[f as usize] = to.get(i).copied().unwrap_or(last);
    }
}

fn run_xlate(ctx: &mut StageCtx, spec: &XlateSpec, table: &[u8; 256]) {
    loop {
        if !ctx.output_connected(StreamId::PRIMARY) {
            short_xlate_input(ctx);
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let out = apply_xlate(rec.as_bytes(), spec, table);
            if ctx.write(Record::from(out)).is_err() {
                short_xlate_input(ctx);
                return;
            }
            let _ = ctx.read();
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

fn short_xlate_input(ctx: &mut StageCtx) {
    if ctx.input_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
    }
}

fn apply_xlate(original: &[u8], spec: &XlateSpec, table: &[u8; 256]) -> Vec<u8> {
    let mut buf = original.to_vec();
    if spec.ranges.is_empty() {
        for b in &mut buf {
            *b = table[*b as usize];
        }
        return buf;
    }
    for range in &spec.ranges {
        for pos in input_range_positions(original, range) {
            if pos < buf.len() {
                buf[pos] = table[buf[pos] as usize];
            }
        }
    }
    buf
}
