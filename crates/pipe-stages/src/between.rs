//! BETWEEN / INSIDE / NOTINSIDE / OUTSIDE — label-group selection.
//!
//! Author's Edition 1.1.11 pp.272–273 (BETWEEN), p.393 (INSIDE),
//! pp.442–443 (NOTINSIDE), pp.445–446 (OUTSIDE).
//!
//! Syntax (fixed railroad order):
//! `BETWEEN [ANYcase] dstring (number | dstring)`  — all-caps verb
//! `INSIDE [ANYcase] dstring (number | dstring)`   — all-caps verb
//! `NOTINSIDe [ANYcase] dstring (number | dstring)` — min 7; synonym `NINSIDE`
//! `OUTSIDE [ANYcase] dstring (number | dstring)`  — all-caps verb
//!
//! Match is a prefix compare (“begins with”), not substring contains.
//! BETWEEN/OUTSIDE include the start and end label records; the count must
//! be 2 or larger. INSIDE/NOTINSIDE exclude the labels; the count must be
//! non-negative. Selected groups go to primary for BETWEEN/INSIDE and to
//! secondary for the converses (OUTSIDE / NOTINSIDE).

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase, is_delimited};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Inclusive groups → primary (p.272).
    Between,
    /// Inclusive groups → secondary; converse of BETWEEN (p.445).
    Outside,
    /// Exclusive groups → primary (p.393).
    Inside,
    /// Exclusive groups → secondary; converse of INSIDE (p.442).
    NotInside,
}

impl Kind {
    fn verb(self) -> &'static str {
        match self {
            Kind::Between => "BETWEEN",
            Kind::Outside => "OUTSIDE",
            Kind::Inside => "INSIDE",
            Kind::NotInside => "NOTINSIDE",
        }
    }

    fn inclusive(self) -> bool {
        matches!(self, Kind::Between | Kind::Outside)
    }

    fn selected_is_primary(self) -> bool {
        matches!(self, Kind::Between | Kind::Inside)
    }

    /// BETWEEN/OUTSIDE: 2 or larger (p.272 / p.445). INSIDE/NOTINSIDE: ≥ 0 (p.393 / p.442).
    fn min_count(self) -> u64 {
        if self.inclusive() {
            2
        } else {
            0
        }
    }
}

#[derive(Debug)]
enum End {
    Pattern(Vec<u8>),
    Count(u64),
}

#[derive(Debug)]
struct Spec {
    kind: Kind,
    start: Vec<u8>,
    end: End,
    anycase: bool,
}

pub fn between(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    stage(Kind::Between, args)
}

pub fn outside(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    stage(Kind::Outside, args)
}

pub fn inside(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    stage(Kind::Inside, args)
}

pub fn notinside(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    stage(Kind::NotInside, args)
}

fn stage(kind: Kind, args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| match parse_spec(kind, &args) {
        Ok(spec) => run(ctx, spec),
        Err(e) => ctx.fail(format!("{}: {e}", kind.verb())),
    })
}

fn parse_spec(kind: Kind, args: &[String]) -> Result<Spec, String> {
    if args.is_empty() {
        return Err("requires two operands".into());
    }

    let mut i = 0;
    let mut anycase = false;
    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }

    if i >= args.len() {
        return Err("requires two operands".into());
    }
    if !is_delimited(&args[i]) {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let start = decode_dstring(&args[i])?;
    i += 1;

    if i >= args.len() {
        return Err("requires two operands".into());
    }

    let end = if is_number_token(&args[i]) {
        let n: u64 = args[i]
            .parse()
            .map_err(|_| format!("unrecognized operand {}", args[i]))?;
        if n < kind.min_count() {
            if kind.inclusive() {
                return Err("count must be at least 2".into());
            }
            return Err("count must be non-negative".into());
        }
        End::Count(n)
    } else if is_delimited(&args[i]) {
        End::Pattern(decode_dstring(&args[i])?)
    } else {
        return Err(format!("unrecognized operand {}", args[i]));
    };
    i += 1;

    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(Spec {
        kind,
        start,
        end,
        anycase,
    })
}

