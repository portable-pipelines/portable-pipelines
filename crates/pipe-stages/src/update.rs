//! UPDATE — Author's Edition 1.1.11 pp.576–577 (tutorial Figure 176 p.86).
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `UPDATE [range] [FIRST] [LAST] [* string]`
//!
//! Gateway. The master file is on the primary input; the CMS UPDATE stream
//! (SEQ8 control cards) is on the secondary input. The updated master is
//! written to the primary output; the update log to the secondary output.
//! Both outputs must be connected (pipe the log to HOLE to discard it).

use pipe_core::{Record, StageCtx, StreamId};

/// Default SEQ8 sequence field (1.1.11 p.576).
const DEFAULT_SEQ_START: usize = 73;
const DEFAULT_SEQ_END: usize = 80;
/// Book: maximum length of the sequence field is 15.
const MAX_SEQ_WIDTH: usize = 15;
/// CMS UPDATE: control-statement data must not extend beyond column 50.
const CONTROL_COLS: usize = 50;
/// SEQ8 default start for `./ S` when the start is omitted.
const SEQ8_RESEQ_DEFAULT: u64 = 1000;
/// Default `$` increment is capped at 100 (CMS UPDATE SEQ8).
const DEFAULT_INCR_CAP: u64 = 100;

const RC_INPUT_SEQ: i32 = 4;
const RC_RESEQ_NOT_FIRST: i32 = 4;
const RC_RESEQ_NUMERIC: i32 = 8;
const RC_OUTPUT_SEQ: i32 = 8;
const RC_SEQ_FIELD: i32 = 12;
const RC_SEQ_NOT_FOUND: i32 = 12;
const RC_UNSUPPORTED: i32 = 32;

pub fn update(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_update(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("UPDATE: {e}"));
                return;
            }
        };
        run_update(ctx, spec);
    })
}

#[derive(Clone, Debug)]
struct UpdateSpec {
    seq_start: usize,
    seq_end: usize,
    first: bool,
    last: bool,
    comment: String,
}

impl UpdateSpec {
    fn width(&self) -> usize {
        self.seq_end - self.seq_start + 1
    }
}

/// Fixed operand order: `[range] [FIRST] [LAST] [* string]`.
fn parse_update(args: &[String]) -> Result<UpdateSpec, String> {
    let mut i = 0;
    let mut seq_start = DEFAULT_SEQ_START;
    let mut seq_end = DEFAULT_SEQ_END;

    if i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u != "FIRST" && u != "LAST" && args[i] != "*" {
            let (s, e) = parse_seq_range(&args[i])?;
            if e - s + 1 > MAX_SEQ_WIDTH {
                return Err("the maximum length of the sequence field is 15".into());
            }
            seq_start = s;
            seq_end = e;
            i += 1;
        }
    }

    let mut first = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("FIRST") {
        first = true;
        i += 1;
    }
    let mut last = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("LAST") {
        last = true;
        i += 1;
    }

    let mut comment = String::new();
    if i < args.len() {
        if args[i] == "*" {
            i += 1;
            comment = args[i..].join(" ");
        } else if args[i].eq_ignore_ascii_case("FIRST") {
            return Err("FIRST must precede LAST".into());
        } else {
            return Err(format!("unrecognized operand {}", args[i]));
        }
    }
    Ok(UpdateSpec {
        seq_start,
        seq_end,
        first,
        last,
        comment,
    })
}

