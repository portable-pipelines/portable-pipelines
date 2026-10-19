//! LOOKUP — Author's Edition 1.1.11, pp. 416–423 (tutorial pp. 87–88).
//!
//! Syntax (fixed order):
//! `LOOKUP [COUNT] [INCREMENt] [SETCOUNT] [TRACKCOUnt] [COUNTDBG]
//!         [NOPAD | PAD xorc] [ANYcase]
//!         [AUTOADD [BEFORE] | CEILING | FLOOR]
//!         [KEYONLY] [STRICT]
//!         [inputRange [inputRange]]
//!         [DETAIL [MASTER | ALLMASTER [PAIRWISE]]
//!          | MASTER [DETAIL]
//!          | ALLMASTER [DETAIL [PAIRWISE]]
//!          | BOTH | MATCHING]`
//!
//! Details on the primary input; the secondary input is the initial reference
//! (read to EOF first). Tertiary input adds to the reference; quaternary input
//! deletes. Matched records → primary; unmatched details → secondary;
//! leftover masters → tertiary at EOF (ascending key); deleted masters →
//! quaternary; duplicate masters → quinary; unmatched deletes → senary.

use std::cmp::Ordering;

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{is_anycase, matches_abbrev};
use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, parse_xorc, InputRange,
};

const TERTIARY: StreamId = StreamId(2);
const QUATERNARY: StreamId = StreamId(3);
const QUINARY: StreamId = StreamId(4);
const SENARY: StreamId = StreamId(5);

/// Sticky fullword ceiling (1.1.11 p.423 Note 7).
const COUNT_MAX: i32 = 2_147_483_647;

pub fn lookup(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_lookup(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("LOOKUP: {e}"));
                return;
            }
        };
        run_lookup(ctx, spec);
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MatchKind {
    Exact,
    Ceiling,
    Floor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteMode {
    /// Default / BOTH: detail then the matching master(s).
    DetailMaster,
    /// DETAIL / MATCHING: detail only.
    Detail,
    /// MASTER: first matching master only.
    Master,
    /// MASTER DETAIL.
    MasterDetail,
    /// ALLMASTER: every matching master; detail discarded.
    AllMaster,
    /// ALLMASTER DETAIL.
    AllMasterDetail,
    /// DETAIL ALLMASTER PAIRWISE: detail, master for each master.
    DetailPairwise,
    /// ALLMASTER DETAIL PAIRWISE: master, detail for each master.
    MasterPairwise,
}

#[derive(Clone, Debug)]
struct LookupSpec {
    count: bool,
    increment: bool,
    setcount: bool,
    trackcount: bool,
    countdbg: bool,
    pad: Option<u8>,
    anycase: bool,
    autoadd: bool,
    autoadd_before: bool,
    match_kind: MatchKind,
    keyonly: bool,
    strict: bool,
    detail_range: Option<InputRange>,
    master_range: Option<InputRange>,
    write_mode: WriteMode,
    all_masters: bool,
}

impl LookupSpec {
    fn count_prefix(&self) -> bool {
        self.count || self.countdbg
    }

    fn prefix_on(&self, stream: StreamId) -> bool {
        if stream == StreamId::PRIMARY {
            self.trackcount
        } else if stream == TERTIARY || stream == QUATERNARY {
            self.count_prefix() || self.trackcount
        } else {
            false
        }
    }
}

struct Master {
    key: Vec<u8>,
    record: Record,
    count: i32,
}

fn parse_lookup(args: &[String]) -> Result<LookupSpec, String> {
    let mut i = 0;

    let mut count = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("COUNT") {
        count = true;
        i += 1;
    }

    let mut increment = false;
    if i < args.len() && is_increment(&args[i]) {
        increment = true;
        i += 1;
    }

    let mut setcount = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("SETCOUNT") {
        setcount = true;
        i += 1;
    }

    let mut trackcount = false;
    if i < args.len() && is_trackcount(&args[i]) {
        trackcount = true;
        i += 1;
    }

    // Note 1: test keyword; four characters are prefixed to the count field.
    let mut countdbg = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("COUNTDBG") {
        countdbg = true;
        i += 1;
    }

    let mut pad = None;
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
            i += 1;
        }
    }

    let mut anycase = false;
    if i < args.len() && is_lookup_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    // AUTOADD / CEILING / FLOOR, KEYONLY, STRICT, ranges, and the write-mode
    // keywords. The railroad puts the flags before the ranges (p.416); the
    // printed example `lookup 1.5 autoadd keyonly` (p.421) puts the range first.
    let mut autoadd = false;
    let mut autoadd_before = false;
    let mut match_kind = MatchKind::Exact;
    let mut keyonly = false;
    let mut strict = false;
    let mut detail_range = None;
    let mut master_range = None;
    let mut write_mode = WriteMode::DetailMaster;
    let mut all_masters = false;
    let mut saw_write = false;

    while i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if !autoadd && match_kind == MatchKind::Exact && upper == "AUTOADD" {
            autoadd = true;
            i += 1;
            if i < args.len() && args[i].eq_ignore_ascii_case("BEFORE") {
                autoadd_before = true;
                i += 1;
            }
            continue;
        }
        if !autoadd && match_kind == MatchKind::Exact && upper == "CEILING" {
            match_kind = MatchKind::Ceiling;
            i += 1;
            continue;
        }
        if !autoadd && match_kind == MatchKind::Exact && upper == "FLOOR" {
            match_kind = MatchKind::Floor;
            i += 1;
            continue;
        }
        if !keyonly && upper == "KEYONLY" {
            keyonly = true;
            i += 1;
            continue;
        }
        if !strict && upper == "STRICT" {
            strict = true;
            i += 1;
            continue;
        }
        if detail_range.is_none()
            && looks_like_input_range_start(&args[i])
            && !is_write_keyword(&args[i])
        {
            detail_range = Some(parse_input_range(args, &mut i)?);
            if i < args.len()
                && looks_like_input_range_start(&args[i])
                && !is_write_keyword(&args[i])
            {
                master_range = Some(parse_input_range(args, &mut i)?);
            }
            continue;
        }
        if !saw_write && is_write_keyword(&args[i]) {
            let (mode, all) = parse_write_mode(args, &mut i)?;
            write_mode = mode;
            all_masters = all;
            saw_write = true;
            continue;
        }
        return Err(unexpected_operand(&args[i]));
    }

    if autoadd && match_kind != MatchKind::Exact {
        return Err("AUTOADD, CEILING, and FLOOR are mutually exclusive".into());
    }
    if autoadd {
        if let (Some(d), Some(m)) = (&detail_range, &master_range) {
            if d != m {
                return Err(
                    "AUTOADD requires the master input range to match the detail range".into(),
                );
            }
        }
    }

    Ok(LookupSpec {
        count,
        increment,
        setcount,
        trackcount,
        countdbg,
        pad,
        anycase,
        autoadd,
        autoadd_before,
        match_kind,
        keyonly,
        strict,
        detail_range,
        master_range,
        write_mode,
        all_masters,
    })
}

