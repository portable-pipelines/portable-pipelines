//! Pipeline commands (Author's Edition Chapter 21, pp.638–660).
//!
//! Shared by PIPCMD and ADDRESS PIPELINE so REXX stages and PIPCMD honour
//! the same verbs and return codes. REXX-only verbs (PEEKTO, READTO,
//! BEGOUTPUT, GETRANGE, NOCOMMIT, SCANRANGE, SCANSTRING, STREAMSTATE ALL)
//! are rejected here; the REXX host handles them.

use pipe_core::{EofReport, PipeError, Record, StageCtx, StreamId};

use crate::common::matches_abbrev;
use crate::registry::StageRegistry;
use crate::run::callpipe_from_spec;

/// Target for commands that only need stage/stream services.
///
/// [`StageCtx`] implements this for PIPCMD and live REXX. The Regina
/// buffered host implements it against a snapshot plus memory buffers.
pub trait CmdTarget {
    fn selected_input(&self) -> StreamId;
    fn selected_output(&self) -> StreamId;
    fn select_input(&mut self, stream: StreamId);
    fn select_output(&mut self, stream: StreamId);
    fn output_connected(&self, stream: StreamId) -> bool;
    fn input_connected(&self, stream: StreamId) -> bool;
    fn input_defined(&self, stream: StreamId) -> bool;
    fn output_defined(&self, stream: StreamId) -> bool;
    fn connected_inputs(&self) -> Vec<StreamId>;
    fn connected_outputs(&self) -> Vec<StreamId>;
    fn add_stream(&mut self, input: bool, output: bool, number: Option<u32>) -> u32;
    fn short(&mut self);
    fn short_input(&mut self);
    fn short_output(&mut self);
    fn commit(&mut self, level: i32) -> Result<(), PipeError>;
    fn yield_others(&mut self) -> Result<(), PipeError>;
    fn issue_message(&mut self, text: &str);
    fn stage_number(&self) -> i32;
    fn write_record(&mut self, rec: Record) -> Result<i32, PipeError>;
    fn set_stage_rc(&mut self, rc: i32);
    fn set_eof_report(&mut self, mode: EofReport);
    fn eof_report(&self) -> EofReport;
    fn set_nocommit(&mut self, yes: bool);
    fn input_record_ready(&self, stream: StreamId) -> bool;
    fn input_at_eof(&self, stream: StreamId) -> bool;
    fn wait_any_input(&mut self, streams: &[StreamId]) -> Result<(), PipeError>;
    fn callpipe(&mut self, spec: &str) -> Result<(), PipeError>;
    fn run_rexx(&mut self, path: &str, args: &[String]) -> Result<(), PipeError>;
}

impl CmdTarget for StageCtx {
    fn selected_input(&self) -> StreamId {
        StageCtx::selected_input(self)
    }
    fn selected_output(&self) -> StreamId {
        StageCtx::selected_output(self)
    }
    fn select_input(&mut self, stream: StreamId) {
        StageCtx::select_input(self, stream);
    }
    fn select_output(&mut self, stream: StreamId) {
        StageCtx::select_output(self, stream);
    }
    fn output_connected(&self, stream: StreamId) -> bool {
        StageCtx::output_connected(self, stream)
    }
    fn input_connected(&self, stream: StreamId) -> bool {
        StageCtx::input_connected(self, stream)
    }
    fn input_defined(&self, stream: StreamId) -> bool {
        StageCtx::input_defined(self, stream)
    }
    fn output_defined(&self, stream: StreamId) -> bool {
        StageCtx::output_defined(self, stream)
    }
    fn connected_inputs(&self) -> Vec<StreamId> {
        StageCtx::connected_inputs(self)
    }
    fn connected_outputs(&self) -> Vec<StreamId> {
        StageCtx::connected_outputs(self)
    }
    fn add_stream(&mut self, input: bool, output: bool, number: Option<u32>) -> u32 {
        StageCtx::add_stream(self, input, output, number)
    }
    fn short(&mut self) {
        StageCtx::short(self);
    }
    fn short_input(&mut self) {
        StageCtx::short_input(self);
    }
    fn short_output(&mut self) {
        StageCtx::short_output(self);
    }
    fn commit(&mut self, level: i32) -> Result<(), PipeError> {
        StageCtx::commit(self, level)
    }
    fn yield_others(&mut self) -> Result<(), PipeError> {
        StageCtx::yield_others(self)
    }
    fn issue_message(&mut self, text: &str) {
        StageCtx::issue_pipeline_message(self, text);
    }
    fn stage_number(&self) -> i32 {
        self.stage_id().0 as i32 + 1
    }
    fn write_record(&mut self, rec: Record) -> Result<i32, PipeError> {
        if !self.output_connected(self.selected_output()) {
            return Ok(12);
        }
        match self.write(rec) {
            Ok(()) => Ok(0),
            Err(PipeError::Severed) => Ok(self.severed_rc()),
            Err(e) => Err(e),
        }
    }
    fn set_stage_rc(&mut self, rc: i32) {
        self.set_return_code(rc);
    }
    fn set_eof_report(&mut self, mode: EofReport) {
        StageCtx::set_eof_report(self, mode);
    }
    fn eof_report(&self) -> EofReport {
        StageCtx::eof_report(self)
    }
    fn set_nocommit(&mut self, yes: bool) {
        StageCtx::set_nocommit(self, yes);
    }
    fn input_record_ready(&self, stream: StreamId) -> bool {
        StageCtx::input_record_ready(self, stream)
    }
    fn input_at_eof(&self, stream: StreamId) -> bool {
        StageCtx::input_at_eof(self, stream)
    }
    fn wait_any_input(&mut self, streams: &[StreamId]) -> Result<(), PipeError> {
        StageCtx::wait_any_input(self, streams)
    }
    fn callpipe(&mut self, spec: &str) -> Result<(), PipeError> {
        callpipe_from_spec(self, spec, &StageRegistry::default())
    }
    fn run_rexx(&mut self, path: &str, args: &[String]) -> Result<(), PipeError> {
        // Nested REXX is a CALLPIPE of that filter on the selected streams.
        let mut spec = format!("*: | {path}");
        for a in args {
            spec.push(' ');
            spec.push_str(a);
        }
        spec.push_str(" | *:");
        callpipe_from_spec(self, &spec, &StageRegistry::default())
    }
}

