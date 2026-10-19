//! RUNPIPE — Author's Edition 1.1.11 pp.481–483 (Appendix G events p.798).
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `RUNPIPE [ MSGLevel number | Xhex ] [ TRACE | EVENTS [ MASK hex ] ]`
//!
//! Control. Must not be a first stage. Each primary-input record is a PIPE
//! specification issued as a new pipeline set. Nested pipeline messages
//! (and TRACE, when enabled) are written to the primary output. Null and
//! blank records are discarded. EVENTS / MASK are parsed then rejected
//! (Appendix G is not implemented).

use pipe_core::{aggregate_return_codes, PipeError, Record, StageCtx, StreamId};

use crate::common::matches_abbrev;
use crate::run::run_independent_spec;

pub fn runpipe(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let spec = match parse_runpipe(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("RUNPIPE: {e}"));
                return;
            }
        };
        run_runpipe(ctx, spec);
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RunpipeSpec {
    /// Accepted (p.482) but unused: this port has no PIPMOD message-level bits.
    #[allow(dead_code)]
    msglevel: Option<u32>,
    trace: bool,
}

fn parse_runpipe(args: &[String]) -> Result<RunpipeSpec, String> {
    let mut i = 0;
    let mut msglevel = None;
    if i < args.len() && matches_abbrev(&args[i].to_ascii_uppercase(), "MSGLEVEL", 4) {
        i += 1;
        let val = args
            .get(i)
            .ok_or_else(|| "MSGLEVEL requires a number or Xhex".to_string())?;
        msglevel = Some(parse_msglevel(val)?);
        i += 1;
    }

    let mut trace = false;
    if i < args.len() {
        let u = args[i].to_ascii_uppercase();
        if u == "TRACE" {
            trace = true;
            i += 1;
        } else if u == "EVENTS" {
            i += 1;
            if i < args.len() && args[i].eq_ignore_ascii_case("MASK") {
                i += 1;
                let hex = args
                    .get(i)
                    .ok_or_else(|| "EVENTS MASK requires a hex value".to_string())?;
                parse_events_mask(hex)?;
                i += 1;
            }
            if i < args.len() {
                return Err(format!("unrecognized operand {}", args[i]));
            }
            return Err("EVENTS is not implemented".into());
        } else {
            return Err(format!("unrecognized operand {}", args[i]));
        }
    }
    if i < args.len() {
        return Err(format!("unrecognized operand {}", args[i]));
    }
    Ok(RunpipeSpec { msglevel, trace })
}

fn parse_msglevel(s: &str) -> Result<u32, String> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && (bytes[0] == b'X' || bytes[0] == b'x') {
        let hex = &s[1..];
        if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("invalid MSGLEVEL Xhex {s}"));
        }
        u32::from_str_radix(hex, 16).map_err(|_| format!("invalid MSGLEVEL Xhex {s}"))
    } else if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        s.parse::<u32>()
            .map_err(|_| format!("invalid MSGLEVEL number {s}"))
    } else {
        Err(format!("invalid MSGLEVEL value {s}"))
    }
}

fn parse_events_mask(s: &str) -> Result<u32, String> {
    let hex = if s.len() >= 2 && s.as_bytes()[0].eq_ignore_ascii_case(&b'X') {
        &s[1..]
    } else {
        s
    };
    if hex.is_empty() || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("invalid EVENTS MASK hex {s}"));
    }
    u32::from_str_radix(hex, 16).map_err(|_| format!("invalid EVENTS MASK hex {s}"))
}

fn run_runpipe(ctx: &mut StageCtx, spec: RunpipeSpec) {
    if !ctx.input_connected(StreamId::PRIMARY) {
        ctx.fail("RUNPIPE must not be a first stage");
        return;
    }
    for stream in ctx.connected_inputs() {
        if stream != StreamId::PRIMARY {
            ctx.fail("RUNPIPE: only the primary input may be connected");
            return;
        }
    }

    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    let mut rcs: Vec<i32> = Vec::new();

    loop {
        match wait_spec_record(ctx) {
            WaitSpec::Ready => {}
            WaitSpec::Eof => {
                ctx.set_return_code(aggregate_return_codes(rcs));
                return;
            }
            WaitSpec::Abort => return,
        }

        let Some(rec) = ctx.peek() else {
            ctx.set_return_code(aggregate_return_codes(rcs));
            return;
        };

        if is_null_or_blank(&rec) {
            let _ = ctx.read();
            continue;
        }

        let pipe_spec = rec.to_string_lossy();
        let nested = run_independent_spec(&pipe_spec, spec.trace);
        rcs.push(nested.rc);
        for line in nested.records {
            if write_ignored(ctx, Record::from(line.as_str())) {
                return;
            }
        }

        // p.482: write all output for an input record before consuming it.
        ctx.select_input(StreamId::PRIMARY);
        if ctx.peek().is_some_and(|r| r.as_bytes() == rec.as_bytes()) {
            let _ = ctx.read();
        }
    }
}