fn is_increment(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "INCREMENT", 8)
}

fn is_trackcount(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "TRACKCOUNT", 8)
}

fn is_lookup_anycase(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    matches_abbrev(&u, "ANYCASE", 3)
        || u == "CASEANY"
        || u == "IGNORECASE"
        || u == "CASEIGNORE"
        || is_anycase(s)
}

fn is_allmaster(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    u == "ALLMASTER" || u == "ALLMASTERS"
}

fn is_write_keyword(s: &str) -> bool {
    let u = s.to_ascii_uppercase();
    u == "DETAIL"
        || u == "MASTER"
        || is_allmaster(s)
        || u == "PAIRWISE"
        || u == "BOTH"
        || u == "MATCHING"
}

fn parse_write_mode(args: &[String], i: &mut usize) -> Result<(WriteMode, bool), String> {
    let upper = args[*i].to_ascii_uppercase();
    if upper == "BOTH" {
        *i += 1;
        return Ok((WriteMode::DetailMaster, false));
    }
    if upper == "MATCHING" {
        *i += 1;
        return Ok((WriteMode::Detail, false));
    }
    if upper == "DETAIL" {
        *i += 1;
        if *i < args.len() && args[*i].eq_ignore_ascii_case("MASTER") {
            *i += 1;
            return Ok((WriteMode::DetailMaster, false));
        }
        if *i < args.len() && is_allmaster(&args[*i]) {
            *i += 1;
            if *i < args.len() && args[*i].eq_ignore_ascii_case("PAIRWISE") {
                *i += 1;
                return Ok((WriteMode::DetailPairwise, true));
            }
            return Ok((WriteMode::DetailMaster, true));
        }
        return Ok((WriteMode::Detail, false));
    }
    if upper == "MASTER" {
        *i += 1;
        if *i < args.len() && args[*i].eq_ignore_ascii_case("DETAIL") {
            *i += 1;
            return Ok((WriteMode::MasterDetail, false));
        }
        return Ok((WriteMode::Master, false));
    }
    if is_allmaster(&args[*i]) {
        *i += 1;
        if *i < args.len() && args[*i].eq_ignore_ascii_case("DETAIL") {
            *i += 1;
            if *i < args.len() && args[*i].eq_ignore_ascii_case("PAIRWISE") {
                *i += 1;
                return Ok((WriteMode::MasterPairwise, true));
            }
            return Ok((WriteMode::AllMasterDetail, true));
        }
        if *i < args.len() && args[*i].eq_ignore_ascii_case("PAIRWISE") {
            return Err("PAIRWISE requires DETAIL with ALLMASTER".into());
        }
        return Ok((WriteMode::AllMaster, true));
    }
    if upper == "PAIRWISE" {
        return Err("PAIRWISE requires DETAIL with ALLMASTER".into());
    }
    Err(unexpected_operand(&args[*i]))
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if arg.eq_ignore_ascii_case("COUNT")
        || is_increment(arg)
        || arg.eq_ignore_ascii_case("SETCOUNT")
        || is_trackcount(arg)
        || arg.eq_ignore_ascii_case("COUNTDBG")
        || upper == "PAD"
        || upper == "NOPAD"
        || is_lookup_anycase(arg)
        || upper == "AUTOADD"
        || upper == "BEFORE"
        || upper == "CEILING"
        || upper == "FLOOR"
        || upper == "KEYONLY"
        || upper == "STRICT"
        || is_write_keyword(arg)
    {
        format!("{upper} must appear in diagram order")
    } else if upper == "AFTER" {
        "AFTER is not a LOOKUP keyword".into()
    } else {
        format!("unexpected operand {arg}")
    }
}