/// Column range only: `n`, `n-m`, or `n.m` (1-based, positive).
fn parse_seq_range(tok: &str) -> Result<(usize, usize), String> {
    if let Some((left, right)) = tok.split_once('.') {
        let start = parse_pos_col(left, tok)?;
        let len = parse_pos_col(right, tok)?;
        return Ok((start, start + len - 1));
    }
    if let Some(idx) = tok
        .char_indices()
        .find_map(|(i, c)| if c == '-' && i > 0 { Some(i) } else { None })
    {
        let start = parse_pos_col(&tok[..idx], tok)?;
        let end = parse_pos_col(&tok[idx + 1..], tok)?;
        if end < start {
            return Err(format!(
                "ending column is before the beginning ({start}…{end})"
            ));
        }
        return Ok((start, end));
    }
    if !tok.is_empty() && tok.bytes().all(|b| b.is_ascii_digit()) {
        let n = parse_pos_col(tok, tok)?;
        return Ok((n, n));
    }
    Err(format!("unrecognized operand {tok}"))
}

fn parse_pos_col(s: &str, tok: &str) -> Result<usize, String> {
    let n: usize = s
        .parse()
        .map_err(|_| format!("unrecognized operand {tok}"))?;
    if n == 0 {
        return Err(format!("unrecognized operand {tok}"));
    }
    Ok(n)
}

#[derive(Clone, Debug)]
enum Control {
    Comment,
    Sequence {
        start: u64,
        incr: u64,
    },
    Insert {
        after: u64,
        number: Option<(u64, u64)>,
    },
    Delete {
        from: u64,
        through: u64,
    },
    Replace {
        from: u64,
        through: u64,
        number: Option<(u64, u64)>,
    },
}

#[derive(Clone, Copy, Debug)]
enum SeqField {
    Blank,
    Number(u64),
    Bad,
}

struct Runtime {
    spec: UpdateSpec,
    held_master: Option<Record>,
    held_update: Option<Record>,
    last_in: Option<u64>,
    last_out: Option<u64>,
    reseq: Option<(u64, u64)>,
    seen_control: bool,
    highest_rc: i32,
}

fn run_update(ctx: &mut StageCtx, spec: UpdateSpec) {
    if ctx.connected_inputs().iter().any(|s| s.0 > 1)
        || ctx.connected_outputs().iter().any(|s| s.0 > 1)
    {
        ctx.fail("UPDATE: only the primary and secondary streams may be connected");
        return;
    }

    // p.576: terminate when either output is not connected. Connecting the
    // secondary output to HOLE discards the log. Both outputs being required
    // also covers "two streams must be defined".
    if !ctx.output_connected(StreamId::PRIMARY) || !ctx.output_connected(StreamId::SECONDARY) {
        short_inputs(ctx);
        return;
    }

    let mut rt = Runtime {
        spec,
        held_master: None,
        held_update: None,
        last_in: None,
        last_out: None,
        reseq: None,
        seen_control: false,
        highest_rc: 0,
    };

    if !apply_updates(ctx, &mut rt) {
        short_inputs(ctx);
    }
    if rt.highest_rc != 0 {
        ctx.set_return_code(rt.highest_rc);
    }
}

fn apply_updates(ctx: &mut StageCtx, rt: &mut Runtime) -> bool {
    loop {
        if !outputs_live(ctx) {
            return false;
        }
        let Some(rec) = next_update(ctx, rt) else {
            break;
        };
        if !is_control(rec.as_bytes()) {
            if !note(ctx, rt, RC_UNSUPPORTED, "missing control card") {
                return false;
            }
            continue;
        }
        if !log_record(ctx, rec.clone()) {
            return false;
        }
        match parse_control(rec.as_bytes(), rt.spec.width()) {
            Ok(Control::Comment) => {
                rt.seen_control = true;
            }
            Ok(Control::Sequence { start, incr }) => {
                if rt.seen_control {
                    if !note(
                        ctx,
                        rt,
                        RC_RESEQ_NOT_FIRST,
                        "resequence request that is not first in the update file",
                    ) {
                        return false;
                    }
                } else {
                    rt.reseq = Some((start, incr));
                }
                rt.seen_control = true;
            }
            Ok(Control::Insert { after, number }) => {
                rt.seen_control = true;
                let data = collect_data(ctx, rt);
                if !apply_insert(ctx, rt, after, number, data) {
                    return false;
                }
            }
            Ok(Control::Delete { from, through }) => {
                rt.seen_control = true;
                if !apply_delete(ctx, rt, from, through) {
                    return false;
                }
            }
            Ok(Control::Replace {
                from,
                through,
                number,
            }) => {
                rt.seen_control = true;
                let data = collect_data(ctx, rt);
                if !apply_delete(ctx, rt, from, through) {
                    return false;
                }
                if !write_inserts(ctx, rt, number, data) {
                    return false;
                }
            }
            Err((rc, msg)) => {
                rt.seen_control = true;
                if !note(ctx, rt, rc, &msg) {
                    return false;
                }
                // A broken I/R card still consumes following data records so
                // they are not treated as further missing control cards.
                if looks_like_insert_or_replace(rec.as_bytes()) {
                    let _ = collect_data(ctx, rt);
                }
            }
        }
    }
    while let Some(rec) = next_master(ctx, rt) {
        if let SeqField::Number(n) = extract_seq(rec.as_bytes(), rt.spec.seq_start, rt.spec.seq_end)
        {
            if !check_input_seq(ctx, rt, n) {
                return false;
            }
        }
        if !write_master(ctx, rt, rec) {
            return false;
        }
    }
    true
}

