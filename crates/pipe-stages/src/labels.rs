//! FRLABEL / TOLABEL / WHILELABEL and their STR forms.
//!
//! Author's Edition 1.1.11 pp.353–354 (FRLABEL), 559–560 (TOLABEL),
//! 596–597 (WHILELABEL), and the STRxxxxx family p.534.
//!
//! Plain verbs (diagrams all-caps; no prefix):
//! `FRLABEL [string]`  (synonym `FROMLABEL`)
//! `TOLABEL [string]`
//! `WHILELABEL [string]`
//!
//! The string starts after exactly one blank; leading and trailing blanks
//! are significant. It is **not** a delimitedString. Matching is a prefix
//! compare (not XEDIT FIND don't-care / underscore): a null string matches
//! any record; a record shorter than the string does not match.
//!
//! STR forms take `[ANYcase] [EXCLUSIVe | INCLUSIVe] delimitedString`
//! (EXCLUSIVE/INCLUSIVE only on STRFRLABEL / STRTOLABEL, p.353 / p.559).
//! `STRWHILElabel` (min 8) is `[ANYcase] delimitedString`.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{decode_dstring, is_anycase, is_delimited, matches_abbrev};

#[derive(Clone, Copy)]
enum Kind {
    /// Discard until the first prefix match, then pass the rest (p.353).
    Fr,
    /// Pass until the first prefix match, then the rest (p.559).
    To,
    /// Pass while records match, then the rest (p.596).
    While,
}

struct Spec {
    verb: &'static str,
    kind: Kind,
    pattern: Vec<u8>,
    anycase: bool,
    /// FRLABEL default true (p.353); TOLABEL default false (p.559).
    inclusive: bool,
}

pub fn frlabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    plain("FRLABEL", Kind::Fr, args, true)
}

pub fn tolabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    plain("TOLABEL", Kind::To, args, false)
}

pub fn whilelabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    plain("WHILELABEL", Kind::While, args, true)
}

pub fn strfrlabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    str_form("STRFRLABEL", Kind::Fr, args, true, true)
}

pub fn strtolabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    str_form("STRTOLABEL", Kind::To, args, true, false)
}

pub fn strwhilelabel(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    str_form("STRWHILELABEL", Kind::While, args, false, true)
}

fn plain(
    verb: &'static str,
    kind: Kind,
    args: Vec<String>,
    inclusive: bool,
) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        run(
            ctx,
            Spec {
                verb,
                kind,
                pattern: remainder_pattern(&args),
                anycase: false,
                inclusive,
            },
        );
    })
}

fn str_form(
    verb: &'static str,
    kind: Kind,
    args: Vec<String>,
    allow_incl: bool,
    default_inclusive: bool,
) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(
        move |ctx| match parse_str(&args, allow_incl, default_inclusive) {
            Ok((pattern, anycase, inclusive)) => run(
                ctx,
                Spec {
                    verb,
                    kind,
                    pattern,
                    anycase,
                    inclusive,
                },
            ),
            Err(e) => ctx.fail(format!("{verb}: {e}")),
        },
    )
}

fn remainder_pattern(args: &[String]) -> Vec<u8> {
    match args {
        [] => Vec::new(),
        [one] => one.as_bytes().to_vec(),
        many => many.join(" ").into_bytes(),
    }
}

fn parse_str(
    args: &[String],
    allow_incl: bool,
    default_inclusive: bool,
) -> Result<(Vec<u8>, bool, bool), String> {
    let mut i = 0;
    let mut anycase = false;
    let mut inclusive = default_inclusive;

    if i < args.len() && is_anycase(&args[i]) {
        anycase = true;
        i += 1;
    }
    if allow_incl && i < args.len() {
        if is_exclusive(&args[i]) {
            inclusive = false;
            i += 1;
        } else if is_inclusive(&args[i]) {
            inclusive = true;
            i += 1;
        }
    }
    if i >= args.len() {
        return Err("a delimited string is required".into());
    }
    if !is_delimited(&args[i]) {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    let pattern = decode_dstring(&args[i])?;
    i += 1;
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }
    Ok((pattern, anycase, inclusive))
}

/// `EXCLUSIVe` (p.353 / p.559): min 8.
fn is_exclusive(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "EXCLUSIVE", 8)
}

/// `INCLUSIVe` (p.353 / p.559): min 8.
fn is_inclusive(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "INCLUSIVE", 8)
}