fn run_lookup(ctx: &mut StageCtx, spec: LookupSpec) {
    if ctx.input_connected(QUINARY) || ctx.input_connected(SENARY) {
        ctx.fail("LOOKUP: quinary and senary input streams must not be connected");
        return;
    }
    for n in 6u32..16 {
        let s = StreamId(n);
        if ctx.input_connected(s) {
            ctx.fail(format!(
                "LOOKUP: unused input stream {n} must not be connected"
            ));
            return;
        }
        if ctx.output_connected(s) {
            ctx.fail(format!(
                "LOOKUP: unused output stream {n} must not be connected"
            ));
            return;
        }
    }

    if !spec.autoadd && !has_side_stream(ctx) {
        ctx.fail("LOOKUP: at least two streams are required");
        return;
    }

    if !any_output_live(ctx) {
        short_all_inputs(ctx);
        return;
    }

    let mut masters: Vec<Master> = Vec::new();
    if !load_reference(ctx, &spec, &mut masters) {
        return;
    }
    if !any_output_live(ctx) {
        short_all_inputs(ctx);
        return;
    }

    process_live_inputs(ctx, &spec, &mut masters);

    if ctx.output_connected(TERTIARY) {
        sever_all_except_tertiary_out(ctx);
        write_unreferenced(ctx, &spec, &mut masters);
    }
}

fn has_side_stream(ctx: &StageCtx) -> bool {
    for n in 1u32..6 {
        let s = StreamId(n);
        if ctx.input_connected(s) || ctx.output_connected(s) {
            return true;
        }
    }
    false
}

fn any_output_live(ctx: &StageCtx) -> bool {
    (0u32..6).any(|n| ctx.output_connected(StreamId(n)))
}

fn short_all_inputs(ctx: &mut StageCtx) {
    for n in 0u32..6 {
        if ctx.input_connected(StreamId(n)) {
            ctx.select_input(StreamId(n));
            ctx.short_input();
        }
    }
}

