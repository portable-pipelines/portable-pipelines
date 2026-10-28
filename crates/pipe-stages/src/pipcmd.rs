//! PIPCMD — Author's Edition 1.1.11 pp.457–458 (pipeline commands Ch.21 pp.638–660).
//!
//! Syntax (diagram is all-caps; no verb prefix):
//! `PIPCMD [ STOP [ ALLEOF | ANYEOF | number ] ]`
//!
//! Control. Reads pipeline commands from the primary input and issues them
//! into the running set. PIPCMD itself writes no records; commands such as
//! OUTPUT and CALLPIPE may write on any selected stream. The command record
//! stays in the input slot until the command completes, then it is consumed
//! unless the command already read it.

use pipe_core::{PipeError, StageCtx, StreamId};

use crate::pipeline_cmd::{self, cmd_abbrev, split_verb};

pub fn pipcmd(args: Vec<String>) -> Box<dyn FnOnce(&mut StageCtx)> {
    Box::new(move |ctx| {
        let stop = match parse_pipcmd(&args) {
            Ok(s) => s,
            Err(e) => {
                ctx.fail(format!("PIPCMD: {e}"));
                return;
            }
        };
        run_pipcmd(ctx, stop);
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    AllEof,
    AnyEof,
    Count(usize),
}

fn parse_pipcmd(args: &[String]) -> Result<Option<Stop>, String> {
    if args.is_empty() {
        return Ok(None);
    }
    if !args[0].eq_ignore_ascii_case("STOP") {
        return Err(format!("unrecognized operand {}", args[0]));
    }
    if args.len() == 1 {
        return Ok(Some(Stop::AllEof));
    }
    let u = args[1].to_ascii_uppercase();
    let stop = if u == "ALLEOF" {
        Stop::AllEof
    } else if u == "ANYEOF" {
        Stop::AnyEof
    } else if let Ok(n) = args[1].parse::<usize>() {
        if n == 0 {
            return Err("STOP count must be at least 1".into());
        }
        Stop::Count(n)
    } else {
        return Err(format!("unrecognized STOP operand {}", args[1]));
    };
    if args.len() > 2 {
        return Err(format!("unrecognized operand {}", args[2]));
    }
    Ok(Some(stop))
}

fn run_pipcmd(ctx: &mut StageCtx, stop: Option<Stop>) {
    ctx.select_input(StreamId::PRIMARY);
    ctx.select_output(StreamId::PRIMARY);

    let initial_outs = ctx.connected_outputs();
    let mut retcode: i32 = 0;

    loop {
        ctx.select_input(StreamId::PRIMARY);
        match wait_command_record(ctx) {
            WaitCmd::Ready => {}
            WaitCmd::Eof => {
                ctx.set_return_code(retcode);
                return;
            }
            WaitCmd::Abort => return,
        }

        let Some(rec) = ctx.peek() else {
            ctx.set_return_code(retcode);
            return;
        };
        let command = rec.to_string_lossy();

        match issue_command(ctx, &command) {
            CmdOutcome::Rc(rc) => {
                consume_command_if_present(ctx, rec.as_bytes());
                if rc < 0 {
                    ctx.set_return_code(rc);
                    ctx.select_input(StreamId::PRIMARY);
                    ctx.short_input();
                    return;
                }
                // SETRC's command RC is 0. The operand is the stage writing.
                // Later nonnegative command RCs still take the maximum.
                let (verb, rest) = split_verb(command.trim_start_matches([' ', '\t']));
                if verb.eq_ignore_ascii_case("SETRC") {
                    if let Some(n) = rest
                        .map(str::trim)
                        .and_then(|w| w.split_whitespace().next())
                        .and_then(|w| w.parse::<i32>().ok())
                    {
                        retcode = n;
                    }
                } else {
                    retcode = retcode.max(rc);
                }
            }
            CmdOutcome::Fail(msg) => {
                consume_command_if_present(ctx, rec.as_bytes());
                if pipe_core::has_stage_context(&msg) || msg.starts_with("PIPCMD:") {
                    ctx.fail(msg);
                } else {
                    ctx.fail(format!("PIPCMD: {msg}"));
                }
                return;
            }
            CmdOutcome::Abort => return,
        }

        if let Some(kind) = stop {
            let eof = initial_outs
                .iter()
                .filter(|s| !ctx.output_connected(**s))
                .count();
            let halt = match kind {
                Stop::AllEof => eof == initial_outs.len(),
                Stop::AnyEof => eof >= 1,
                Stop::Count(n) => eof >= n,
            };
            if halt {
                ctx.set_return_code(retcode);
                ctx.select_input(StreamId::PRIMARY);
                ctx.short_input();
                return;
            }
        }
    }
}

enum WaitCmd {
    Ready,
    Eof,
    Abort,
}

fn wait_command_record(ctx: &mut StageCtx) -> WaitCmd {
    loop {
        if ctx.input_record_ready(StreamId::PRIMARY) {
            return WaitCmd::Ready;
        }
        if ctx.input_at_eof(StreamId::PRIMARY) {
            return WaitCmd::Eof;
        }
        match ctx.wait_any_input(&[StreamId::PRIMARY]) {
            Ok(()) => {
                if ctx.input_record_ready(StreamId::PRIMARY) {
                    return WaitCmd::Ready;
                }
                if ctx.input_at_eof(StreamId::PRIMARY) {
                    return WaitCmd::Eof;
                }
            }
            Err(_) => return WaitCmd::Abort,
        }
    }
}

fn consume_command_if_present(ctx: &mut StageCtx, command: &[u8]) {
    ctx.select_input(StreamId::PRIMARY);
    if !ctx.input_record_ready(StreamId::PRIMARY) {
        return;
    }
    if ctx.peek().is_some_and(|r| r.as_bytes() == command) {
        let _ = ctx.read();
    }
}

enum CmdOutcome {
    Rc(i32),
    Fail(String),
    Abort,
}

fn issue_command(ctx: &mut StageCtx, record: &str) -> CmdOutcome {
    let trimmed = record.trim_start_matches([' ', '\t']);
    if trimmed.is_empty() {
        return CmdOutcome::Rc(-7);
    }
    let (verb, rest) = split_verb(trimmed);
    let upper = verb.to_ascii_uppercase();

    if is_rexx_only(&upper, rest) {
        return CmdOutcome::Fail(format!("{upper} is not available to PIPCMD"));
    }

    match pipeline_cmd::dispatch_command(ctx, &upper, rest) {
        Ok(rc) => CmdOutcome::Rc(rc),
        Err(PipeError::Aborted) => CmdOutcome::Abort,
        Err(PipeError::Severed) => CmdOutcome::Rc(12),
        Err(e) => {
            let s = e.to_string();
            if pipe_core::has_stage_context(&s) {
                CmdOutcome::Fail(s)
            } else {
                CmdOutcome::Fail(format!("PIPCMD: {s}"))
            }
        }
    }
}

fn is_rexx_only(upper: &str, rest: Option<&str>) -> bool {
    matches!(
        upper,
        "BEGOUTPUT" | "GETRANGE" | "NOCOMMIT" | "PEEKTO" | "READTO" | "SCANRANGE" | "SCANSTRING"
    ) || (cmd_abbrev(upper, "STREAMSTATE", 8)
        && rest.is_some_and(|r| {
            r.split_whitespace()
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("ALL"))
        }))
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn no_operands() {
        assert_eq!(parse_pipcmd(&[]).unwrap(), None);
    }

    #[test]
    fn stop_defaults_to_alleof() {
        assert_eq!(parse_pipcmd(&["STOP".into()]).unwrap(), Some(Stop::AllEof));
    }

    #[test]
    fn stop_keywords() {
        assert_eq!(
            parse_pipcmd(&["stop".into(), "anyeof".into()]).unwrap(),
            Some(Stop::AnyEof)
        );
        assert_eq!(
            parse_pipcmd(&["STOP".into(), "ALLEOF".into()]).unwrap(),
            Some(Stop::AllEof)
        );
        assert_eq!(
            parse_pipcmd(&["STOP".into(), "2".into()]).unwrap(),
            Some(Stop::Count(2))
        );
    }

    #[test]
    fn rejects_invented() {
        let e = parse_pipcmd(&["FOREVER".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand FOREVER"), "{e}");
        let e = parse_pipcmd(&["ANYEOF".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand"), "{e}");
        let e = parse_pipcmd(&["STOP".into(), "FOREVER".into()]).unwrap_err();
        assert!(e.contains("unrecognized STOP operand"), "{e}");
        let e = parse_pipcmd(&["STOP".into(), "ALLEOF".into(), "1".into()]).unwrap_err();
        assert!(e.contains("unrecognized operand 1"), "{e}");
        let e = parse_pipcmd(&["STOP".into(), "0".into()]).unwrap_err();
        assert!(e.contains("at least 1"), "{e}");
    }
}
