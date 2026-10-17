//! ZONE — Author's Edition 1.1.11, pp. 610–611 (tutorial pp.52, 67 Figure 140).
//!
//! Syntax (fixed order):
//! `ZONE inputRange [CASEI] [REVERSE] word [string]`
//!
//! `word` is the program to run (the book assumes a selection stage). ZONE
//! extracts the range (optional CASEI / REVERSE), runs that stage on the
//! extracted bytes, and writes the original record to the same output stream
//! the argument stage used.

use std::cell::RefCell;
use std::rc::Rc;

use pipe_core::{PipelineBuilder, Record, StageCtx, StreamId, TraceConfig};

use crate::input_range::{
    extract_input_range, looks_like_input_range_start, parse_input_range, InputRange,
};
use crate::registry::StageRegistry;

pub fn zone(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_zone(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("ZONE: {e}"));
                return;
            }
        };

        if ctx.input_connected(StreamId::SECONDARY) {
            ctx.fail("ZONE: secondary input stream must not be connected");
            return;
        }

        if !outputs_live(ctx) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_input(StreamId::PRIMARY);
        let originals = match ctx.read_all() {
            Ok(v) => v,
            Err(_) => return,
        };

        let zones: Vec<Record> = originals
            .iter()
            .map(|r| Record::from(transform_zone(r.as_bytes(), &spec)))
            .collect();

        let routes = match run_selection(&spec, zones) {
            Ok(v) => v,
            Err(e) => {
                ctx.fail(format!("ZONE: {e}"));
                return;
            }
        };

        for (orig, stream) in originals.into_iter().zip(routes) {
            if !ctx.output_connected(stream) {
                continue;
            }
            ctx.select_output(stream);
            if ctx.write(orig).is_err() && !outputs_live(ctx) {
                return;
            }
        }
    })
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

struct ZoneSpec {
    range: InputRange,
    casei: bool,
    reverse: bool,
    verb: String,
    inner_args: Vec<String>,
}

fn parse_zone(args: &[String]) -> Result<ZoneSpec, String> {
    let mut i = 0;
    if i >= args.len() || !looks_like_input_range_start(&args[i]) {
        return Err("inputRange is required".into());
    }
    let range = parse_input_range(args, &mut i)?;

    // [CASEI] [REVERSE] — whole words; railroad order (p.610).
    let mut casei = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("CASEI") {
        casei = true;
        i += 1;
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

    let mut inner_args: Vec<String> = args[i..].to_vec();
    // CASEI uppercases the argument string as well as the zone (p.285 / p.610).
    if casei {
        for a in &mut inner_args {
            *a = a.to_ascii_uppercase();
        }
    }

    Ok(ZoneSpec {
        range,
        casei,
        reverse,
        verb,
        inner_args,
    })
}

fn transform_zone(record: &[u8], spec: &ZoneSpec) -> Vec<u8> {
    let mut zone = extract_input_range(record, &spec.range);
    if spec.casei {
        for b in &mut zone {
            *b = b.to_ascii_uppercase();
        }
    }
    if spec.reverse {
        zone.reverse();
    }
    zone
}

/// Run the argument stage on the extracted zones.
///
/// Primary and secondary are separate connections, so drain order is not write
/// order. TRACE records each inner `write()` in call order (one original per
/// inner write).
fn run_selection(spec: &ZoneSpec, zones: Vec<Record>) -> Result<Vec<StreamId>, String> {
    let registry = StageRegistry::default();
    let inner = registry
        .create(&spec.verb, spec.inner_args.clone())
        .ok_or_else(|| format!("unknown stage: {}", spec.verb))?;

    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&trace)));
    let feed = b.add_stage(
        "zone-feed",
        Box::new(move |ctx: &mut StageCtx| {
            for rec in zones {
                if ctx.write(rec).is_err() {
                    break;
                }
            }
        }),
    );
    let sel = b.add_stage(spec.verb.clone(), inner);
    let drain_p = b.add_stage("zone-drain-0", hole_drain());
    let drain_s = b.add_stage("zone-drain-1", hole_drain());
    b.connect_primary(feed, sel);
    b.connect(sel, StreamId::PRIMARY, drain_p, StreamId::PRIMARY);
    b.connect(sel, StreamId::SECONDARY, drain_s, StreamId::PRIMARY);
    b.run().map_err(|e| e.to_string())?;
    let lines = trace.borrow().clone();
    Ok(routes_from_trace(&lines, &spec.verb))
}

fn hole_drain() -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(|ctx| while ctx.read().ok().flatten().is_some() {})
}

fn routes_from_trace(lines: &[String], verb: &str) -> Vec<StreamId> {
    let verb_u = verb.to_ascii_uppercase();
    let mut routes = Vec::new();
    for line in lines {
        let u = line.to_ascii_uppercase();
        let Some(rest) = u.split("WRITE STREAM=").nth(1) else {
            continue;
        };
        // Skip the feeder; drains only read.
        if u.contains("ZONE-FEED") {
            continue;
        }
        // Inner stage name is the ZONE argument verb.
        if !u.contains(&format!(" {verb_u} WRITE STREAM=")) {
            continue;
        }
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let after = rest[digits.len()..].trim_start();
        // Actual writes include `len=`; skip `wait` / `deliver`.
        if !after.starts_with("LEN=") {
            continue;
        }
        if let Ok(n) = digits.parse::<u32>() {
            routes.push(StreamId(n));
        }
    }
    routes
}