fn load_reference(ctx: &mut StageCtx, spec: &LookupSpec, masters: &mut Vec<Master>) -> bool {
    ctx.select_input(StreamId::SECONDARY);
    loop {
        match ctx.read() {
            Ok(Some(rec)) => {
                if !ingest_master(ctx, spec, masters, rec) {
                    return false;
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    true
}

fn ingest_master(
    ctx: &mut StageCtx,
    spec: &LookupSpec,
    masters: &mut Vec<Master>,
    rec: Record,
) -> bool {
    let (initial, body) = match strip_setcount(spec, rec.as_bytes()) {
        Ok(v) => v,
        Err(e) => {
            ctx.fail(format!("LOOKUP: SETCOUNT: {e}"));
            return false;
        }
    };
    let key = extract_key(
        &body,
        spec.master_range.as_ref().or(spec.detail_range.as_ref()),
    );
    let stored = if spec.keyonly {
        Record::from(key.clone())
    } else {
        Record::from(body)
    };
    if !spec.all_masters && first_index(masters, spec, &key).is_some() {
        return emit(ctx, QUINARY, stored);
    }
    masters.push(Master {
        key,
        record: stored,
        count: initial,
    });
    true
}

fn process_live_inputs(ctx: &mut StageCtx, spec: &LookupSpec, masters: &mut Vec<Master>) {
    let mut primary_eof = ctx.input_at_eof(StreamId::PRIMARY);
    let mut tertiary_eof = ctx.input_at_eof(TERTIARY);
    let mut quaternary_eof = ctx.input_at_eof(QUATERNARY);

    if primary_eof {
        propagate_primary_eof(ctx);
    }
    if tertiary_eof {
        propagate_tertiary_eof(ctx);
    }
    if quaternary_eof {
        propagate_quaternary_eof(ctx);
    }

    loop {
        if !any_output_live(ctx) {
            short_all_inputs(ctx);
            return;
        }
        maybe_short_primary(ctx);

        let order = if spec.strict {
            [TERTIARY, QUATERNARY, StreamId::PRIMARY]
        } else {
            [StreamId::PRIMARY, TERTIARY, QUATERNARY]
        };
        let live: Vec<StreamId> = order
            .into_iter()
            .filter(|&s| ctx.input_connected(s) && !ctx.input_at_eof(s))
            .collect();
        if live.is_empty() {
            if !primary_eof {
                propagate_primary_eof(ctx);
            }
            if !tertiary_eof {
                propagate_tertiary_eof(ctx);
            }
            if !quaternary_eof {
                propagate_quaternary_eof(ctx);
            }
            break;
        }

        match ctx.read_any(&live) {
            Ok(Some((stream, rec))) => {
                if stream == StreamId::PRIMARY {
                    if !handle_detail(ctx, spec, masters, rec) {
                        return;
                    }
                } else if stream == TERTIARY {
                    if !ingest_master(ctx, spec, masters, rec) {
                        return;
                    }
                } else if !handle_delete(ctx, spec, masters, rec) {
                    return;
                }
            }
            Ok(None) => {
                if !primary_eof {
                    propagate_primary_eof(ctx);
                }
                if !tertiary_eof {
                    propagate_tertiary_eof(ctx);
                }
                if !quaternary_eof {
                    propagate_quaternary_eof(ctx);
                }
                break;
            }
            Err(_) => return,
        }

        if !primary_eof && ctx.input_at_eof(StreamId::PRIMARY) {
            primary_eof = true;
            propagate_primary_eof(ctx);
        }
        if !tertiary_eof && ctx.input_at_eof(TERTIARY) {
            tertiary_eof = true;
            propagate_tertiary_eof(ctx);
        }
        if !quaternary_eof && ctx.input_at_eof(QUATERNARY) {
            quaternary_eof = true;
            propagate_quaternary_eof(ctx);
        }
    }
}

fn propagate_primary_eof(ctx: &mut StageCtx) {
    if ctx.output_connected(StreamId::PRIMARY) {
        ctx.select_output(StreamId::PRIMARY);
        ctx.sever_output();
    }
    if ctx.output_connected(StreamId::SECONDARY) {
        ctx.select_output(StreamId::SECONDARY);
        ctx.sever_output();
    }
}

fn propagate_tertiary_eof(ctx: &mut StageCtx) {
    if ctx.output_connected(QUINARY) {
        ctx.select_output(QUINARY);
        ctx.sever_output();
    }
}

fn propagate_quaternary_eof(ctx: &mut StageCtx) {
    if ctx.output_connected(QUATERNARY) {
        ctx.select_output(QUATERNARY);
        ctx.sever_output();
    }
    if ctx.output_connected(SENARY) {
        ctx.select_output(SENARY);
        ctx.sever_output();
    }
}

fn maybe_short_primary(ctx: &mut StageCtx) {
    if !ctx.output_connected(StreamId::PRIMARY)
        && !ctx.output_connected(StreamId::SECONDARY)
        && !ctx.output_connected(TERTIARY)
        && ctx.input_connected(StreamId::PRIMARY)
    {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
    }
}

fn handle_detail(
    ctx: &mut StageCtx,
    spec: &LookupSpec,
    masters: &mut Vec<Master>,
    rec: Record,
) -> bool {
    let (delta, body) = match strip_increment(spec, rec.as_bytes()) {
        Ok(v) => v,
        Err(e) => {
            ctx.fail(format!("LOOKUP: INCREMENT: {e}"));
            return false;
        }
    };
    let key = extract_key(&body, spec.detail_range.as_ref());
    let detail = Record::from(body);

    if spec.autoadd && spec.autoadd_before && first_index(masters, spec, &key).is_none() {
        add_auto(spec, masters, &key, &detail);
    }

    let found = find_matches(masters, spec, &key);
    if found.is_empty() {
        if !emit(ctx, StreamId::SECONDARY, detail.clone()) {
            return false;
        }
        if spec.autoadd && !spec.autoadd_before {
            add_auto(spec, masters, &key, &detail);
        }
        return true;
    }

    for &idx in &found {
        masters[idx].count = add_count(masters[idx].count, delta);
    }
    write_match(ctx, spec, &detail, masters, &found)
}

fn add_auto(spec: &LookupSpec, masters: &mut Vec<Master>, key: &[u8], detail: &Record) {
    let stored = if spec.keyonly {
        Record::from(key.to_vec())
    } else {
        detail.clone()
    };
    masters.push(Master {
        key: key.to_vec(),
        record: stored,
        count: 0,
    });
}

fn handle_delete(
    ctx: &mut StageCtx,
    spec: &LookupSpec,
    masters: &mut Vec<Master>,
    rec: Record,
) -> bool {
    // Write quaternary output before the delete record is consumed (p.420).
    ctx.delay_record(rec.clone());
    let key = extract_key(
        rec.as_bytes(),
        spec.master_range.as_ref().or(spec.detail_range.as_ref()),
    );
    let found = find_all_indices(masters, spec, &key);
    let ok = if found.is_empty() {
        emit(ctx, SENARY, rec)
    } else {
        let mut live = true;
        for idx in found.iter().rev() {
            let m = masters.remove(*idx);
            if spec.count_prefix() || m.count == 0 {
                if !emit_master(ctx, spec, QUATERNARY, &m) {
                    live = false;
                    break;
                }
            }
        }
        live
    };
    let _ = ctx.read();
    ok
}

fn write_match(
    ctx: &mut StageCtx,
    spec: &LookupSpec,
    detail: &Record,
    masters: &[Master],
    found: &[usize],
) -> bool {
    let first = found[0];
    match spec.write_mode {
        WriteMode::Detail => emit(ctx, StreamId::PRIMARY, detail.clone()),
        WriteMode::DetailMaster => {
            if !emit(ctx, StreamId::PRIMARY, detail.clone()) {
                return false;
            }
            for &idx in found {
                if !emit_master(ctx, spec, StreamId::PRIMARY, &masters[idx]) {
                    return false;
                }
                if !spec.all_masters {
                    break;
                }
            }
            true
        }
        WriteMode::Master => emit_master(ctx, spec, StreamId::PRIMARY, &masters[first]),
        WriteMode::MasterDetail => {
            if !emit_master(ctx, spec, StreamId::PRIMARY, &masters[first]) {
                return false;
            }
            emit(ctx, StreamId::PRIMARY, detail.clone())
        }
        WriteMode::AllMaster => {
            for &idx in found {
                if !emit_master(ctx, spec, StreamId::PRIMARY, &masters[idx]) {
                    return false;
                }
            }
            true
        }
        WriteMode::AllMasterDetail => {
            for &idx in found {
                if !emit_master(ctx, spec, StreamId::PRIMARY, &masters[idx]) {
                    return false;
                }
            }
            emit(ctx, StreamId::PRIMARY, detail.clone())
        }
        WriteMode::DetailPairwise => {
            for &idx in found {
                if !emit(ctx, StreamId::PRIMARY, detail.clone()) {
                    return false;
                }
                if !emit_master(ctx, spec, StreamId::PRIMARY, &masters[idx]) {
                    return false;
                }
            }
            true
        }
        WriteMode::MasterPairwise => {
            for &idx in found {
                if !emit_master(ctx, spec, StreamId::PRIMARY, &masters[idx]) {
                    return false;
                }
                if !emit(ctx, StreamId::PRIMARY, detail.clone()) {
                    return false;
                }
            }
            true
        }
    }
}

fn write_unreferenced(ctx: &mut StageCtx, spec: &LookupSpec, masters: &mut [Master]) {
    masters.sort_by(|a, b| keys_cmp(&a.key, &b.key, spec.pad, spec.anycase));
    for m in masters.iter() {
        if spec.count_prefix() || m.count == 0 {
            if !emit_master(ctx, spec, TERTIARY, m) {
                return;
            }
        }
    }
}

fn sever_all_except_tertiary_out(ctx: &mut StageCtx) {
    for n in 0u32..6 {
        let s = StreamId(n);
        if ctx.input_connected(s) {
            ctx.select_input(s);
            ctx.short_input();
        }
        if s != TERTIARY && ctx.output_connected(s) {
            ctx.select_output(s);
            ctx.sever_output();
        }
    }
}

fn first_index(masters: &[Master], spec: &LookupSpec, key: &[u8]) -> Option<usize> {
    masters
        .iter()
        .position(|m| keys_equal(&m.key, key, spec.pad, spec.anycase))
}

fn find_all_indices(masters: &[Master], spec: &LookupSpec, key: &[u8]) -> Vec<usize> {
    masters
        .iter()
        .enumerate()
        .filter(|(_, m)| keys_equal(&m.key, key, spec.pad, spec.anycase))
        .map(|(i, _)| i)
        .collect()
}

fn find_matches(masters: &[Master], spec: &LookupSpec, key: &[u8]) -> Vec<usize> {
    match spec.match_kind {
        MatchKind::Exact => find_all_indices(masters, spec, key),
        MatchKind::Ceiling => bound_key(masters, spec, key, Ordering::Less)
            .map(|k| find_all_indices(masters, spec, &k))
            .unwrap_or_default(),
        MatchKind::Floor => bound_key(masters, spec, key, Ordering::Greater)
            .map(|k| find_all_indices(masters, spec, &k))
            .unwrap_or_default(),
    }
}

/// Smallest key that is not `skip` relative to `key` (CEILING skips Less;
/// FLOOR skips Greater).
fn bound_key(masters: &[Master], spec: &LookupSpec, key: &[u8], skip: Ordering) -> Option<Vec<u8>> {
    let mut best: Option<Vec<u8>> = None;
    for m in masters {
        if keys_cmp(&m.key, key, spec.pad, spec.anycase) == skip {
            continue;
        }
        match &best {
            None => best = Some(m.key.clone()),
            Some(bk) => {
                let prefer = if skip == Ordering::Less {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
                if keys_cmp(&m.key, bk, spec.pad, spec.anycase) == prefer {
                    best = Some(m.key.clone());
                }
            }
        }
    }
    best
}

fn extract_key(record: &[u8], range: Option<&InputRange>) -> Vec<u8> {
    match range {
        None => record.to_vec(),
        Some(r) => extract_input_range(record, r),
    }
}

fn keys_equal(a: &[u8], b: &[u8], pad: Option<u8>, anycase: bool) -> bool {
    keys_cmp(a, b, pad, anycase) == Ordering::Equal
}

fn keys_cmp(a: &[u8], b: &[u8], pad: Option<u8>, anycase: bool) -> Ordering {
    let mut left = a.to_vec();
    let mut right = b.to_vec();
    if anycase {
        for x in &mut left {
            *x = x.to_ascii_uppercase();
        }
        for x in &mut right {
            *x = x.to_ascii_uppercase();
        }
    }
    if let Some(p) = pad {
        if left.len() < right.len() {
            left.resize(right.len(), p);
        } else if right.len() < left.len() {
            right.resize(left.len(), p);
        }
        left.cmp(&right)
    } else if left.len() != right.len() {
        let n = left.len().min(right.len());
        match left[..n].cmp(&right[..n]) {
            Ordering::Equal => left.len().cmp(&right.len()),
            o => o,
        }
    } else {
        left.cmp(&right)
    }
}

fn strip_increment(spec: &LookupSpec, rec: &[u8]) -> Result<(i32, Vec<u8>), String> {
    if !spec.increment {
        return Ok((1, rec.to_vec()));
    }
    parse_count_prefix(rec, true, 1)
}

fn strip_setcount(spec: &LookupSpec, rec: &[u8]) -> Result<(i32, Vec<u8>), String> {
    if !spec.setcount {
        return Ok((0, rec.to_vec()));
    }
    parse_count_prefix(rec, false, 0)
}

fn parse_count_prefix(rec: &[u8], allow_neg: bool, default: i32) -> Result<(i32, Vec<u8>), String> {
    if rec.len() < 10 {
        return Err("input record is shorter than 10 bytes".into());
    }
    let field = &rec[..10];
    let rest = rec[10..].to_vec();
    let s = std::str::from_utf8(field).map_err(|_| "count field is not a number".to_string())?;
    let t = s.trim();
    if t.is_empty() {
        return Ok((default, rest));
    }
    let n: i32 = t
        .parse()
        .map_err(|_| format!("count field is not a number ({t})"))?;
    if !allow_neg && n < 0 {
        return Err("count must be non-negative".into());
    }
    Ok((n, rest))
}

fn add_count(cur: i32, delta: i32) -> i32 {
    let v = i64::from(cur) + i64::from(delta);
    if v < 0 {
        0
    } else if v > i64::from(COUNT_MAX) {
        COUNT_MAX
    } else {
        v as i32
    }
}

fn count_bytes(n: i32, dbg: bool) -> Vec<u8> {
    let body = format!("{n:>10}").into_bytes();
    if dbg {
        let mut out = b"    ".to_vec();
        out.extend_from_slice(&body);
        out
    } else {
        body
    }
}

fn with_count_prefix(rec: &Record, n: i32, dbg: bool) -> Record {
    let mut out = count_bytes(n, dbg);
    out.extend_from_slice(rec.as_bytes());
    Record::from(out)
}

fn emit_master(ctx: &mut StageCtx, spec: &LookupSpec, stream: StreamId, master: &Master) -> bool {
    let rec = if spec.prefix_on(stream) {
        with_count_prefix(&master.record, master.count, spec.countdbg)
    } else {
        master.record.clone()
    };
    emit(ctx, stream, rec)
}

fn emit(ctx: &mut StageCtx, stream: StreamId, rec: Record) -> bool {
    if !any_output_live(ctx) {
        return false;
    }
    if !ctx.output_connected(stream) {
        return true;
    }
    ctx.select_output(stream);
    let _ = ctx.write(rec);
    any_output_live(ctx)
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    fn parse(words: &[&str]) -> LookupSpec {
        let args: Vec<String> = words.iter().map(|s| (*s).to_string()).collect();
        parse_lookup(&args).expect("parse")
    }

    #[test]
    fn default_detail_master() {
        let s = parse(&[]);
        assert_eq!(s.write_mode, WriteMode::DetailMaster);
        assert!(!s.all_masters);
        assert_eq!(s.match_kind, MatchKind::Exact);
    }

    #[test]
    fn both_and_matching() {
        assert_eq!(parse(&["BOTH"]).write_mode, WriteMode::DetailMaster);
        assert_eq!(parse(&["MATCHING"]).write_mode, WriteMode::Detail);
    }

    #[test]
    fn allmasters_spelling() {
        assert_eq!(parse(&["ALLMASTERS"]).write_mode, WriteMode::AllMaster);
        assert_eq!(
            parse(&["DETAIL", "ALLMASTER", "PAIRWISE"]).write_mode,
            WriteMode::DetailPairwise
        );
    }

    #[test]
    fn increment_abbrev() {
        assert!(parse_lookup(&["INCREMEN".into()]).unwrap().increment);
        assert!(parse_lookup(&["INCREMENT".into()]).unwrap().increment);
        assert!(parse_lookup(&["INCREME".into()]).is_err());
    }

    #[test]
    fn trackcount_abbrev() {
        assert!(parse_lookup(&["TRACKCOU".into()]).unwrap().trackcount);
        assert!(parse_lookup(&["TRACKCO".into()]).is_err());
    }

    #[test]
    fn autoadd_before_and_ranges() {
        let s = parse(&["AUTOADD", "BEFORE", "1.5", "1.5"]);
        assert!(s.autoadd && s.autoadd_before);
        assert!(parse_lookup(&["AUTOADD".into(), "1.5".into(), "2.5".into()]).is_err());
    }

    #[test]
    fn pairwise_needs_allmaster_detail() {
        assert!(parse_lookup(&["PAIRWISE".into()]).is_err());
        assert!(parse_lookup(&["ALLMASTER".into(), "PAIRWISE".into()]).is_err());
    }
}