enum WaitSpec {
    Ready,
    Eof,
    Abort,
}

fn wait_spec_record(ctx: &mut StageCtx) -> WaitSpec {
    loop {
        if ctx.input_record_ready(StreamId::PRIMARY) {
            return WaitSpec::Ready;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return WaitSpec::Eof;
        }
        match ctx.wait_any_input(&[StreamId::PRIMARY]) {
            Ok(()) => {
                if ctx.input_record_ready(StreamId::PRIMARY) {
                    return WaitSpec::Ready;
                }
                if ctx.input_at_eof(StreamId::PRIMARY) {
                    return WaitSpec::Eof;
                }
            }
            Err(_) => return WaitSpec::Abort,
        }
    }
}

fn is_null_or_blank(rec: &Record) -> bool {
    rec.as_bytes()
        .iter()
        .all(|&b| b == b' ' || b == b'\t' || b == b'\r')
}

/// p.482: ignore write errors (including parent stall/sever) so they cannot
/// leak into the nested pipeline set. `Aborted` means the parent network is
/// coming down; stop issuing further nested specs.
fn write_ignored(ctx: &mut StageCtx, rec: Record) -> bool {
    ctx.select_output(StreamId::PRIMARY);
    match ctx.write(rec) {
        Ok(()) | Err(PipeError::Severed) => false,
        Err(PipeError::Aborted) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn no_operands() {
        assert_eq!(
            parse_runpipe(&[]).unwrap(),
            RunpipeSpec {
                msglevel: None,
                trace: false
            }
        );
    }

    #[test]
    fn msglevel_and_trace() {
        assert_eq!(
            parse_runpipe(&["MSGLEVEL".into(), "4".into()]).unwrap(),
            RunpipeSpec {
                msglevel: Some(4),
                trace: false
            }
        );
        assert_eq!(
            parse_runpipe(&["msgl".into(), "xFF".into(), "TRACE".into()]).unwrap(),
            RunpipeSpec {
                msglevel: Some(0xFF),
                trace: true
            }
        );
        assert_eq!(
            parse_runpipe(&["trace".into()]).unwrap(),
            RunpipeSpec {
                msglevel: None,
                trace: true
            }
        );
    }

    #[test]
    fn rejects_invented_and_wrong_order() {
        let e = parse_runpipe(&["FOREVER".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand FOREVER"), "{e}");
        let e = parse_runpipe(&["TRACE".into(), "MSGLEVEL".into(), "1".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand MSGLEVEL"), "{e}");
        let e = parse_runpipe(&["ANYEOF".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand"), "{e}");
        let e = parse_runpipe(&["MASK".into(), "0".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand MASK"), "{e}");
        let e = parse_runpipe(&["MSG".into(), "1".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand MSG"), "{e}");
        let e = parse_runpipe(&["MSGLEVEL".into()]).unwrap_err();
        assert!(e.contains("MSGLEVEL requires"), "{e}");
        let e = parse_runpipe(&["MSGLEVEL".into(), "-1".into()]).unwrap_err();
        assert!(e.contains("invalid MSGLEVEL"), "{e}");
        let e = parse_runpipe(&["MSGLEVEL".into(), "X".into()]).unwrap_err();
        assert!(e.contains("invalid MSGLEVEL"), "{e}");
    }

    #[test]
    fn events_is_documented_but_unimplemented() {
        let e = parse_runpipe(&["EVENTS".into()]).unwrap_err();
        assert!(e.contains("EVENTS is not implemented"), "{e}");
        let e = parse_runpipe(&["events".into(), "MASK".into(), "00FF".into()]).unwrap_err();
        assert!(e.contains("EVENTS is not implemented"), "{e}");
        let e = parse_runpipe(&["EVENTS".into(), "MASK".into()]).unwrap_err();
        assert!(e.contains("MASK requires a hex"), "{e}");
        let e = parse_runpipe(&["EVENTS".into(), "MASK".into(), "zz".into()]).unwrap_err();
        assert!(e.contains("invalid EVENTS MASK"), "{e}");
        let e = parse_runpipe(&["EVENTS".into(), "FOREVER".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand FOREVER"), "{e}");
    }
}