/// Issue one command. RC -7 means the verb is not a pipeline command.
pub fn dispatch_command(
    ctx: &mut dyn CmdTarget,
    verb: &str,
    rest: Option<&str>,
) -> Result<i32, PipeError> {
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
        return Ok(cmd_message(ctx, arg));
    }
    if cmd_abbrev(verb, "ADDSTREAM", 8) {
        return Ok(cmd_addstream(ctx, arg));
    }
    if verb == "EOFREPORT" {
        return Ok(cmd_eofreport(ctx, arg));
    }
    if verb == "ISSUEMSG" {
        return Ok(cmd_issuemsg(ctx, arg));
    }
    if verb == "SETRC" {
        return Ok(cmd_setrc(ctx, arg));
    }
    if verb == "REXX" {
        return cmd_rexx(ctx, arg);
    }
    Ok(-7)
}

pub fn cmd_abbrev(key: &str, full: &str, min_len: usize) -> bool {
    matches_abbrev(key, full, min_len)
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

pub fn split_verb(s: &str) -> (&str, Option<&str>) {
    match s.find(|c: char| c.is_ascii_whitespace()) {
        None => (s, None),
        Some(i) => {
            let verb = &s[..i];
            let rest = &s[i + 1..];
            (verb, Some(rest))
        }
    }
}

fn cmd_output(ctx: &mut dyn CmdTarget, rest: Option<&str>) -> Result<i32, PipeError> {
    let rec = match rest {
        None => Record::empty(),
        Some(s) => Record::from(s),
    };
    ctx.write_record(rec)
}

fn cmd_callpipe(ctx: &mut dyn CmdTarget, spec: &str) -> Result<i32, PipeError> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(PipeError::stage(
            "CALLPIPE requires a pipeline specification",
        ));
    }
    let _parsed = pipe_parser::parse_pipeline(spec).map_err(|e| PipeError::parse(e.to_string()))?;
    ctx.callpipe(spec)?;
    Ok(0)
}

fn cmd_commit(ctx: &mut dyn CmdTarget, arg: &str) -> Result<i32, PipeError> {
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

fn cmd_select(ctx: &mut dyn CmdTarget, arg: &str) -> Result<i32, PipeError> {
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
            if !ctx.input_defined(stream) {
                return Ok(4);
            }
            ctx.select_input(stream);
        }
        SelectSide::Output => {
            if !ctx.output_defined(stream) {
                return Ok(4);
            }
            ctx.select_output(stream);
        }
        SelectSide::Both => {
            if !ctx.input_defined(stream) && !ctx.output_defined(stream) {
                return Ok(4);
            }
            ctx.select_input(stream);
            ctx.select_output(stream);
        }
    }
    Ok(0)
}

#[derive(Clone, Copy)]
pub enum SelectSide {
    Input,
    Output,
    Both,
}

