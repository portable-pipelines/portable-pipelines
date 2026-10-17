//! SPLIT — Author's Edition 1.1.11, pp. 505–506.
//!
//! Syntax (fixed order):
//! `[ANYCase] [MINimum number] [AT | [snumber] BEFORE | [snumber] AFTER] [NOT] [BLANK | target [number]]`
//!
//! `target` is an `xrange`, `STRing dstring`, or `ANYof dstring`.

use pipe_core::{Record, StageCtx};

use crate::common::{is_anycase, matches_abbrev};
use crate::xrange::{
    anyof_target, cut_index, decode_target_dstring, find_match, parse_bare_target,
    parse_positive_usize, Placement, Target,
};

pub fn split(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_split(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("SPLIT: {e}"));
                return;
            }
        };

        while let Ok(Some(r)) = ctx.read() {
            // Note 1: copy null input records; never generate null records.
            if r.is_empty() {
                if ctx.write(Record::empty()).is_err() {
                    return;
                }
                continue;
            }

            for piece in apply_split(r.as_bytes(), &spec) {
                if piece.is_empty() {
                    continue;
                }
                if ctx.write(Record::from(piece)).is_err() {
                    return;
                }
            }
        }
    })
}

struct SplitSpec {
    anycase: bool,
    minimum: usize,
    placement: Placement,
    not: bool,
    target: Target,
    count: Option<u64>,
}

fn parse_split(args: &[String]) -> Result<SplitSpec, String> {
    let mut i = 0;

    // [ANYCase] — min 4; CASEANY / IGNORECASE / CASEIGNORE are synonyms.
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    // [MINimum number]
    let mut minimum = 0usize;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if matches_abbrev(&upper, "MINIMUM", 3) {
            i += 1;
            if i >= args.len() {
                return Err("MINIMUM requires a number".into());
            }
            let n = parse_positive_usize(&args[i], "MINIMUM")?;
            minimum = n;
            i += 1;
        }
    }

    // [AT | [snumber] BEFORE | [snumber] AFTER]
    // A number is an snumber only when the next word is BEFORE or AFTER.
    let mut placement = Placement::At;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "AT" {
            placement = Placement::At;
            i += 1;
        } else if upper == "BEFORE" {
            placement = Placement::Before(0);
            i += 1;
        } else if upper == "AFTER" {
            placement = Placement::After(0);
            i += 1;
        } else if let Ok(offset) = args[i].parse::<i32>() {
            if i + 1 < args.len() {
                let next = args[i + 1].to_ascii_uppercase();
                if next == "BEFORE" {
                    placement = Placement::Before(offset);
                    i += 2;
                } else if next == "AFTER" {
                    placement = Placement::After(offset);
                    i += 2;
                }
            }
        }
    }

    // [NOT]
    let mut not = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("NOT") {
        not = true;
        i += 1;
    }

    // [BLANK | target [number]]
    let mut target = None;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "BLANK" || upper == "SPACE" {
            target = Some(Target::Blank);
            i += 1;
        } else if matches_abbrev(&upper, "STRING", 3) {
            i += 1;
            if i >= args.len() {
                return Err("STRING requires a delimited string".into());
            }
            target = Some(Target::String(decode_target_dstring(&args[i])?));
            i += 1;
        } else if matches_abbrev(&upper, "ANYOF", 3) {
            i += 1;
            if i >= args.len() {
                return Err("ANYOF requires a delimited string".into());
            }
            target = Some(anyof_target(&args[i], anycase)?);
            i += 1;
        } else {
            match parse_bare_target(&args[i], anycase)? {
                Some(t) => {
                    target = Some(t);
                    i += 1;
                }
                None => {
                    if args[i].parse::<u64>().is_ok() {
                        return Err(
                            "count is only valid after a target (for example: SPLIT BLANK 2)"
                                .into(),
                        );
                    }
                    return Err(format!("unexpected operand {}", args[i]));
                }
            }
        }
    }

    // Count is only valid after a target (including explicit BLANK).
    let mut count = None;
    if i < args.len() {
        if target.is_none() {
            return Err(format!(
                "count is only valid after a target, not before placement (got {})",
                args[i]
            ));
        }
        let n = args[i].parse::<u64>().map_err(|_| {
            let u = args[i].to_ascii_uppercase();
            if u == "BEFORE" || u == "AFTER" || u == "AT" {
                format!("{u} must appear before the target")
            } else {
                format!("unexpected operand {}", args[i])
            }
        })?;
        if n == 0 {
            return Err("count must be at least 1".into());
        }
        count = Some(n);
        i += 1;
    }

    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(SplitSpec {
        anycase,
        minimum,
        placement,
        not,
        target: target.unwrap_or(Target::Blank),
        count,
    })
}

fn apply_split(bytes: &[u8], spec: &SplitSpec) -> Vec<Vec<u8>> {
    let mut pieces = Vec::new();
    let mut prev = 0usize;
    let mut search = spec.minimum.min(bytes.len());
    let mut made = 0u64;
    let max = spec.count.unwrap_or(u64::MAX);

    while search < bytes.len() && made < max {
        let Some((ms, mlen)) = find_match(bytes, search, &spec.target, spec.not, spec.anycase)
        else {
            break;
        };
        let sp = cut_index(spec.placement, ms, mlen, bytes.len());

        if sp > prev {
            emit(&mut pieces, &bytes[prev..sp]);
        }

        match spec.placement {
            Placement::At => {
                prev = ms.saturating_add(mlen).min(bytes.len());
            }
            Placement::Before(_) | Placement::After(_) => {
                if sp > prev {
                    prev = sp.min(bytes.len());
                }
            }
        }

        search = ms.saturating_add(mlen).max(prev);
        if spec.minimum > 0 {
            search = search.max(prev.saturating_add(spec.minimum));
        }
        if search <= prev && spec.placement == Placement::At {
            search = prev;
        }
        made += 1;
    }

    if prev < bytes.len() {
        emit(&mut pieces, &bytes[prev..]);
    }
    pieces
}

fn emit(pieces: &mut Vec<Vec<u8>>, slice: &[u8]) {
    if !slice.is_empty() {
        pieces.push(slice.to_vec());
    }
}