fn is_number_token(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn run(ctx: &mut StageCtx, spec: Spec) {
    if ctx.input_connected(StreamId::SECONDARY) {
        ctx.fail(format!(
            "{}: secondary input stream must not be connected",
            spec.kind.verb()
        ));
        return;
    }
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::SECONDARY.0 {
            ctx.fail(format!(
                "{}: no input stream other than the primary may be connected",
                spec.kind.verb()
            ));
            return;
        }
    }

    if !outputs_live(ctx) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    run_groups(ctx, &spec);
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

/// Prefix compare (p.272 / p.393 / p.442 / p.445). A null string matches any record.
fn begins_with(record: &[u8], pattern: &[u8], anycase: bool) -> bool {
    if pattern.len() > record.len() {
        return false;
    }
    if anycase {
        record[..pattern.len()].eq_ignore_ascii_case(pattern)
    } else {
        record.starts_with(pattern)
    }
}

fn selected_stream(kind: Kind) -> StreamId {
    if kind.selected_is_primary() {
        StreamId::PRIMARY
    } else {
        StreamId::SECONDARY
    }
}

fn other_stream(kind: Kind) -> StreamId {
    if kind.selected_is_primary() {
        StreamId::SECONDARY
    } else {
        StreamId::PRIMARY
    }
}

enum Mode {
    Searching,
    /// `None` = until the next end-pattern record. `Some(n)` = n records still to take.
    Selecting {
        remaining: Option<u64>,
    },
}

fn run_groups(ctx: &mut StageCtx, spec: &Spec) {
    let mut mode = Mode::Searching;
    loop {
        if !outputs_live(ctx) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            let dest = route(spec, &mut mode, rec.as_bytes());
            if !write_or_discard(ctx, rec, dest) {
                return;
            }
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

fn route(spec: &Spec, mode: &mut Mode, bytes: &[u8]) -> StreamId {
    let selected = selected_stream(spec.kind);
    let other = other_stream(spec.kind);
    match mode {
        Mode::Searching => {
            if begins_with(bytes, &spec.start, spec.anycase) {
                enter_group(spec, mode);
                if spec.kind.inclusive() {
                    selected
                } else {
                    other
                }
            } else {
                other
            }
        }
        Mode::Selecting { remaining } => match remaining {
            Some(n) => {
                let left = *n - 1;
                if left == 0 {
                    *mode = Mode::Searching;
                } else {
                    *remaining = Some(left);
                }
                selected
            }
            None => {
                let End::Pattern(end) = &spec.end else {
                    unreachable!("pattern mode has a pattern end");
                };
                if begins_with(bytes, end, spec.anycase) {
                    *mode = Mode::Searching;
                    if spec.kind.inclusive() {
                        selected
                    } else {
                        other
                    }
                } else {
                    selected
                }
            }
        },
    }
}

/// After consuming a start-label record, enter the group (or stay searching
/// for exclusive count 0). The start record itself is never treated as the
/// end of the same group (“the next record”, p.272 / p.393).
fn enter_group(spec: &Spec, mode: &mut Mode) {
    match &spec.end {
        End::Count(n) => {
            if spec.kind.inclusive() {
                // Start record already fills one slot of the count.
                if *n == 1 {
                    *mode = Mode::Searching;
                } else {
                    *mode = Mode::Selecting {
                        remaining: Some(*n - 1),
                    };
                }
            } else if *n == 0 {
                *mode = Mode::Searching;
            } else {
                *mode = Mode::Selecting {
                    remaining: Some(*n),
                };
            }
        }
        End::Pattern(_) => {
            *mode = Mode::Selecting { remaining: None };
        }
    }
}

/// Write `rec` to `stream` if connected, otherwise consume it.
/// Returns false when no output remains.
fn write_or_discard(ctx: &mut StageCtx, rec: Record, stream: StreamId) -> bool {
    if ctx.output_connected(stream) {
        ctx.select_output(stream);
        let write_failed = ctx.write(rec).is_err();
        let _ = ctx.read();
        if write_failed && !outputs_live(ctx) {
            return false;
        }
        true
    } else {
        let _ = ctx.read();
        outputs_live(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_string_matches_any_record() {
        assert!(begins_with(b"", b"", false));
        assert!(begins_with(b"abc", b"", false));
        assert!(begins_with(b"abc", b"", true));
    }

    #[test]
    fn shorter_record_does_not_match() {
        assert!(!begins_with(b"ab", b"abc", false));
        assert!(!begins_with(b"", b"x", false));
    }

    #[test]
    fn prefix_not_contains() {
        assert!(begins_with(b"start here", b"start", false));
        assert!(!begins_with(b"xxstart", b"start", false));
    }

    #[test]
    fn anycase_compares_ascii_letters() {
        assert!(begins_with(b"START", b"start", true));
        assert!(!begins_with(b"START", b"start", false));
    }

    #[test]
    fn number_token_is_unsigned_decimal() {
        assert!(is_number_token("0"));
        assert!(is_number_token("2"));
        assert!(is_number_token("00"));
        assert!(!is_number_token(""));
        assert!(!is_number_token("+2"));
        assert!(!is_number_token("-1"));
        assert!(!is_number_token("2.0"));
        assert!(!is_number_token("*"));
    }

    #[test]
    fn between_rejects_count_below_two() {
        assert!(parse_spec(Kind::Between, &["/x/".into(), "0".into()])
            .unwrap_err()
            .contains("at least 2"));
        assert!(parse_spec(Kind::Outside, &["/x/".into(), "1".into()])
            .unwrap_err()
            .contains("at least 2"));
        assert!(parse_spec(Kind::Inside, &["/x/".into(), "0".into()]).is_ok());
        assert!(parse_spec(Kind::NotInside, &["/x/".into(), "1".into()]).is_ok());
    }

    #[test]
    fn first_operand_must_be_a_dstring() {
        let err = parse_spec(Kind::Between, &["start".into(), "/end/".into()]).unwrap_err();
        assert!(err.contains("unrecognized operand"), "{err}");
        let err = parse_spec(Kind::Inside, &["1-4".into(), "/x/".into()]).unwrap_err();
        assert!(err.contains("unrecognized operand"), "{err}");
    }
}