fn looks_like_insert_or_replace(bytes: &[u8]) -> bool {
    matches!(
        control_fields(bytes).first().map(|s| s.as_str()),
        Some("I" | "R")
    )
}

fn collect_data(ctx: &mut StageCtx, rt: &mut Runtime) -> Vec<Record> {
    let mut data = Vec::new();
    loop {
        match next_update(ctx, rt) {
            None => break,
            Some(rec) if is_control(rec.as_bytes()) => {
                rt.held_update = Some(rec);
                break;
            }
            Some(rec) => data.push(rec),
        }
    }
    data
}

fn apply_insert(
    ctx: &mut StageCtx,
    rt: &mut Runtime,
    after: u64,
    number: Option<(u64, u64)>,
    data: Vec<Record>,
) -> bool {
    if !copy_before(ctx, rt, after) {
        return false;
    }
    // CMS UPDATE SEQ8: `./ I 0` inserts at the beginning of the master
    // (a dummy sequence 0 that need not appear in the file).
    let Some(rec) = next_master(ctx, rt) else {
        if after == 0 {
            return write_inserts(ctx, rt, number, data);
        }
        return note(
            ctx,
            rt,
            RC_SEQ_NOT_FOUND,
            &format!("sequence number {after} not found"),
        );
    };
    match extract_seq(rec.as_bytes(), rt.spec.seq_start, rt.spec.seq_end) {
        SeqField::Number(n) if n == after => {
            if !check_input_seq(ctx, rt, n) {
                return false;
            }
            if !write_master(ctx, rt, rec) {
                return false;
            }
            write_inserts(ctx, rt, number, data)
        }
        _ => {
            rt.held_master = Some(rec);
            if after == 0 {
                write_inserts(ctx, rt, number, data)
            } else {
                note(
                    ctx,
                    rt,
                    RC_SEQ_NOT_FOUND,
                    &format!("sequence number {after} not found"),
                )
            }
        }
    }
}

fn apply_delete(ctx: &mut StageCtx, rt: &mut Runtime, from: u64, through: u64) -> bool {
    if from > through {
        return note(
            ctx,
            rt,
            RC_SEQ_FIELD,
            "sequence number range is not ascending",
        );
    }
    if !copy_before(ctx, rt, from) {
        return false;
    }
    drop_range(ctx, rt, from, through)
}

