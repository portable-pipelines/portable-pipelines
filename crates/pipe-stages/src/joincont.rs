//! JOINCONT — Author's Edition 1.1.11, pp. 401–403.
//!
//! Syntax (fixed order):
//! `[ANYCase] [NOT] [TRAILING | RANGE inputRange | LEADING] [DELAY] [ANYof]
//!  delimitedString [KEEP] [delimitedString]`

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase, matches_abbrev};
use crate::input_range::{extract_input_range, parse_input_range, InputRange};

pub fn joincont(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_joincont(&args) {
            Ok(spec) => spec,
            Err(e) => {
                ctx.fail(format!("JOINCONT: {e}"));
                return;
            }
        };

        if !ctx.output_connected(StreamId::PRIMARY) {
            ctx.select_input(StreamId::PRIMARY);
            ctx.short_input();
            return;
        }

        ctx.select_output(StreamId::PRIMARY);
        match spec.where_ {
            Where::Leading => run_leading(ctx, &spec),
            Where::Trailing | Where::Range => run_trailing(ctx, &spec),
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Where {
    Trailing,
    Leading,
    Range,
}

struct JoincontSpec {
    anycase: bool,
    not: bool,
    where_: Where,
    range: Option<InputRange>,
    /// Accepted with TRAILING/RANGE; ignored with LEADING (p.402).
    delay: bool,
    anyof: bool,
    needle: Vec<u8>,
    keep: bool,
    insert: Vec<u8>,
}

impl JoincontSpec {
    fn retain_marker(&self) -> bool {
        // Deleted only when RANGE, NOT, and KEEP are all omitted (p.402).
        self.keep || self.not || self.where_ == Where::Range
    }
}

fn parse_joincont(args: &[String]) -> Result<JoincontSpec, String> {
    let mut i = 0;

    // [ANYCase] — min 4; ANYOF wins at 3 (Note 1).
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    // [NOT]
    let mut not = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("NOT") {
        not = true;
        i += 1;
    }

    // [TRAILING | RANGE inputRange | LEADING] — whole words; TRAILING is default.
    let mut where_ = Where::Trailing;
    let mut range = None;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if upper == "TRAILING" {
            where_ = Where::Trailing;
            i += 1;
        } else if upper == "LEADING" {
            where_ = Where::Leading;
            i += 1;
        } else if upper == "RANGE" {
            where_ = Where::Range;
            i += 1;
            range = Some(parse_input_range(args, &mut i)?);
        }
    }

    // [DELAY] — ignored with LEADING.
    let mut delay = false;
    if i < args.len() && args[i].eq_ignore_ascii_case("DELAY") {
        delay = true;
        i += 1;
    }

    // [ANYof] — min 3.
    let mut anyof = false;
    if i < args.len() {
        let upper = args[i].to_ascii_uppercase();
        if matches_abbrev(&upper, "ANYOF", 3) {
            anyof = true;
            i += 1;
        }
    }

    if i >= args.len() {
        return Err("requires a delimited continuation string".into());
    }
    if !pipe_parser::is_dstring(&args[i]) {
        return Err(unexpected_operand(&args[i]));
    }
    let needle = decode_dstring(&args[i])?;
    i += 1;

    // [KEEP] — RANGE implies KEEP.
    let mut keep = where_ == Where::Range;
    if i < args.len() && args[i].eq_ignore_ascii_case("KEEP") {
        keep = true;
        i += 1;
    }

    // [delimitedString] insert between joined records.
    let mut insert = Vec::new();
    if i < args.len() {
        if pipe_parser::is_dstring(&args[i]) {
            insert = decode_dstring(&args[i])?;
            i += 1;
        } else {
            return Err(unexpected_operand(&args[i]));
        }
    }

    if i < args.len() {
        return Err(unexpected_operand(&args[i]));
    }

    let _ = delay; // parsed; timing is inherent in read-then-write (see architecture.md).
    Ok(JoincontSpec {
        anycase,
        not,
        where_,
        range,
        delay,
        anyof,
        needle,
        keep,
        insert,
    })
}

fn unexpected_operand(arg: &str) -> String {
    let upper = arg.to_ascii_uppercase();
    if is_anycase(arg)
        || upper == "NOT"
        || upper == "TRAILING"
        || upper == "LEADING"
        || upper == "RANGE"
        || upper == "DELAY"
        || matches_abbrev(&upper, "ANYOF", 3)
        || upper == "KEEP"
    {
        format!("{upper} must appear in diagram order")
    } else {
        format!("unexpected operand {arg}")
    }
}

fn run_trailing(ctx: &mut StageCtx, spec: &JoincontSpec) {
    // DELAY (p.402): consume the last input of a set before writing. `read`
    // already takes the record off the connection, so DELAY does not change
    // output records; it is still a valid operand (ignored with LEADING).
    let _ = spec.delay;
    let mut buf: Option<Vec<u8>> = None;
    loop {
        let rec = match ctx.read() {
            Ok(Some(r)) => r,
            Ok(None) => {
                if let Some(b) = buf.take() {
                    let _ = emit(ctx, b);
                }
                return;
            }
            Err(_) => {
                if let Some(b) = buf.take() {
                    let _ = emit(ctx, b);
                }
                return;
            }
        };

        let cont = continues(rec.as_bytes(), spec);
        match buf.as_mut() {
            None => {
                if !cont {
                    if !emit(ctx, rec.into_bytes()) {
                        return;
                    }
                } else {
                    let mut b = rec.into_bytes();
                    if !spec.retain_marker() {
                        strip_marker(&mut b, spec);
                    }
                    buf = Some(b);
                }
            }
            Some(b) => {
                // Insert only between two records (p.402); never after EOF.
                b.extend_from_slice(&spec.insert);
                if cont {
                    let mut piece = rec.into_bytes();
                    if !spec.retain_marker() {
                        strip_marker(&mut piece, spec);
                    }
                    b.extend_from_slice(&piece);
                } else {
                    b.extend_from_slice(rec.as_bytes());
                    let out = buf.take().unwrap();
                    if !emit(ctx, out) {
                        return;
                    }
                }
            }
        }
    }
}

fn run_leading(ctx: &mut StageCtx, spec: &JoincontSpec) {
    let mut buf = match ctx.read() {
        Ok(Some(r)) => r.into_bytes(),
        Ok(None) | Err(_) => return,
    };

    loop {
        let next = match ctx.read() {
            Ok(Some(r)) => r,
            Ok(None) => {
                let _ = emit(ctx, buf);
                return;
            }
            Err(_) => {
                let _ = emit(ctx, buf);
                return;
            }
        };

        if continues(next.as_bytes(), spec) {
            buf.extend_from_slice(&spec.insert);
            let mut piece = next.into_bytes();
            if !spec.retain_marker() {
                strip_marker(&mut piece, spec);
            }
            buf.extend_from_slice(&piece);
        } else {
            if !emit(ctx, buf) {
                return;
            }
            buf = next.into_bytes();
        }
    }
}

fn continues(bytes: &[u8], spec: &JoincontSpec) -> bool {
    let hit = match spec.where_ {
        Where::Trailing => trailing_hit(bytes, spec),
        Where::Leading => leading_hit(bytes, spec),
        Where::Range => range_hit(bytes, spec),
    };
    if spec.not {
        !hit
    } else {
        hit
    }
}

fn trailing_hit(bytes: &[u8], spec: &JoincontSpec) -> bool {
    if spec.anyof {
        match bytes.last() {
            Some(&b) => char_in_set(b, &spec.needle, spec.anycase),
            None => false,
        }
    } else {
        suffix_eq(bytes, &spec.needle, spec.anycase)
    }
}

fn leading_hit(bytes: &[u8], spec: &JoincontSpec) -> bool {
    if spec.anyof {
        match bytes.first() {
            Some(&b) => char_in_set(b, &spec.needle, spec.anycase),
            None => false,
        }
    } else {
        prefix_eq(bytes, &spec.needle, spec.anycase)
    }
}

fn range_hit(bytes: &[u8], spec: &JoincontSpec) -> bool {
    let Some(range) = spec.range.as_ref() else {
        return false;
    };
    let contents = extract_input_range(bytes, range);
    if spec.anyof {
        // A single character of the range is compared against the set (p.402).
        match contents.as_slice() {
            [b] => char_in_set(*b, &spec.needle, spec.anycase),
            _ => false,
        }
    } else {
        bytes_eq(&contents, &spec.needle, spec.anycase)
    }
}

fn strip_marker(bytes: &mut Vec<u8>, spec: &JoincontSpec) {
    match spec.where_ {
        Where::Trailing => {
            let n = if spec.anyof { 1 } else { spec.needle.len() };
            if bytes.len() >= n {
                bytes.truncate(bytes.len() - n);
            }
        }
        Where::Leading => {
            let n = if spec.anyof { 1 } else { spec.needle.len() };
            if bytes.len() >= n {
                bytes.drain(..n);
            }
        }
        Where::Range => {}
    }
}

fn suffix_eq(bytes: &[u8], needle: &[u8], anycase: bool) -> bool {
    if needle.len() > bytes.len() {
        return false;
    }
    bytes_eq(&bytes[bytes.len() - needle.len()..], needle, anycase)
}

fn prefix_eq(bytes: &[u8], needle: &[u8], anycase: bool) -> bool {
    if needle.len() > bytes.len() {
        return false;
    }
    bytes_eq(&bytes[..needle.len()], needle, anycase)
}

fn bytes_eq(a: &[u8], b: &[u8], anycase: bool) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if anycase {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn char_in_set(b: u8, set: &[u8], anycase: bool) -> bool {
    if anycase {
        let u = b.to_ascii_uppercase();
        set.iter().any(|&c| c.to_ascii_uppercase() == u)
    } else {
        set.contains(&b)
    }
}

fn emit(ctx: &mut StageCtx, bytes: Vec<u8>) -> bool {
    ctx.select_output(StreamId::PRIMARY);
    match ctx.write(Record::from(bytes)) {
        Ok(()) => ctx.output_connected(StreamId::PRIMARY),
        Err(_) => false,
    }
}