pub fn side_keyword(word: &str) -> Option<SelectSide> {
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

fn select_anyinput(ctx: &mut dyn CmdTarget) -> Result<i32, PipeError> {
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

fn cmd_short(ctx: &mut dyn CmdTarget, arg: &str) -> Result<i32, PipeError> {
    if word_count(arg) != 0 {
        return Ok(-112);
    }
    ctx.short();
    Ok(0)
}

fn cmd_sever(ctx: &mut dyn CmdTarget, arg: &str) -> Result<i32, PipeError> {
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

fn cmd_stagenum(ctx: &dyn CmdTarget, arg: &str) -> i32 {
    if word_count(arg) != 0 {
        return -112;
    }
    ctx.stage_number()
}

fn cmd_streamnum(ctx: &dyn CmdTarget, arg: &str) -> i32 {
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
            SelectSide::Input => ctx.input_defined(stream),
            SelectSide::Output => ctx.output_defined(stream),
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

fn cmd_streamstate(ctx: &dyn CmdTarget, arg: &str) -> i32 {
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
    if w0 == "ALL" {
        // REXX-only: PIPCMD rejects STREAMSTATE ALL before dispatch.
        return -7;
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
            } else if stream.0 == 0 || ctx.input_defined(stream) {
                12
            } else {
                -4
            }
        }
        SelectSide::Output => {
            if ctx.output_connected(stream) {
                0
            } else if stream.0 == 0 || ctx.output_defined(stream) {
                12
            } else {
                -4
            }
        }
        SelectSide::Both => unreachable!(),
    }
}

fn cmd_maxstream(ctx: &dyn CmdTarget, arg: &str) -> i32 {
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

fn cmd_suspend(ctx: &mut dyn CmdTarget, arg: &str) -> Result<i32, PipeError> {
    if word_count(arg) != 0 {
        return Ok(-112);
    }
    ctx.yield_others()?;
    Ok(0)
}

fn cmd_message(ctx: &mut dyn CmdTarget, arg: &str) -> i32 {
    ctx.issue_message(arg);
    0
}

fn cmd_addstream(ctx: &mut dyn CmdTarget, arg: &str) -> i32 {
    let words: Vec<&str> = arg.split_whitespace().collect();
    if words.is_empty() {
        return ctx.add_stream(true, true, None) as i32;
    }
    if let Some(side) = side_keyword(words[0]) {
        if matches!(side, SelectSide::Both) {
            return -164;
        }
        if words.len() > 2 {
            return -112;
        }
        let number = if words.len() == 2 {
            match words[1].parse::<u32>() {
                Ok(n) => Some(n),
                Err(_) => return -178,
            }
        } else {
            None
        };
        let (input, output) = match side {
            SelectSide::Input => (true, false),
            SelectSide::Output => (false, true),
            SelectSide::Both => unreachable!(),
        };
        return ctx.add_stream(input, output, number) as i32;
    }
    if words.len() > 1 {
        return -112;
    }
    match words[0].parse::<u32>() {
        Ok(n) => ctx.add_stream(true, true, Some(n)) as i32,
        Err(_) => -178,
    }
}

fn cmd_eofreport(ctx: &mut dyn CmdTarget, arg: &str) -> i32 {
    let words: Vec<&str> = arg.split_whitespace().collect();
    if words.len() > 1 {
        return -112;
    }
    let mode = if words.is_empty() {
        EofReport::Current
    } else {
        let u = words[0].to_ascii_uppercase();
        if u == "CURRENT" {
            EofReport::Current
        } else if u == "ANY" {
            EofReport::Any
        } else if u == "ALL" {
            EofReport::All
        } else {
            return -4;
        }
    };
    ctx.set_eof_report(mode);
    0
}

fn cmd_issuemsg(ctx: &mut dyn CmdTarget, arg: &str) -> i32 {
    let mut words = arg.split_whitespace();
    let Some(num) = words.next() else {
        return -42;
    };
    if num.parse::<i32>().is_err() {
        return -42;
    }
    // Portable: there is no IBM message repository. The number, optional
    // component, and remaining tokens are written as one message line.
    ctx.issue_message(arg.trim());
    0
}

fn cmd_setrc(ctx: &mut dyn CmdTarget, arg: &str) -> i32 {
    let mut words = arg.split_whitespace();
    let Some(w) = words.next() else {
        return -2147483648;
    };
    if words.next().is_some() {
        return -2147483648;
    }
    let Ok(n) = w.parse::<i32>() else {
        return -2147483648;
    };
    ctx.set_stage_rc(n);
    0
}

fn cmd_rexx(ctx: &mut dyn CmdTarget, arg: &str) -> Result<i32, PipeError> {
    let mut words = arg.split_whitespace();
    let Some(path) = words.next() else {
        return Ok(-42);
    };
    let args: Vec<String> = words.map(|s| s.to_string()).collect();
    ctx.run_rexx(path, &args)?;
    Ok(0)
}