/// Copy master records with sequence < `target`. Leaves a record whose
/// sequence is ≥ target in `held_master`.
fn copy_before(ctx: &mut StageCtx, rt: &mut Runtime, target: u64) -> bool {
    loop {
        let Some(rec) = next_master(ctx, rt) else {
            return true;
        };
        match extract_seq(rec.as_bytes(), rt.spec.seq_start, rt.spec.seq_end) {
            SeqField::Blank => {
                if !write_master(ctx, rt, rec) {
                    return false;
                }
            }
            SeqField::Bad => {
                if !note(ctx, rt, RC_SEQ_FIELD, "sequence number is not numeric") {
                    return false;
                }
                if !write_master(ctx, rt, rec) {
                    return false;
                }
            }
            SeqField::Number(n) => {
                if n < target {
                    if !check_input_seq(ctx, rt, n) {
                        return false;
                    }
                    if !write_master(ctx, rt, rec) {
                        return false;
                    }
                } else {
                    // Leave the record unread; the caller checks its sequence
                    // when it consumes it so FIRST is not applied twice.
                    rt.held_master = Some(rec);
                    return true;
                }
            }
        }
    }
}

fn drop_range(ctx: &mut StageCtx, rt: &mut Runtime, from: u64, through: u64) -> bool {
    let mut saw_from = false;
    loop {
        let Some(rec) = next_master(ctx, rt) else {
            if !saw_from {
                return note(
                    ctx,
                    rt,
                    RC_SEQ_NOT_FOUND,
                    &format!("sequence number {from} not found"),
                );
            }
            if from != through {
                return note(
                    ctx,
                    rt,
                    RC_SEQ_NOT_FOUND,
                    &format!("sequence number {through} not found"),
                );
            }
            return true;
        };
        match extract_seq(rec.as_bytes(), rt.spec.seq_start, rt.spec.seq_end) {
            SeqField::Blank => {
                if saw_from {
                    if !log_record(ctx, rec) {
                        return false;
                    }
                } else if !write_master(ctx, rt, rec) {
                    return false;
                }
            }
            SeqField::Bad => {
                if !note(ctx, rt, RC_SEQ_FIELD, "sequence number is not numeric") {
                    return false;
                }
                if saw_from {
                    if !log_record(ctx, rec) {
                        return false;
                    }
                } else if !write_master(ctx, rt, rec) {
                    return false;
                }
            }
            SeqField::Number(n) => {
                if !check_input_seq(ctx, rt, n) {
                    return false;
                }
                if !saw_from {
                    if n == from {
                        saw_from = true;
                        if !log_record(ctx, rec) {
                            return false;
                        }
                        if from == through {
                            return true;
                        }
                    } else {
                        rt.held_master = Some(rec);
                        return note(
                            ctx,
                            rt,
                            RC_SEQ_NOT_FOUND,
                            &format!("sequence number {from} not found"),
                        );
                    }
                } else if n <= through {
                    if !log_record(ctx, rec) {
                        return false;
                    }
                    if n == through {
                        return true;
                    }
                } else {
                    rt.held_master = Some(rec);
                    return note(
                        ctx,
                        rt,
                        RC_SEQ_NOT_FOUND,
                        &format!("sequence number {through} not found"),
                    );
                }
            }
        }
    }
}

fn write_inserts(
    ctx: &mut StageCtx,
    rt: &mut Runtime,
    number: Option<(u64, u64)>,
    data: Vec<Record>,
) -> bool {
    let mut next_seq = number.map(|(s, _)| s);
    let incr = number.map(|(_, i)| i).unwrap_or(0);
    for rec in data {
        let rec = if let Some(n) = next_seq {
            let numbered = match overlay_seq(rec.as_bytes(), rt.spec.seq_start, rt.spec.seq_end, n)
            {
                Ok(bytes) => Record::from(bytes),
                Err(msg) => {
                    if !note(ctx, rt, RC_SEQ_FIELD, &msg) {
                        return false;
                    }
                    rec
                }
            };
            next_seq = Some(n.saturating_add(incr));
            numbered
        } else {
            rec
        };
        if !log_record(ctx, rec.clone()) {
            return false;
        }
        if !write_master(ctx, rt, rec) {
            return false;
        }
    }
    true
}

