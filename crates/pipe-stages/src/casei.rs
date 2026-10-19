//! CASEI — Author's Edition 1.1.11 pp.285–286.
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `CASEI [ZONE inputRange] [REVERSE] word [string]`
//!
//! Control. Translates the nested stage’s argument string to uppercase and
//! passes uppercase copies of each input record (or of a ZONE) to that stage.
//! When the nested stage writes to its primary or secondary output, the
//! corresponding original input record is written to CASEI’s matching output.
//!
//! `word` is any registered stage (the book assumes a selection stage; the
//! diagram takes any `word`). Further tokens are that stage’s operands,
//! parsed once with CASEI (Note 2).

use pipe_core::{Record, StageCtx, StreamId};

use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, InputRange,
};
use crate::registry::StageRegistry;

pub fn casei(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_casei(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("CASEI: {e}"));
                return;
            }
        };
        run_casei(ctx, spec);
    })
}

struct CaseiSpec {
    range: Option<InputRange>,
    reverse: bool,
    verb: String,
    inner_args: Vec<String>,
}

fn parse_casei(args: &[String]) -> Result<CaseiSpec, String> {
    let mut i = 0;

    // [ZONE inputRange] [REVERSE] — whole words; railroad order (p.285).
    let mut range = None;
    if i < args.len() && args[i].eq_ignore_ascii_case("ZONE") {
        i += 1;
        if i >= args.len() || !looks_like_input_range_start(&args[i]) {
            return Err("ZONE requires an inputRange".into());
        }
        range = Some(parse_input_range(args, &mut i)?);
    }
    let mut reverse = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("REVERSE") {
        reverse = true;
        i += 1;
    }

    if i >= args.len() {
        return Err("a word (the name of the program to run) is required".into());
    }
    let verb = args[i].clone();
    i += 1;

    let registry = StageRegistry::default();
    if !registry.contains(&verb) {
        return Err(format!("unknown stage: {verb}"));
    }

    // p.285: translate the argument string to uppercase (portable ASCII A–Z).
    let inner_args: Vec<String> = args[i..].iter().map(|a| a.to_ascii_uppercase()).collect();

    Ok(CaseiSpec {
        range,
        reverse,
        verb,
        inner_args,
    })
}

fn transform(record: &[u8], spec: &CaseiSpec) -> Vec<u8> {
    let mut bytes = match &spec.range {
        Some(range) => extract_input_range(record, range),
        None => record.to_vec(),
    };
    for b in &mut bytes {
        *b = b.to_ascii_uppercase();
    }
    if spec.reverse {
        bytes.reverse();
    }
    bytes
}

fn extra_input_connected(ctx: &StageCtx) -> bool {
    ctx.connected_inputs()
        .iter()
        .any(|&s| s != StreamId::PRIMARY)
}

