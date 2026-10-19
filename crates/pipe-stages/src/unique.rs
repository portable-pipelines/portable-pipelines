//! UNIQUE — Author's Edition 1.1.11, pp. 572–574 (tutorial Figure 116 p.56).
//!
//! Syntax (fixed order; COUNT / selection may also trail a range):
//! `UNIQue [COUNT] [NOPAD | PAD xorc] [ANYcase] [uniqueRanges]
//!         [LAST | SINGLEs | FIRST | MULTiple | PAIRwise]`
//!
//! Compatibility (p.572): two optional keywords then an optional range.
//! `PAIRWISE` cannot be specified with `COUNT`.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{is_anycase, matches_abbrev};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, parse_xorc, InputRange,
};

pub fn unique(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_unique(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("UNIQUE: {e}"));
                return;
            }
        };

        if ctx.input_connected(StreamId::SECONDARY) {
            ctx.fail("UNIQUE: secondary input stream must not be connected");
            return;
        }

        if !outputs_live(ctx) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        match spec.selection {
            Selection::Pairwise => run_pairwise(ctx, &spec),
            _ => run_runs(ctx, &spec),
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selection {
    Last,
    First,
    Singles,
    Multiple,
    Pairwise,
}

#[derive(Clone, Debug)]
struct KeyField {
    range: Option<InputRange>,
    pad: Option<u8>,
}

struct UniqueSpec {
    count: bool,
    anycase: bool,
    selection: Selection,
    fields: Vec<KeyField>,
}

fn parse_unique(args: &[String]) -> Result<UniqueSpec, String> {
    let mut i = 0;

    // [COUNT] — whole word (diagram).
    let mut count = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("COUNT") {
        count = true;
        i += 1;
    }

    // [NOPAD | PAD xorc] — default NOPAD; whole words.
    let mut default_pad = None;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "PAD" {
            i += 1;
            if i >= args.len() {
                return Err("PAD requires an xorc".into());
            }
            default_pad = Some(parse_xorc(&args[i])?);
            i += 1;
        } else if upper == "NOPAD" {
            i += 1;
        }
    }

    // [ANYcase] — min 3 (shared helper; no ANYOF on this stage). Synonyms Note 4.
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    // Optional uniqueRanges, then COUNT / selection on either side of the range
    // (diagram: range then keyword; compatibility: keywords then range, p.572).
    let mut selection = None;
    let mut fields = None;

    while i < args.len() {
        if selection.is_none() {
            if let Some(sel) = parse_selection(&args[i]) {
                selection = Some(sel);
                i += 1;
                continue;
            }
        }
        if !count && args[i].eq_ignore_ascii_case("COUNT") {
            count = true;
            i += 1;
            continue;
        }
        if fields.is_none() && is_ranges_start(&args[i]) {
            fields = Some(parse_unique_ranges(args, &mut i, default_pad)?);
            continue;
        }
        return Err(unexpected_operand(&args[i]));
    }

    let selection = selection.unwrap_or(Selection::Last);
    if count && selection == Selection::Pairwise {
        return Err("PAIRWISE cannot be specified with COUNT".into());
    }

    Ok(UniqueSpec {
        count,
        anycase,
        selection,
        fields: fields.unwrap_or_else(|| {
            vec![KeyField {
                range: None,
                pad: default_pad,
            }]
        }),
    })
}

fn parse_selection(s: &str) -> Option<Selection> {
    let u = s.to_ascii_uppercase();
    if u == "LAST" {
        Some(Selection::Last)
    } else if u == "FIRST" {
        Some(Selection::First)
    } else if matches_abbrev(&u, "SINGLES", 6) {
        Some(Selection::Singles)
    } else if matches_abbrev(&u, "MULTIPLE", 4) {
        Some(Selection::Multiple)
    } else if matches_abbrev(&u, "PAIRWISE", 4) {
        Some(Selection::Pairwise)
    } else {
        None
    }
}

fn is_ranges_start(s: &str) -> bool {
    s.starts_with('(') || looks_like_input_range_start(s)
}

fn parse_unique_ranges(
    args: &[String],
    i: &mut usize,
    default_pad: Option<u8>,
) -> Result<Vec<KeyField>, String> {
    if *i >= args.len() {
        return Err("incomplete uniqueRanges".into());
    }
    if args[*i].starts_with('(') {
        parse_paren_fields(args, i, default_pad)
    } else {
        let range = parse_input_range(args, i)?;
        Ok(vec![KeyField {
            range: Some(range),
            pad: default_pad,
        }])
    }
}