fn write_master(ctx: &mut StageCtx, rt: &mut Runtime, rec: Record) -> bool {
    let rec = if let Some((ref mut next, incr)) = rt.reseq {
        let n = *next;
        *next = n.saturating_add(incr);
        match overlay_seq(rec.as_bytes(), rt.spec.seq_start, rt.spec.seq_end, n) {
            Ok(bytes) => Record::from(bytes),
            Err(msg) => {
                if !note(ctx, rt, RC_SEQ_FIELD, &msg) {
                    return false;
                }
                rec
            }
        }
    } else {
        rec
    };
    if !rt.spec.last {
        if let SeqField::Number(n) = extract_seq(rec.as_bytes(), rt.spec.seq_start, rt.spec.seq_end)
        {
            if let Some(prev) = rt.last_out {
                if n <= prev
                    && !note(
                        ctx,
                        rt,
                        RC_OUTPUT_SEQ,
                        &format!("sequence error in the output master file: {prev} to {n}"),
                    )
                {
                    return false;
                }
            }
            rt.last_out = Some(n);
        }
    }
    if !outputs_live(ctx) {
        return false;
    }
    ctx.select_output(StreamId::PRIMARY);
    ctx.write(rec).is_ok()
}

fn check_input_seq(ctx: &mut StageCtx, rt: &mut Runtime, n: u64) -> bool {
    if rt.spec.first {
        if let Some(prev) = rt.last_in {
            if n <= prev
                && !note(
                    ctx,
                    rt,
                    RC_INPUT_SEQ,
                    &format!("sequence error in the input master file: {prev} to {n}"),
                )
            {
                return false;
            }
        }
    }
    rt.last_in = Some(n);
    true
}

fn note(ctx: &mut StageCtx, rt: &mut Runtime, rc: i32, msg: &str) -> bool {
    if rc > rt.highest_rc {
        rt.highest_rc = rc;
    }
    let line = if rt.spec.comment.is_empty() {
        msg.to_string()
    } else {
        format!("{}: {msg}", rt.spec.comment)
    };
    log_record(ctx, Record::from(line))
}

fn log_record(ctx: &mut StageCtx, rec: Record) -> bool {
    if !ctx.output_connected(StreamId::SECONDARY) {
        return false;
    }
    ctx.select_output(StreamId::SECONDARY);
    ctx.write(rec).is_ok()
}

fn next_master(ctx: &mut StageCtx, rt: &mut Runtime) -> Option<Record> {
    if let Some(rec) = rt.held_master.take() {
        return Some(rec);
    }
    ctx.select_input(StreamId::PRIMARY);
    ctx.read().ok().flatten()
}

fn next_update(ctx: &mut StageCtx, rt: &mut Runtime) -> Option<Record> {
    if let Some(rec) = rt.held_update.take() {
        return Some(rec);
    }
    if !ctx.input_connected(StreamId::SECONDARY) {
        return None;
    }
    ctx.select_input(StreamId::SECONDARY);
    ctx.read().ok().flatten()
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) && ctx.output_connected(StreamId::SECONDARY)
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}

fn is_control(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == b'.' && bytes[1] == b'/'
}

fn control_fields(bytes: &[u8]) -> Vec<String> {
    let limit = bytes.len().min(CONTROL_COLS);
    let text = String::from_utf8_lossy(&bytes[..limit]);
    if !text.starts_with("./") {
        return Vec::new();
    }
    text[2..]
        .split_whitespace()
        .map(|s| s.to_ascii_uppercase())
        .collect()
}