fn extra_output_connected(ctx: &StageCtx) -> bool {
    ctx.connected_outputs()
        .iter()
        .any(|&s| s.0 > StreamId::SECONDARY.0)
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

fn run_casei(ctx: &mut StageCtx, spec: CaseiSpec) {
    // p.285: records from the primary input; no other input may be connected.
    if extra_input_connected(ctx) {
        ctx.fail("CASEI: secondary input stream must not be connected");
        return;
    }
    if extra_output_connected(ctx) {
        ctx.fail("CASEI: at most two streams may be defined");
        return;
    }

    // Same as ZONE: stop when no output stream is connected.
    if !outputs_live(ctx) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    let registry = StageRegistry::default();
    let verb = spec.verb.clone();
    let inner = match registry.create(&verb, spec.inner_args.clone()) {
        Some(s) => s,
        None => {
            ctx.fail(format!("CASEI: unknown stage: {verb}"));
            return;
        }
    };

    let feed = ctx.spawn_stage("casei-feed", casei_feed(spec));
    let sel = ctx.spawn_stage(verb, inner);
    let restore = ctx.spawn_stage("casei-restore", casei_restore());

    ctx.retarget_input(StreamId::PRIMARY, feed, StreamId::PRIMARY);
    // Originals first (secondary) so the restorer arms before the nested
    // stage can write a trigger (same ordering as PREDSELECT / FANOUT).
    ctx.connect_streams(feed, StreamId::SECONDARY, restore, StreamId::PRIMARY);
    ctx.connect_streams(feed, StreamId::PRIMARY, sel, StreamId::PRIMARY);
    ctx.connect_streams(sel, StreamId::PRIMARY, restore, StreamId::SECONDARY);
    ctx.connect_streams(sel, StreamId::SECONDARY, restore, StreamId::TERTIARY);
    ctx.retarget_output(StreamId::PRIMARY, restore, StreamId::PRIMARY);
    if ctx.output_connected(StreamId::SECONDARY) {
        ctx.retarget_output(StreamId::SECONDARY, restore, StreamId::SECONDARY);
    }
}

/// Copy each original to the restorer, then the uppercase (zone / reverse)
/// form to the nested stage.
fn casei_feed(spec: CaseiSpec) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        ctx.select_input(StreamId::PRIMARY);
        loop {
            if !ctx.output_connected(StreamId::PRIMARY)
                && !ctx.output_connected(StreamId::SECONDARY)
            {
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
                return;
            }
            if ctx.input_record_ready(StreamId::PRIMARY) {
                let Some(orig) = ctx.peek() else {
                    continue;
                };
                let transformed = Record::from(transform(orig.as_bytes(), &spec));
                ctx.select_output(StreamId::SECONDARY);
                if ctx.write(orig).is_err() {
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.short_input();
                    return;
                }
                ctx.select_output(StreamId::PRIMARY);
                if ctx.write(transformed).is_err() {
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.short_input();
                    return;
                }
                ctx.select_input(StreamId::PRIMARY);
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
    })
}

/// Route each original onto the stream the nested stage wrote (p.285).
fn casei_restore() -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| loop {
        if !outputs_live(ctx) {
            short_inputs(ctx);
            return;
        }

        if ctx.input_record_ready(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            let Some(orig) = ctx.read().ok().flatten() else {
                short_inputs(ctx);
                return;
            };
            if !route_original(ctx, orig) {
                short_inputs(ctx);
                return;
            }
            continue;
        }

        if ctx.input_at_eof(StreamId::PRIMARY) {
            short_inputs(ctx);
            return;
        }

        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            short_inputs(ctx);
            return;
        }
    })
}

fn route_original(ctx: &mut StageCtx, orig: Record) -> bool {
    loop {
        if ctx.input_record_ready(StreamId::SECONDARY) {
            ctx.select_input(StreamId::SECONDARY);
            let _ = ctx.read();
            return emit(ctx, StreamId::PRIMARY, orig);
        }
        if ctx.input_record_ready(StreamId::TERTIARY) {
            ctx.select_input(StreamId::TERTIARY);
            let _ = ctx.read();
            return emit(ctx, StreamId::SECONDARY, orig);
        }
        if ctx.input_at_eof(StreamId::SECONDARY) && ctx.input_at_eof(StreamId::TERTIARY) {
            // Nested stage wrote nothing for this original (not a selection
            // stage). Drop it, matching ZONE’s “no write → dropped”.
            return outputs_live(ctx);
        }
        if ctx
            .wait_any_input(&[StreamId::SECONDARY, StreamId::TERTIARY])
            .is_err()
        {
            return false;
        }
    }
}

fn emit(ctx: &mut StageCtx, stream: StreamId, rec: Record) -> bool {
    if ctx.output_connected(stream) {
        ctx.select_output(stream);
        if ctx.write(rec).is_err() {
            return outputs_live(ctx);
        }
    }
    outputs_live(ctx)
}

fn short_inputs(ctx: &mut StageCtx) {
    for stream in ctx.connected_inputs() {
        if ctx.input_connected(stream) {
            ctx.select_input(stream);
            ctx.short_input();
        }
    }
}
