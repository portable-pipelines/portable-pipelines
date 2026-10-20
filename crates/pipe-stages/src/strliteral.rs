//! STRLITERAL — Author's Edition 1.1.11 pp.412–413, 534.
//!
//! Syntax (diagram all-caps; no verb prefix):
//! `STRLITERAL [PREFACE | APPEND [CONDitional] | IFEMPTY] [delimitedString]`
//!
//! Default is PREFACE (write the record, then copy input), matching LITERAL.
//! The keywords live on the STRLITERAL railroad only (p.412); LITERAL itself
//! still takes a remainder string after one blank.

use pipe_core::{Record, StageCtx, StreamId};

use crate::common::{copy_selected, decode_dstring, is_delimited, matches_abbrev, write_then_copy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Preface,
    Append,
    IfEmpty,
}

pub fn strliteral(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_strliteral(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("STRLITERAL: {e}"));
                return;
            }
        };
        run_strliteral(ctx, spec);
    })
}

struct Spec {
    mode: Mode,
    conditional: bool,
    record: Record,
}

fn parse_strliteral(args: &[String]) -> Result<Spec, String> {
    let mut i = 0;
    let mut mode = Mode::Preface;
    let mut mode_set = false;
    let mut conditional = false;
    let mut ifempty = false;

    while i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u == "PREFACE" {
            if mode_set || ifempty {
                return Err("PREFACE cannot be combined with APPEND or IFEMPTY".into());
            }
            mode = Mode::Preface;
            mode_set = true;
            i += 1;
        } else if u == "APPEND" {
            if mode_set || ifempty {
                return Err("APPEND cannot be combined with PREFACE or IFEMPTY".into());
            }
            mode = Mode::Append;
            mode_set = true;
            i += 1;
        } else if u == "IFEMPTY" {
            if mode_set || conditional || ifempty {
                return Err(
                    "IFEMPTY cannot be combined with PREFACE, APPEND, or CONDITIONAL".into(),
                );
            }
            ifempty = true;
            mode = Mode::IfEmpty;
            i += 1;
        } else if is_conditional(&args[i]) {
            // p.412 railroad: CONDitional is nested under APPEND only.
            if ifempty {
                return Err("CONDITIONAL cannot be combined with IFEMPTY".into());
            }
            if mode != Mode::Append {
                return Err("CONDITIONAL is valid only with APPEND".into());
            }
            if conditional {
                return Err("CONDITIONAL specified twice".into());
            }
            conditional = true;
            i += 1;
        } else {
            break;
        }
    }

    let record = if i < args.len() {
        if !is_delimited(&args[i]) {
            return Err(format!("unrecognized operand {}", args[i]));
        }
        let bytes = decode_dstring(&args[i])?;
        i += 1;
        Record::from(bytes)
    } else {
        Record::empty()
    };
    if i < args.len() {
        return Err(format!("unexpected operand {}", args[i]));
    }

    Ok(Spec {
        mode,
        conditional,
        record,
    })
}

/// `CONDitional` (p.412): min 4.
fn is_conditional(s: &str) -> bool {
    matches_abbrev(&s.to_ascii_uppercase(), "CONDITIONAL", 4)
}

fn extra_input_connected(ctx: &StageCtx) -> bool {
    ctx.connected_inputs()
        .iter()
        .any(|&s| s != StreamId::PRIMARY)
}

fn run_strliteral(ctx: &mut StageCtx, spec: Spec) {
    if extra_input_connected(ctx) {
        ctx.fail("STRLITERAL: secondary input stream must not be connected");
        return;
    }
    if !ctx.output_connected(StreamId::PRIMARY) {
        ctx.select_input(StreamId::PRIMARY);
        ctx.short_input();
        return;
    }

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    match spec.mode {
        Mode::Preface => {
            if spec.conditional && !input_has_record(ctx) {
                return;
            }
            write_then_copy(ctx, spec.record);
        }
        Mode::Append => {
            let had = copy_counting(ctx);
            if !ctx.output_connected(StreamId::PRIMARY) {
                return;
            }
            if spec.conditional && !had {
                return;
            }
            ctx.select_output(StreamId::PRIMARY);
            let _ = ctx.write(spec.record);
        }
        Mode::IfEmpty => {
            if input_has_record(ctx) {
                copy_selected(ctx);
            } else if ctx.output_connected(StreamId::PRIMARY) {
                ctx.select_output(StreamId::PRIMARY);
                let _ = ctx.write(spec.record);
            }
        }
    }
}

fn input_has_record(ctx: &mut StageCtx) -> bool {
    if !ctx.input_connected(StreamId::PRIMARY) {
        return false;
    }
    ctx.select_input(StreamId::PRIMARY);
    loop {
        if ctx.input_record_ready(StreamId::PRIMARY) {
            return ctx.peek().is_some();
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return false;
        }
        if ctx.wait_any_input(&[StreamId::PRIMARY]).is_err() {
            return false;
        }
    }
}

fn copy_counting(ctx: &mut StageCtx) -> bool {
    let mut any = false;
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);
    while let Ok(Some(r)) = ctx.read() {
        any = true;
        if ctx.write(r).is_err() {
            break;
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_abbreviation_is_four() {
        assert!(is_conditional("CONDITIONAL"));
        assert!(is_conditional("cond"));
        assert!(is_conditional("CONDIT"));
        assert!(!is_conditional("con"));
        assert!(!is_conditional("CON"));
    }

    #[test]
    fn default_is_preface_null_record() {
        let s = parse_strliteral(&[]).unwrap();
        assert_eq!(s.mode, Mode::Preface);
        assert!(!s.conditional);
        assert!(s.record.is_empty());
    }

    #[test]
    fn ifempty_rejects_conditional() {
        assert!(parse_strliteral(&["IFEMPTY".into(), "COND".into(), "/x/".into()]).is_err());
    }

    #[test]
    fn conditional_requires_append() {
        assert!(parse_strliteral(&["COND".into(), "/x/".into()]).is_err());
        assert!(parse_strliteral(&["PREFACE".into(), "COND".into(), "/x/".into()]).is_err());
        let s = parse_strliteral(&["APPEND".into(), "COND".into(), "/x/".into()]).unwrap();
        assert_eq!(s.mode, Mode::Append);
        assert!(s.conditional);
    }
}
