//! CHOP — Author's Edition 1.1.11, pp. 289–290 (examples pp.6, 12, 58–59).
//!
//! Syntax (two forms, fixed order):
//! `CHOP | TRUNCate [number]` — truncate after the column (default 80).
//! `CHOP [ANYCase] [[snumber] BEFORE | AFTER] [NOT] target`
//!
//! `target` is an `xrange`, `STRing dstring`, or `ANYof dstring`.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{is_anycase, matches_abbrev};
use crate::xrange::{
    anyof_target, cut_index, find_match, is_snumber_token, parse_bare_target, parse_column_number,
    parse_snumber, string_target, Placement, Target,
};

pub fn chop(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_chop(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("CHOP: {e}"));
                return;
            }
        };

        if ctx.input_connected(StreamId::SECONDARY) {
            ctx.fail("CHOP: secondary input stream must not be connected");
            return;
        }

        if !outputs_live(ctx) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        while outputs_live(ctx) {
            let rec = match ctx.read() {
                Ok(Some(r)) => r,
                Ok(None) | Err(_) => return,
            };
            let bytes = rec.as_bytes();
            let at = chop_at(bytes, &spec);
            let prefix = bytes[..at].to_vec();
            let rest = bytes[at..].to_vec();

            ctx.select_output(StreamId::PRIMARY);
            if ctx.write(Record::from(prefix)).is_err() && !outputs_live(ctx) {
                return;
            }
            ctx.select_output(StreamId::SECONDARY);
            if ctx.write(Record::from(rest)).is_err() && !outputs_live(ctx) {
                return;
            }
        }
    })
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

enum ChopSpec {
    /// Truncate after this 1-based column. 0 yields a null primary record.
    Column(u32),
    Target {
        anycase: bool,
        placement: Placement,
        not: bool,
        target: Target,
    },
}

fn parse_chop(args: &[String]) -> Result<ChopSpec, String> {
    let mut i = 0;

    // [ANYCase] — min 4; ANYOF wins at 3 (p.290 Note 3). Synonyms Note 4.
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    // [[snumber] BEFORE | AFTER] — BEFORE is the default for the target form.
    let mut placement_seen = false;
    let mut placement = Placement::Before(0);
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "BEFORE" {
            placement_seen = true;
            i += 1;
        } else if upper == "AFTER" {
            placement = Placement::After(0);
            placement_seen = true;
            i += 1;
        } else if is_snumber_token(&args[i]) && i + 1 < args.len() {
            let next = args[i + 1].to_ascii_uppercase();
            if next == "BEFORE" || next == "AFTER" {
                let n = parse_snumber(&args[i])?;
                placement = if next == "BEFORE" {
                    Placement::Before(n)
                } else {
                    Placement::After(n)
                };
                placement_seen = true;
                i += 2;
            }
        }
    }

    // [NOT]
    let mut not = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("NOT") {
        not = true;
        i += 1;
    }

    // Column form: a lone nonnegative number is a column, not xorc (unlike SPLIT).
    if !anycase && !placement_seen && !not {
        if i >= args.len() {
            return Ok(ChopSpec::Column(80));
        }
        if i + 1 == args.len() {
            if let Some(n) = parse_column_number(&args[i])? {
                return Ok(ChopSpec::Column(n));
            }
        }
    }

    if i >= args.len() {
        return Err(missing_target(anycase, placement_seen, not));
    }

    let upper = args[i].to_ascii_uppercase();
    let target = if matches_abbrev(&upper, "STRING", 3) {
        i += 1;
        if i >= args.len() {
            return Err("STRING requires a delimited string".into());
        }
        let t = string_target(&args[i])?;
        i += 1;
        t
    } else if matches_abbrev(&upper, "ANYOF", 3) {
        i += 1;
        if i >= args.len() {
            return Err("ANYOF requires a delimited string".into());
        }
        let t = anyof_target(&args[i], anycase)?;
        i += 1;
        t
    } else {
        match parse_bare_target(&args[i], anycase)? {
            Some(t) => {
                i += 1;
                t
            }
            None => return Err(unexpected_operand(&args[i])),
        }
    };

    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    Ok(ChopSpec::Target {
        anycase,
        placement,
        not,
        target,
    })
}

fn missing_target(anycase: bool, placement: bool, not: bool) -> String {
    if anycase || placement || not {
        "target required after ANYCASE / BEFORE / AFTER / NOT".into()
    } else {
        "target required".into()
    }
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if is_anycase(arg)
        || upper == "NOT"
        || upper == "BEFORE"
        || upper == "AFTER"
        || upper == "AT"
        || matches_abbrev(&upper, "MINIMUM", 3)
        || matches_abbrev(&upper, "STRING", 3)
        || matches_abbrev(&upper, "ANYOF", 3)
    {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn chop_at(bytes: &[u8], spec: &ChopSpec) -> usize {
    match spec {
        ChopSpec::Column(n) => {
            let n = *n as usize;
            n.min(bytes.len())
        }
        ChopSpec::Target {
            anycase,
            placement,
            not,
            target,
        } => match find_match(bytes, 0, target, *not, *anycase) {
            Some((ms, mlen)) => cut_index(*placement, ms, mlen, bytes.len()),
            None => bytes.len(),
        },
    }
}