fn parse_control(bytes: &[u8], width: usize) -> Result<Control, (i32, String)> {
    let limit = bytes.len().min(CONTROL_COLS);
    let text = String::from_utf8_lossy(&bytes[..limit]);
    if !text.starts_with("./") {
        return Err((RC_UNSUPPORTED, "unsupported or missing control card".into()));
    }
    let rest = &text[2..];
    if rest.is_empty() || !rest.starts_with(|c: char| c.is_ascii_whitespace()) {
        return Err((RC_UNSUPPORTED, "unsupported or missing control card".into()));
    }
    let raw: Vec<&str> = rest.split_whitespace().collect();
    if raw.is_empty() {
        return Err((RC_UNSUPPORTED, "unsupported or missing control card".into()));
    }
    let op = raw[0].to_ascii_uppercase();
    match op.as_str() {
        "*" => Ok(Control::Comment),
        "S" => parse_sequence(&raw[1..]),
        "I" => parse_insert(&raw[1..], width),
        "D" => parse_delete(&raw[1..], width),
        "R" => parse_replace(&raw[1..], width),
        _ => Err((RC_UNSUPPORTED, "unsupported or missing control card".into())),
    }
}

fn parse_sequence(fields: &[&str]) -> Result<Control, (i32, String)> {
    let start = if fields.is_empty() {
        SEQ8_RESEQ_DEFAULT
    } else {
        parse_reseq_number(fields[0], "start")?
    };
    let incr = if fields.len() < 2 {
        start
    } else {
        parse_reseq_number(fields[1], "increment")?
    };
    Ok(Control::Sequence { start, incr })
}

fn parse_reseq_number(s: &str, what: &str) -> Result<u64, (i32, String)> {
    parse_digits(s).ok_or_else(|| {
        (
            RC_RESEQ_NUMERIC,
            format!("resequence {what} is not numeric"),
        )
    })
}

fn parse_insert(fields: &[&str], width: usize) -> Result<Control, (i32, String)> {
    if fields.is_empty() {
        return Err((RC_SEQ_FIELD, "sequence number is missing".into()));
    }
    let after = parse_seqno(fields[0], width)?;
    let number = parse_dollar_numbering(&fields[1..], after, width)?;
    Ok(Control::Insert { after, number })
}

fn parse_delete(fields: &[&str], width: usize) -> Result<Control, (i32, String)> {
    if fields.is_empty() {
        return Err((RC_SEQ_FIELD, "sequence number is missing".into()));
    }
    let from = parse_seqno(fields[0], width)?;
    let mut through = from;
    let mut i = 1;
    if i < fields.len() && fields[i] != "$" {
        through = parse_seqno(fields[i], width)?;
        i += 1;
    }
    if i < fields.len() {
        if fields[i] == "$" {
            i += 1;
        } else {
            return Err((RC_SEQ_FIELD, "dollar sign is missing".into()));
        }
    }
    if i < fields.len() {
        return Err((RC_UNSUPPORTED, "unsupported or missing control card".into()));
    }
    Ok(Control::Delete { from, through })
}

fn parse_replace(fields: &[&str], width: usize) -> Result<Control, (i32, String)> {
    if fields.is_empty() {
        return Err((RC_SEQ_FIELD, "sequence number is missing".into()));
    }
    let from = parse_seqno(fields[0], width)?;
    let mut i = 1;
    let mut through = from;
    if i < fields.len() && fields[i] != "$" {
        through = parse_seqno(fields[i], width)?;
        i += 1;
    }
    let number = parse_dollar_numbering(&fields[i..], from, width)?;
    Ok(Control::Replace {
        from,
        through,
        number,
    })
}

fn parse_dollar_numbering(
    fields: &[&str],
    seqno: u64,
    width: usize,
) -> Result<Option<(u64, u64)>, (i32, String)> {
    if fields.is_empty() {
        return Ok(None);
    }
    if fields[0] != "$" {
        return Err((RC_SEQ_FIELD, "dollar sign is missing".into()));
    }
    let default_incr = default_insert_incr(seqno);
    let mut start = seqno.saturating_add(default_incr);
    let mut incr = default_incr;
    if fields.len() > 1 {
        start = parse_seqno(fields[1], width)?;
    }
    if fields.len() > 2 {
        incr = parse_seqno(fields[2], width)?;
    }
    if fields.len() > 3 {
        return Err((RC_UNSUPPORTED, "unsupported or missing control card".into()));
    }
    Ok(Some((start, incr)))
}