fn parse_paren_fields(
    args: &[String],
    i: &mut usize,
    default_pad: Option<u8>,
) -> Result<Vec<KeyField>, String> {
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
        return Err("uniqueRanges list is empty".into());
    }
    parse_field_list(&inner, default_pad)
}

fn parse_field_list(args: &[String], default_pad: Option<u8>) -> Result<Vec<KeyField>, String> {
    let mut i = 0;
    let mut fields = Vec::new();
    while i < args.len() {
        if !looks_like_input_range_start(&args[i]) {
            return Err(unexpected_operand(&args[i]));
        }
        let range = parse_input_range(args, &mut i)?;
        let mut pad = default_pad;
        if i < args.len() {
            let upper = args[i].to_ascii_uppercase();
            if upper == "PAD" {
                i += 1;
                if i >= args.len() {
                    return Err("PAD requires an xorc".into());
                }
                pad = Some(parse_xorc(&args[i])?);
                i += 1;
            } else if upper == "NOPAD" {
                pad = None;
                i += 1;
            }
        }
        fields.push(KeyField {
            range: Some(range),
            pad,
        });
    }
    Ok(fields)
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if arg.eq_ignore_ascii_case("COUNT")
        || upper == "PAD"
        || upper == "NOPAD"
        || is_anycase(arg)
        || parse_selection(arg).is_some()
    {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

fn records_equal(a: &[u8], b: &[u8], spec: &UniqueSpec) -> bool {
    spec.fields
        .iter()
        .all(|field| field_equal(a, b, field, spec.anycase))
}

fn field_equal(a: &[u8], b: &[u8], field: &KeyField, anycase: bool) -> bool {
    let mut ka = extract_field(a, field);
    let mut kb = extract_field(b, field);
    if let Some(p) = field.pad {
        pad_shorter(&mut ka, &mut kb, p);
    } else if ka.len() != kb.len() {
        return false;
    }
    if anycase {
        ka.eq_ignore_ascii_case(&kb)
    } else {
        ka == kb
    }
}

fn extract_field(record: &[u8], field: &KeyField) -> Vec<u8> {
    match &field.range {
        None => record.to_vec(),
        Some(range) => extract_input_range(record, range),
    }
}

fn pad_shorter(a: &mut Vec<u8>, b: &mut Vec<u8>, pad: u8) {
    if a.len() < b.len() {
        a.resize(b.len(), pad);
    } else if b.len() < a.len() {
        b.resize(a.len(), pad);
    }
}

fn count_prefix(n: u64) -> Vec<u8> {
    format!("{n:>10}").into_bytes()
}

fn with_count(rec: Record, count: bool, n: u64) -> Record {
    if !count {
        return rec;
    }
    let mut out = count_prefix(n);
    out.extend_from_slice(rec.as_bytes());
    Record::from(out)
}

fn emit(ctx: &mut StageCtx, stream: StreamId, rec: Record, count: bool, n: u64) -> bool {
    if !outputs_live(ctx) {
        return false;
    }
    if !ctx.output_connected(stream) {
        return true;
    }
    ctx.select_output(stream);
    match ctx.write(with_count(rec, count, n)) {
        Ok(()) => outputs_live(ctx),
        Err(_) => outputs_live(ctx),
    }
}

fn run_runs(ctx: &mut StageCtx, spec: &UniqueSpec) {
    match spec.selection {
        Selection::First => run_first(ctx, spec),
        Selection::Last => run_last(ctx, spec),
        Selection::Singles => run_singles(ctx, spec),
        Selection::Multiple => run_multiple(ctx, spec),
        Selection::Pairwise => unreachable!("PAIRWISE uses run_pairwise"),
    }
}

fn read_rec(ctx: &mut StageCtx) -> Option<Record> {
    match ctx.read() {
        Ok(Some(r)) => Some(r),
        Ok(None) | Err(_) => None,
    }
}

/// FIRST does not delay the record (p.573): first of a run is written immediately.
fn run_first(ctx: &mut StageCtx, spec: &UniqueSpec) {
    let mut prev: Option<Record> = None;
    let mut pos = 0u64;
    while let Some(rec) = read_rec(ctx) {
        let same = prev
            .as_ref()
            .is_some_and(|p| records_equal(p.as_bytes(), rec.as_bytes(), spec));
        if same {
            pos += 1;
            if !emit(ctx, StreamId::SECONDARY, rec, spec.count, pos) {
                return;
            }
        } else {
            pos = 1;
            if !emit(ctx, StreamId::PRIMARY, rec.clone(), spec.count, 1) {
                return;
            }
            prev = Some(rec);
        }
    }
}

/// LAST (default) delays one record: leading duplicates → secondary, last → primary.
fn run_last(ctx: &mut StageCtx, spec: &UniqueSpec) {
    let mut hold: Option<Record> = None;
    let mut pos = 0u64;
    while let Some(rec) = read_rec(ctx) {
        match hold.take() {
            None => {
                hold = Some(rec);
                pos = 1;
            }
            Some(prev) => {
                if records_equal(prev.as_bytes(), rec.as_bytes(), spec) {
                    if !emit(ctx, StreamId::SECONDARY, prev, spec.count, pos) {
                        return;
                    }
                    pos += 1;
                    hold = Some(rec);
                } else {
                    if !emit(ctx, StreamId::PRIMARY, prev, spec.count, pos) {
                        return;
                    }
                    hold = Some(rec);
                    pos = 1;
                }
            }
        }
    }
    if let Some(prev) = hold {
        let _ = emit(ctx, StreamId::PRIMARY, prev, spec.count, pos);
    }
}

/// SINGLES: only singleton runs → primary; complete duplicate sets → secondary.
fn run_singles(ctx: &mut StageCtx, spec: &UniqueSpec) {
    run_by_run_size(ctx, spec, true);
}

/// MULTIPLE: only runs with more than one record → primary; singles → secondary.
fn run_multiple(ctx: &mut StageCtx, spec: &UniqueSpec) {
    run_by_run_size(ctx, spec, false);
}

fn run_by_run_size(ctx: &mut StageCtx, spec: &UniqueSpec, singles: bool) {
    let primary_when_multi = !singles;
    let mut hold: Option<Record> = None;
    let mut pos = 0u64;
    let mut multi = false;
    while let Some(rec) = read_rec(ctx) {
        match hold.take() {
            None => {
                hold = Some(rec);
                pos = 1;
                multi = false;
            }
            Some(prev) => {
                if records_equal(prev.as_bytes(), rec.as_bytes(), spec) {
                    if !multi {
                        let stream = if primary_when_multi {
                            StreamId::PRIMARY
                        } else {
                            StreamId::SECONDARY
                        };
                        if !emit(ctx, stream, prev, spec.count, 1) {
                            return;
                        }
                        multi = true;
                    }
                    pos += 1;
                    let stream = if primary_when_multi {
                        StreamId::PRIMARY
                    } else {
                        StreamId::SECONDARY
                    };
                    if !emit(ctx, stream, rec.clone(), spec.count, pos) {
                        return;
                    }
                    hold = Some(rec);
                } else {
                    if !multi {
                        let stream = if primary_when_multi {
                            StreamId::SECONDARY
                        } else {
                            StreamId::PRIMARY
                        };
                        if !emit(ctx, stream, prev, spec.count, 1) {
                            return;
                        }
                    }
                    hold = Some(rec);
                    pos = 1;
                    multi = false;
                }
            }
        }
    }
    if let Some(prev) = hold {
        if !multi {
            let stream = if primary_when_multi {
                StreamId::SECONDARY
            } else {
                StreamId::PRIMARY
            };
            let _ = emit(ctx, stream, prev, spec.count, 1);
        }
    }
}

/// PAIRWISE: non-overlapping pairs; both records go to primary when keys differ.
/// An odd leftover has no partner and is written to the primary (see architecture.md).
fn run_pairwise(ctx: &mut StageCtx, spec: &UniqueSpec) {
    loop {
        let Some(a) = read_rec(ctx) else {
            return;
        };
        let Some(b) = read_rec(ctx) else {
            let _ = emit(ctx, StreamId::PRIMARY, a, false, 0);
            return;
        };
        let stream = if records_equal(a.as_bytes(), b.as_bytes(), spec) {
            StreamId::SECONDARY
        } else {
            StreamId::PRIMARY
        };
        if !emit(ctx, stream, a, false, 0) {
            return;
        }
        if !emit(ctx, stream, b, false, 0) {
            return;
        }
    }
}
