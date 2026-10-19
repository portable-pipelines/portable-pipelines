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

use pipe_core::{PipeError, Record, StageCtx, StreamId};

use crate::common::matches_abbrev;
use crate::registry::StageRegistry;
use crate::run::callpipe_from_spec;

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
                retcode = retcode.max(rc);
            }
            CmdOutcome::Fail(msg) => {
                consume_command_if_present(ctx, rec.as_bytes());
                ctx.fail(format!("PIPCMD: {msg}"));
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
    if let Some(name) = unimplemented_allowlist(&upper) {
        return CmdOutcome::Fail(format!("{name} is not implemented"));
    }

    match dispatch_command(ctx, &upper, rest) {
        Ok(rc) => CmdOutcome::Rc(rc),
        Err(PipeError::Aborted) => CmdOutcome::Abort,
        Err(PipeError::Severed) => CmdOutcome::Rc(12),
        Err(e) => CmdOutcome::Fail(e.to_string()),
    }
}

fn split_verb(s: &str) -> (&str, Option<&str>) {
    match s.find(|c: char| c.is_ascii_whitespace()) {
        None => (s, None),
        Some(i) => {
            let verb = &s[..i];
            let rest = &s[i + 1..];
            (verb, Some(rest))
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

fn unimplemented_allowlist(upper: &str) -> Option<&'static str> {
    if cmd_abbrev(upper, "ADDSTREAM", 8) {
        Some("ADDSTREAM")
    } else if upper == "EOFREPORT" {
        Some("EOFREPORT")
    } else if upper == "ISSUEMSG" {
        Some("ISSUEMSG")
    } else if upper == "REXX" {
        Some("REXX")
    } else if upper == "SETRC" {
        Some("SETRC")
    } else {
        None
    }
}

fn cmd_abbrev(key: &str, full: &str, min_len: usize) -> bool {
    matches_abbrev(key, full, min_len)
}

fn dispatch_command(ctx: &mut StageCtx, verb: &str, rest: Option<&str>) -> Result<i32, PipeError> {
    let arg = rest.unwrap_or("");
    if verb == "OUTPUT" {
        return cmd_output(ctx, rest);
    }
    if verb == "CALLPIPE" {
        return cmd_callpipe(ctx, arg);
    }
    if verb == "ADDPIPE" {
        return cmd_callpipe(ctx, arg);
    }
    if verb == "COMMIT" {
        return cmd_commit(ctx, arg);
    }
    if verb == "SELECT" {
        return cmd_select(ctx, arg);
    }
    if verb == "SHORT" {
        return cmd_short(ctx, arg);
    }
    if verb == "SEVER" {
        return cmd_sever(ctx, arg);
    }
    if verb == "RESOLVE" {
        return Ok(cmd_resolve(arg));
    }
    if verb == "STAGENUM" {
        return Ok(cmd_stagenum(ctx, arg));
    }
    if cmd_abbrev(verb, "STREAMNUM", 8) {
        return Ok(cmd_streamnum(ctx, arg));
    }
    if cmd_abbrev(verb, "STREAMSTATE", 8) {
        return Ok(cmd_streamstate(ctx, arg));
    }
    if cmd_abbrev(verb, "MAXSTREAM", 8) {
        return Ok(cmd_maxstream(ctx, arg));
    }
    if verb == "SUSPEND" {
        return cmd_suspend(ctx, arg);
    }
    if verb == "MESSAGE" {
        return Ok(cmd_message(arg));
    }
    Ok(-7)
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn cmd_output(ctx: &mut StageCtx, rest: Option<&str>) -> Result<i32, PipeError> {
    if !ctx.output_connected(ctx.selected_output()) {
        return Ok(12);
    }
    let rec = match rest {
        None => Record::empty(),
        Some(s) => Record::from(s),
    };
    match ctx.write(rec) {
        Ok(()) => Ok(0),
        Err(PipeError::Severed) => Ok(12),
        Err(e) => Err(e),
    }
}

fn cmd_callpipe(ctx: &mut StageCtx, spec: &str) -> Result<i32, PipeError> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(PipeError::stage(
            "CALLPIPE requires a pipeline specification",
        ));
    }
    preflight_callpipe(spec)?;
    callpipe_from_spec(ctx, spec, &StageRegistry::default())?;
    Ok(0)
}

fn preflight_callpipe(spec: &str) -> Result<(), PipeError> {
    let parsed = pipe_parser::parse_pipeline(spec).map_err(|e| PipeError::parse(e.to_string()))?;
    let reg = StageRegistry::default();
    for pipe in &parsed.pipelines {
        for s in pipe {
            if s.connector.is_some() || s.verb.is_empty() {
                continue;
            }
            if reg.contains(&s.verb) {
                continue;
            }
            let lower = s.verb.to_ascii_lowercase();
            if lower.ends_with(".rexx")
                || lower.ends_with(".rex")
                || s.verb.eq_ignore_ascii_case("REXX")
            {
                continue;
            }
            return Err(PipeError::parse(format!("unknown stage: {}", s.verb)));
        }
    }
    Ok(())
}

fn cmd_commit(ctx: &mut StageCtx, arg: &str) -> Result<i32, PipeError> {
    let mut words = arg.split_whitespace();
    let Some(w) = words.next() else {
        return Ok(-2147483648);
    };
    if words.next().is_some() {
        return Ok(-2147483648);
    }
    let Ok(level) = w.parse::<i32>() else {
        return Ok(-2147483648);
    };
    ctx.commit(level)?;
    Ok(0)
}

fn cmd_select(ctx: &mut StageCtx, arg: &str) -> Result<i32, PipeError> {
    let words: Vec<&str> = arg.split_whitespace().collect();
    if words.is_empty() {
        return Ok(-169);
    }
    let w0 = words[0].to_ascii_uppercase();
    if cmd_abbrev(&w0, "ANYINPUT", 5) {
        if words.len() > 1 {
            if words.len() == 2 && side_keyword(words[1]).is_some() {
                return Ok(-168);
            }
            return Ok(-112);
        }
        return select_anyinput(ctx);
    }
    let (side, idx) = if let Some(side) = side_keyword(words[0]) {
        if words.len() == 1 {
            return Ok(-169);
        }
        if words.len() > 2 {
            return Ok(-112);
        }
        (side, 1)
    } else {
        if words.len() > 1 {
            return Ok(-112);
        }
        (SelectSide::Both, 0)
    };
    let Ok(n) = words[idx].parse::<u32>() else {
        return Ok(4);
    };
    let stream = StreamId(n);
    match side {
        SelectSide::Input => {
            if !stream_defined_input(ctx, stream) {
                return Ok(4);
            }
            ctx.select_input(stream);
        }
        SelectSide::Output => {
            if !stream_defined_output(ctx, stream) {
                return Ok(4);
            }
            ctx.select_output(stream);
        }
        SelectSide::Both => {
            if !stream_defined_input(ctx, stream) && !stream_defined_output(ctx, stream) {
                return Ok(4);
            }
            ctx.select_input(stream);
            ctx.select_output(stream);
        }
    }
    Ok(0)
}

#[derive(Clone, Copy)]
enum SelectSide {
    Input,
    Output,
    Both,
}

fn side_keyword(word: &str) -> Option<SelectSide> {
    let u = word.to_ascii_uppercase();
    if u == "BOTH" {
        Some(SelectSide::Both)
    } else if cmd_abbrev(&u, "INPUT", 2) {
        Some(SelectSide::Input)
    } else if cmd_abbrev(&u, "OUTPUT", 3) {
        Some(SelectSide::Output)
    } else {
        None
    }
}

fn stream_defined_input(ctx: &StageCtx, stream: StreamId) -> bool {
    stream.0 == 0 || ctx.input_connected(stream)
}

fn stream_defined_output(ctx: &StageCtx, stream: StreamId) -> bool {
    stream.0 == 0 || ctx.output_connected(stream)
}

fn select_anyinput(ctx: &mut StageCtx) -> Result<i32, PipeError> {
    let mut streams = ctx.connected_inputs();
    if streams.is_empty() {
        streams.push(StreamId::PRIMARY);
    }
    if streams.iter().all(|&s| ctx.input_at_eof(s)) {
        return Ok(12);
    }
    if !streams.iter().any(|&s| ctx.input_record_ready(s)) {
        ctx.wait_any_input(&streams)?;
    }
    if let Some(&s) = streams.iter().find(|&&s| ctx.input_record_ready(s)) {
        ctx.select_input(s);
        return Ok(0);
    }
    if streams.iter().all(|&s| ctx.input_at_eof(s)) {
        Ok(12)
    } else {
        Ok(0)
    }
}

fn cmd_short(ctx: &mut StageCtx, arg: &str) -> Result<i32, PipeError> {
    if word_count(arg) != 0 {
        return Ok(-112);
    }
    ctx.short();
    Ok(0)
}

fn cmd_sever(ctx: &mut StageCtx, arg: &str) -> Result<i32, PipeError> {
    let mut words = arg.split_whitespace();
    let Some(w) = words.next() else {
        return Ok(-163);
    };
    if words.next().is_some() {
        return Ok(-112);
    }
    match side_keyword(w) {
        Some(SelectSide::Input) => ctx.short_input(),
        Some(SelectSide::Output) => ctx.short_output(),
        _ => return Ok(-164),
    }
    Ok(0)
}

fn cmd_resolve(arg: &str) -> i32 {
    let mut words = arg.split_whitespace();
    let Some(name) = words.next() else {
        return -42;
    };
    if words.next().is_some() {
        return -112;
    }
    if StageRegistry::default().contains(name) {
        1
    } else {
        0
    }
}

fn cmd_stagenum(ctx: &StageCtx, arg: &str) -> i32 {
    if word_count(arg) != 0 {
        return -112;
    }
    ctx.stage_id().0 as i32 + 1
}

fn cmd_streamnum(ctx: &StageCtx, arg: &str) -> i32 {
    let words: Vec<&str> = arg.split_whitespace().collect();
    if words.is_empty() {
        return -163;
    }
    if words.len() > 2 {
        return -112;
    }
    let Some(side) = side_keyword(words[0]) else {
        return -164;
    };
    if matches!(side, SelectSide::Both) {
        return -164;
    }
    let ident = words.get(1).copied().unwrap_or("*");
    if ident == "*" {
        return match side {
            SelectSide::Input => ctx.selected_input().0 as i32,
            SelectSide::Output => ctx.selected_output().0 as i32,
            SelectSide::Both => unreachable!(),
        };
    }
    if let Ok(n) = ident.parse::<u32>() {
        let stream = StreamId(n);
        let exists = match side {
            SelectSide::Input => stream_defined_input(ctx, stream),
            SelectSide::Output => stream_defined_output(ctx, stream),
            SelectSide::Both => unreachable!(),
        };
        if exists {
            n as i32
        } else {
            -102
        }
    } else {
        -178
    }
}

fn cmd_streamstate(ctx: &StageCtx, arg: &str) -> i32 {
    let words: Vec<&str> = arg.split_whitespace().collect();
    if words.is_empty() {
        return -163;
    }
    if words.len() > 2 {
        return -112;
    }
    let w0 = words[0].to_ascii_uppercase();
    if w0 == "SUMMARY" {
        if words.len() > 1 {
            return -112;
        }
        let in_ok = !ctx.connected_inputs().is_empty();
        let out_ok = !ctx.connected_outputs().is_empty();
        return if in_ok && out_ok { 0 } else { 12 };
    }
    let Some(side) = side_keyword(words[0]) else {
        return -164;
    };
    if matches!(side, SelectSide::Both) {
        return -164;
    }
    let stream = if let Some(ident) = words.get(1) {
        if *ident == "*" {
            match side {
                SelectSide::Input => ctx.selected_input(),
                SelectSide::Output => ctx.selected_output(),
                SelectSide::Both => unreachable!(),
            }
        } else if let Ok(n) = ident.parse::<u32>() {
            StreamId(n)
        } else {
            return -4;
        }
    } else {
        match side {
            SelectSide::Input => ctx.selected_input(),
            SelectSide::Output => ctx.selected_output(),
            SelectSide::Both => unreachable!(),
        }
    };
    match side {
        SelectSide::Input => {
            if ctx.input_connected(stream) {
                0
            } else if stream.0 == 0 {
                12
            } else {
                -4
            }
        }
        SelectSide::Output => {
            if ctx.output_connected(stream) {
                0
            } else if stream.0 == 0 {
                12
            } else {
                -4
            }
        }
        SelectSide::Both => unreachable!(),
    }
}

fn cmd_maxstream(ctx: &StageCtx, arg: &str) -> i32 {
    let mut words = arg.split_whitespace();
    let Some(w) = words.next() else {
        return -163;
    };
    if words.next().is_some() {
        return -112;
    }
    let Some(side) = side_keyword(w) else {
        return -164;
    };
    if matches!(side, SelectSide::Both) {
        return -164;
    }
    let streams = match side {
        SelectSide::Input => ctx.connected_inputs(),
        SelectSide::Output => ctx.connected_outputs(),
        SelectSide::Both => unreachable!(),
    };
    streams.iter().map(|s| s.0).max().unwrap_or(0) as i32
}

fn cmd_suspend(ctx: &mut StageCtx, arg: &str) -> Result<i32, PipeError> {
    if word_count(arg) != 0 {
        return Ok(-112);
    }
    ctx.yield_others()?;
    Ok(0)
}

fn cmd_message(arg: &str) -> i32 {
    eprintln!("{arg}");
    0
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