fn parse_seqno(s: &str, width: usize) -> Result<u64, (i32, String)> {
    let digits = s.trim();
    if digits.is_empty() {
        return Err((RC_SEQ_FIELD, "sequence number is missing".into()));
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err((RC_SEQ_FIELD, "sequence number is not numeric".into()));
    }
    if digits.len() > width {
        return Err((
            RC_SEQ_FIELD,
            "sequence number is longer than the sequence field width".into(),
        ));
    }
    digits
        .parse::<u64>()
        .map_err(|_| (RC_SEQ_FIELD, "sequence number is not numeric".into()))
}

fn parse_digits(s: &str) -> Option<u64> {
    let t = s.trim();
    if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse().ok()
}

/// One tenth of the place value of the least significant nonzero digit,
/// capped at 100 (CMS UPDATE `$` default).
fn default_insert_incr(seqno: u64) -> u64 {
    if seqno == 0 {
        return DEFAULT_INCR_CAP;
    }
    let mut place = 1u64;
    let mut n = seqno;
    while n % 10 == 0 {
        n /= 10;
        place = place.saturating_mul(10);
    }
    (place / 10).min(DEFAULT_INCR_CAP)
}

fn extract_seq(rec: &[u8], start: usize, end: usize) -> SeqField {
    let i0 = start.saturating_sub(1);
    if rec.len() <= i0 {
        return SeqField::Blank;
    }
    let i1 = end.min(rec.len());
    let field = &rec[i0..i1];
    let text = String::from_utf8_lossy(field);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return SeqField::Blank;
    }
    if !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return SeqField::Bad;
    }
    match trimmed.parse::<u64>() {
        Ok(n) => SeqField::Number(n),
        Err(_) => SeqField::Bad,
    }
}

fn overlay_seq(rec: &[u8], start: usize, end: usize, n: u64) -> Result<Vec<u8>, String> {
    let width = end - start + 1;
    let formatted = format!("{n:0width$}");
    if formatted.len() > width {
        return Err("sequence number is longer than the sequence field width".into());
    }
    let mut out = rec.to_vec();
    if out.len() < end {
        out.resize(end, b' ');
    }
    let i0 = start - 1;
    out[i0..end].copy_from_slice(formatted.as_bytes());
    Ok(out)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn default_incr_examples() {
        // `./ I 2600 $` → incr 10 (place 100 / 10).
        assert_eq!(default_insert_incr(2600), 10);
        // `./ I 240000 $` → incr 100 (place 10000 / 10, cap 100).
        assert_eq!(default_insert_incr(240000), 100);
        assert_eq!(default_insert_incr(10000), 100);
        assert_eq!(default_insert_incr(5), 0);
    }

    #[test]
    fn parse_range_forms() {
        assert_eq!(parse_seq_range("73-80").unwrap(), (73, 80));
        assert_eq!(parse_seq_range("1.8").unwrap(), (1, 8));
        assert_eq!(parse_seq_range("8").unwrap(), (8, 8));
        assert!(parse_seq_range("WORD").is_err());
        assert!(parse_seq_range("80-73").is_err());
    }

    #[test]
    fn parse_operand_order() {
        let s = parse_update(&[
            "1-8".into(),
            "FIRST".into(),
            "LAST".into(),
            "*".into(),
            "x".into(),
        ])
        .unwrap();
        assert_eq!((s.seq_start, s.seq_end), (1, 8));
        assert!(s.first && s.last);
        assert_eq!(s.comment, "x");
        assert!(parse_update(&["LAST".into(), "FIRST".into()]).is_err());
        assert!(parse_update(&["SEQ8".into()]).is_err());
        assert!(parse_update(&["FIRST".into(), "1-8".into()]).is_err());
    }
}