fn run(ctx: &mut StageCtx, spec: Spec) {
    if ctx.input_connected(StreamId::SECONDARY) {
        ctx.fail(format!(
            "{}: secondary input stream must not be connected",
            spec.verb
        ));
        return;
    }
    for stream in ctx.connected_inputs() {
        if stream.0 > StreamId::SECONDARY.0 {
            ctx.fail(format!(
                "{}: no input stream other than the primary may be connected",
                spec.verb
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
    match spec.kind {
        Kind::Fr => run_fr(ctx, &spec),
        Kind::To => run_to(ctx, &spec),
        Kind::While => run_while(ctx, &spec),
    }
}

fn outputs_live(ctx: &StageCtx) -> bool {
    ctx.output_connected(StreamId::PRIMARY) || ctx.output_connected(StreamId::SECONDARY)
}

/// Prefix compare (p.353 / p.559 / p.596). Null string matches any record.
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

fn run_fr(ctx: &mut StageCtx, spec: &Spec) {
    loop {
        if !outputs_live(ctx) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if begins_with(rec.as_bytes(), &spec.pattern, spec.anycase) {
                sever_if_connected(ctx, StreamId::SECONDARY);
                if !spec.inclusive {
                    let _ = ctx.read();
                }
                copy_remaining(ctx, StreamId::PRIMARY);
                return;
            }
            if !write_or_discard(ctx, rec, StreamId::SECONDARY) {
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

fn run_to(ctx: &mut StageCtx, spec: &Spec) {
    loop {
        if !outputs_live(ctx) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if begins_with(rec.as_bytes(), &spec.pattern, spec.anycase) {
                if spec.inclusive && !write_or_discard(ctx, rec, StreamId::PRIMARY) {
                    return;
                }
                sever_if_connected(ctx, StreamId::PRIMARY);
                // Exclusive default: the match is peeked, not consumed (p.559).
                copy_or_leave_unread(ctx, StreamId::SECONDARY);
                return;
            }
            if !write_or_discard(ctx, rec, StreamId::PRIMARY) {
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

fn run_while(ctx: &mut StageCtx, spec: &Spec) {
    loop {
        if !outputs_live(ctx) {
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            if begins_with(rec.as_bytes(), &spec.pattern, spec.anycase) {
                if !write_or_discard(ctx, rec, StreamId::PRIMARY) {
                    return;
                }
                continue;
            }
            sever_if_connected(ctx, StreamId::PRIMARY);
            // First non-match is peeked, not consumed (p.596; same shape as
            // TOLABEL's leftover when secondary is not defined).
            copy_or_leave_unread(ctx, StreamId::SECONDARY);
            return;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return;
        }
    }
}

fn sever_if_connected(ctx: &mut StageCtx, stream: StreamId) {
    if ctx.output_connected(stream) {
        ctx.select_output(stream);
        ctx.sever_output();
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

/// When `dest` is connected, copy the peeked record and the rest there (IBM
/// SHORT splice stand-in). When it is not, leave the peeked record unread —
/// do not `read()` it. On a CALLPIPE loan the slot stays so the caller can
/// read the leftover after restore. Standalone still `short_input`s so a
/// producer is not parked on the full slot.
fn copy_or_leave_unread(ctx: &mut StageCtx, dest: StreamId) {
    if ctx.output_connected(dest) {
        copy_remaining(ctx, dest);
        return;
    }
    ctx.select_input(StreamId::PRIMARY);
    if ctx.input_is_loaned(StreamId::PRIMARY) {
        return;
    }
    ctx.short_input();
}

/// Copy the current peeked record (if any) and the rest of the file to `dest`.
///
/// IBM shorts input to this output (p.353 / p.559 / p.596). Our `short`
/// severs and does not splice, so this copy is the stand-in. When `dest` is
/// not connected the remaining input is severed unread (TAKE remainder).
fn copy_remaining(ctx: &mut StageCtx, dest: StreamId) {
    ctx.select_input(StreamId::PRIMARY);
    if !ctx.output_connected(dest) {
        ctx.short_input();
        return;
    }
    loop {
        if !ctx.output_connected(dest) {
            ctx.short_input();
            return;
        }
        if ctx.input_record_ready(StreamId::PRIMARY) {
            let Some(rec) = ctx.peek() else {
                continue;
            };
            ctx.select_output(dest);
            let write_failed = ctx.write(rec).is_err();
            let _ = ctx.read();
            if write_failed {
                if ctx.output_connected(dest) {
                    continue;
                }
                ctx.short_input();
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
        assert!(!begins_with(b"", b" ", false));
    }

    #[test]
    fn prefix_is_literal_not_xedit_find() {
        // A blank in the label string is a required blank, not FIND don't-care.
        assert!(begins_with(b"a c", b"a c", false));
        assert!(!begins_with(b"axc", b"a c", false));
        assert!(!begins_with(b"a b", b"a_b", false));
    }

    #[test]
    fn anycase_compares_ascii_letters() {
        assert!(begins_with(b"Abc", b"abc", true));
        assert!(!begins_with(b"Abc", b"abc", false));
        assert!(!begins_with(b"xabc", b"abc", true));
    }

    #[test]
    fn remainder_pattern_joins_nested_tokens() {
        assert_eq!(remainder_pattern(&[]), b"");
        assert_eq!(remainder_pattern(&["abc".into()]), b"abc");
        assert_eq!(remainder_pattern(&["a".into(), "c".into()]), b"a c");
        assert_eq!(remainder_pattern(&[" abc ".into()]), b" abc ");
    }

    #[test]
    fn exclusive_abbreviation_is_eight() {
        assert!(is_exclusive("EXCLUSIVE"));
        assert!(is_exclusive("exclusiv"));
        assert!(!is_exclusive("exclusi"));
        assert!(is_inclusive("INCLUSIVE"));
        assert!(is_inclusive("inclusiv"));
        assert!(!is_inclusive("inclusi"));
    }
}
