use std::collections::{HashMap, HashSet, VecDeque};

use pipe_core::{
    issue_pipeline_message, EofReport, PipeError, PipeResult, Record, StageCtx, StreamId,
};
use pipe_parser::scan_dstring_prefix;
use pipe_stages::pipeline_cmd::{self, CmdTarget};
use pipe_stages::{extract_input_range, locate_input_range, parse_input_range, InputRange};

/// Result of one ADDRESS PIPELINE command, including REXX variable sets.
#[derive(Debug, Default)]
pub struct CmdResult {
    pub rc: i32,
    pub assigns: Vec<(String, String)>,
}

/// Pipeline subcommands exposed to REXX stages (CMS-style).
///
/// *Live* mode talks to `StageCtx` (subset interpreter).
/// *Buffered* mode is used for Regina: drain/fill vectors so `RexxStart`
/// never suspends the dispatcher coroutine (Regina is not re-entrant across
/// interleaved stage coroutines).
pub struct RexxPipeHost<'a> {
    ctx: Option<&'a mut StageCtx>,
    buf_in: VecDeque<String>,
    buf_out: Vec<String>,
    pub last_record: Option<String>,
    pub rc: i32,
    /// Program return code from `EXIT n`, distinct from last-command `RC`.
    pub exit_rc: Option<i32>,
    selected_in: StreamId,
    selected_out: StreamId,
    defined_in: HashSet<u32>,
    defined_out: HashSet<u32>,
    connected_in: HashSet<u32>,
    connected_out: HashSet<u32>,
    input_severed: bool,
    output_severed: bool,
    stage_num: i32,
    eof_report: EofReport,
    nocommit: bool,
    implied_output: bool,
    pub(crate) stage_rc: Option<i32>,
    ranges: HashMap<String, InputRange>,
}

impl<'a> RexxPipeHost<'a> {
    pub fn new(ctx: &'a mut StageCtx) -> Self {
        let selected_in = ctx.selected_input();
        let selected_out = ctx.selected_output();
        let connected_in: HashSet<u32> = ctx.connected_inputs().iter().map(|s| s.0).collect();
        let connected_out: HashSet<u32> = ctx.connected_outputs().iter().map(|s| s.0).collect();
        let stage_num = ctx.stage_id().0 as i32 + 1;
        Self {
            ctx: Some(ctx),
            buf_in: VecDeque::new(),
            buf_out: Vec::new(),
            last_record: None,
            rc: 0,
            exit_rc: None,
            selected_in,
            selected_out,
            defined_in: connected_in.clone(),
            defined_out: connected_out.clone(),
            connected_in,
            connected_out,
            input_severed: false,
            output_severed: false,
            stage_num,
            eof_report: EofReport::Off,
            nocommit: false,
            implied_output: false,
            stage_rc: None,
            ranges: HashMap::new(),
        }
    }

    /// Host that serves I/O from memory (for Regina). Snapshot `from` first
    /// so STAGENUM / STREAMSTATE / MAXSTREAM match the live stage.
    pub fn buffered_from(from: &StageCtx, input: Vec<String>) -> RexxPipeHost<'static> {
        let connected_in: HashSet<u32> = from.connected_inputs().iter().map(|s| s.0).collect();
        let connected_out: HashSet<u32> = from.connected_outputs().iter().map(|s| s.0).collect();
        RexxPipeHost {
            ctx: None,
            buf_in: input.into(),
            buf_out: Vec::new(),
            last_record: None,
            rc: 0,
            exit_rc: None,
            selected_in: from.selected_input(),
            selected_out: from.selected_output(),
            defined_in: connected_in.clone(),
            defined_out: connected_out.clone(),
            connected_in,
            connected_out,
            input_severed: false,
            output_severed: false,
            stage_num: from.stage_id().0 as i32 + 1,
            eof_report: from.eof_report(),
            nocommit: from.nocommit(),
            implied_output: false,
            stage_rc: None,
            ranges: HashMap::new(),
        }
    }

    /// Host that serves `READTO` / `OUTPUT` from memory (for Regina unit tests).
    pub fn buffered(input: Vec<String>) -> RexxPipeHost<'static> {
        let mut connected_in = HashSet::new();
        connected_in.insert(0);
        let mut connected_out = HashSet::new();
        connected_out.insert(0);
        RexxPipeHost {
            ctx: None,
            buf_in: input.into(),
            buf_out: Vec::new(),
            last_record: None,
            rc: 0,
            exit_rc: None,
            selected_in: StreamId::PRIMARY,
            selected_out: StreamId::PRIMARY,
            defined_in: connected_in.clone(),
            defined_out: connected_out.clone(),
            connected_in,
            connected_out,
            input_severed: false,
            output_severed: false,
            stage_num: 1,
            eof_report: EofReport::Off,
            nocommit: false,
            implied_output: false,
            stage_rc: None,
            ranges: HashMap::new(),
        }
    }

    pub fn implied_output(&self) -> bool {
        self.implied_output
    }

    pub(crate) fn input_severed(&self) -> bool {
        self.input_severed
    }

    pub(crate) fn output_severed(&self) -> bool {
        self.output_severed
    }

    pub(crate) fn selected_in_id(&self) -> StreamId {
        self.selected_in
    }

    pub(crate) fn selected_out_id(&self) -> StreamId {
        self.selected_out
    }

    /// Copy `EXIT n` / SETRC onto the stage return code (Author's Edition p.219).
    pub fn apply_stage_exit(&mut self) {
        let rc = self.exit_rc.or(self.stage_rc);
        if let Some(rc) = rc {
            if let Some(ctx) = self.ctx.as_mut() {
                ctx.set_return_code(rc);
            }
        }
    }

    pub fn into_output(self) -> Vec<String> {
        self.buf_out
    }

    /// PEEKTO [varname]: wait for the next input record without consuming it.
    pub fn peekto(&mut self) -> PipeResult<Option<String>> {
        if let Some(ctx) = self.ctx.as_mut() {
            let stream = ctx.selected_input();
            loop {
                if ctx.input_at_eof(stream) {
                    self.rc = 12;
                    self.last_record = None;
                    return Ok(None);
                }
                if let Some(r) = ctx.peek() {
                    let s = r.to_string_lossy();
                    self.last_record = Some(s.clone());
                    self.rc = 0;
                    return Ok(Some(s));
                }
                if !ctx.input_connected(stream) {
                    self.rc = ctx.severed_rc();
                    self.last_record = None;
                    return Ok(None);
                }
                match ctx.wait_any_input(&[stream]) {
                    Ok(()) => {}
                    Err(PipeError::Severed) => {
                        self.rc = ctx.severed_rc();
                        self.last_record = None;
                        return Ok(None);
                    }
                    Err(e) => {
                        self.rc = 8;
                        return Err(e);
                    }
                }
            }
        }
        if self.input_severed {
            self.rc = self.severed_rc_buf();
            self.last_record = None;
            return Ok(None);
        }
        match self.buf_in.front().cloned() {
            Some(s) => {
                self.last_record = Some(s.clone());
                self.rc = 0;
                Ok(Some(s))
            }
            None => {
                self.rc = 12;
                self.last_record = None;
                Ok(None)
            }
        }
    }

    /// READTO [varname]: read one record (stored in last_record).
    pub fn readto(&mut self) -> PipeResult<Option<String>> {
        if let Some(ctx) = self.ctx.as_mut() {
            match ctx.read() {
                Ok(Some(r)) => {
                    let s = r.to_string_lossy();
                    self.last_record = Some(s.clone());
                    self.rc = 0;
                    Ok(Some(s))
                }
                Ok(None) => {
                    self.rc = 12;
                    self.last_record = None;
                    Ok(None)
                }
                Err(PipeError::Severed) => {
                    self.rc = ctx.severed_rc();
                    self.last_record = None;
                    Ok(None)
                }
                Err(e) => {
                    self.rc = 8;
                    Err(e)
                }
            }
        } else {
            if self.input_severed {
                self.rc = self.severed_rc_buf();
                self.last_record = None;
                return Ok(None);
            }
            match self.buf_in.pop_front() {
                Some(s) => {
                    self.last_record = Some(s.clone());
                    self.rc = 0;
                    Ok(Some(s))
                }
                None => {
                    self.rc = 12;
                    self.last_record = None;
                    Ok(None)
                }
            }
        }
    }

    fn severed_rc_buf(&self) -> i32 {
        if matches!(self.eof_report, EofReport::Off) {
            12
        } else {
            8
        }
    }

    /// OUTPUT: write a string as one record (IBM pipeline subcommand, p.648).
    /// The caller supplies the remainder after `OUTPUT` and one blank, exactly.
    pub fn output(&mut self, data: &str) -> PipeResult<()> {
        let rc = self.write_record(Record::from(data))?;
        self.rc = rc;
        Ok(())
    }

    pub fn short(&mut self) {
        CmdTarget::short(self);
        self.rc = 0;
    }

    pub fn select_input(&mut self, n: u32) {
        CmdTarget::select_input(self, StreamId(n));
    }

    pub fn select_output(&mut self, n: u32) {
        CmdTarget::select_output(self, StreamId(n));
    }

    pub fn callpipe(&mut self, spec: &str) -> PipeResult<()> {
        if let Some(ctx) = self.ctx.as_mut() {
            crate::callpipe_spec(ctx, spec)?;
            self.rc = 0;
            return Ok(());
        }
        crate::callpipe_buffered(&mut self.buf_in, &mut self.buf_out, spec)?;
        self.rc = 0;
        Ok(())
    }

    pub fn addpipe(&mut self, spec: &str) -> PipeResult<()> {
        self.callpipe(spec)
    }

    pub fn commit(&mut self, level: i32) -> PipeResult<()> {
        CmdTarget::commit(self, level)?;
        self.rc = 0;
        Ok(())
    }

    pub fn set_rc_error(&mut self, e: &PipeError) {
        self.rc = 8;
        let _ = e;
    }

    /// Dispatch one ADDRESS PIPELINE command string.
    pub fn dispatch(&mut self, cmd: &str) -> Result<CmdResult, String> {
        let cmd = cmd.trim_start_matches([' ', '\t']);
        if cmd.is_empty() {
            self.rc = 0;
            return Ok(CmdResult {
                rc: 0,
                assigns: vec![],
            });
        }
        let (verb, rest) = pipeline_cmd::split_verb(cmd);
        let upper = verb.to_ascii_uppercase();
        let arg = rest.unwrap_or("");

        if upper == "PEEKTO" {
            return self.cmd_peekto(arg);
        }
        if upper == "READTO" {
            return self.cmd_readto(arg);
        }
        if upper == "BEGOUTPUT" {
            return self.cmd_begoutput(arg);
        }
        if upper == "NOCOMMIT" {
            return self.cmd_nocommit(arg);
        }
        if upper == "GETRANGE" {
            return self.cmd_getrange(arg);
        }
        if upper == "SCANRANGE" {
            return self.cmd_scanrange(arg);
        }
        if upper == "SCANSTRING" {
            return self.cmd_scanstring(arg);
        }
        if pipeline_cmd::cmd_abbrev(&upper, "STREAMSTATE", 8) {
            let w0 = arg
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if w0 == "ALL" {
                return self.cmd_streamstate_all(arg);
            }
        }

        match pipeline_cmd::dispatch_command(self, &upper, rest) {
            Ok(rc) => {
                self.rc = rc;
                Ok(CmdResult {
                    rc,
                    assigns: vec![],
                })
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn cmd_peekto(&mut self, arg: &str) -> Result<CmdResult, String> {
        let mut words = arg.split_whitespace();
        let var = words.next().map(|s| s.to_ascii_uppercase());
        if words.next().is_some() {
            self.rc = -112;
            return Ok(CmdResult {
                rc: -112,
                assigns: vec![],
            });
        }
        let rec = self.peekto().map_err(|e| e.to_string())?;
        let mut assigns = Vec::new();
        if let (Some(name), Some(s)) = (var, rec.as_ref()) {
            assigns.push((name, s.clone()));
        }
        Ok(CmdResult {
            rc: self.rc,
            assigns,
        })
    }

    fn cmd_readto(&mut self, arg: &str) -> Result<CmdResult, String> {
        let mut words = arg.split_whitespace();
        let var = words.next().map(|s| s.to_ascii_uppercase());
        if words.next().is_some() {
            self.rc = -112;
            return Ok(CmdResult {
                rc: -112,
                assigns: vec![],
            });
        }
        let rec = self.readto().map_err(|e| e.to_string())?;
        let mut assigns = Vec::new();
        if let (Some(name), Some(s)) = (var, rec.as_ref()) {
            assigns.push((name, s.clone()));
        }
        Ok(CmdResult {
            rc: self.rc,
            assigns,
        })
    }

    fn cmd_begoutput(&mut self, arg: &str) -> Result<CmdResult, String> {
        if arg.split_whitespace().next().is_some() {
            self.rc = -112;
            return Ok(CmdResult {
                rc: -112,
                assigns: vec![],
            });
        }
        self.implied_output = true;
        self.rc = 0;
        Ok(CmdResult {
            rc: 0,
            assigns: vec![],
        })
    }

    fn cmd_nocommit(&mut self, arg: &str) -> Result<CmdResult, String> {
        if arg.split_whitespace().next().is_some() {
            self.rc = -112;
            return Ok(CmdResult {
                rc: -112,
                assigns: vec![],
            });
        }
        self.set_nocommit(true);
        self.rc = 0;
        Ok(CmdResult {
            rc: 0,
            assigns: vec![],
        })
    }

    fn cmd_scanrange(&mut self, arg: &str) -> Result<CmdResult, String> {
        let (name, rest_var, string) = match split_two_then_rest(arg) {
            Some(t) => t,
            None => {
                self.rc = -42;
                return Ok(CmdResult {
                    rc: -42,
                    assigns: vec![],
                });
            }
        };
        let tokens: Vec<String> = string.split_whitespace().map(|s| s.to_string()).collect();
        if tokens.is_empty() {
            self.rc = 8;
            return Ok(CmdResult {
                rc: 8,
                assigns: vec![],
            });
        }
        let mut i = 0usize;
        match parse_input_range(&tokens, &mut i) {
            Ok(range) => {
                let consumed: Vec<&str> = tokens.iter().take(i).map(|s| s.as_str()).collect();
                let desc = consumed.join(" ");
                let remainder = skip_n_words(&string, i);
                self.ranges.insert(name.to_ascii_uppercase(), range.clone());
                self.ranges.insert(desc.to_ascii_uppercase(), range);
                self.rc = 0;
                Ok(CmdResult {
                    rc: 0,
                    assigns: vec![
                        (name.to_ascii_uppercase(), desc),
                        (rest_var.to_ascii_uppercase(), remainder),
                    ],
                })
            }
            Err(_) => {
                self.rc = 8;
                Ok(CmdResult {
                    rc: 8,
                    assigns: vec![],
                })
            }
        }
    }

    fn cmd_scanstring(&mut self, arg: &str) -> Result<CmdResult, String> {
        let (name, rest_var, string) = match split_two_then_rest(arg) {
            Some(t) => t,
            None => {
                self.rc = -42;
                return Ok(CmdResult {
                    rc: -42,
                    assigns: vec![],
                });
            }
        };
        match scan_dstring_prefix(&string) {
            Ok((bytes, rest)) => {
                let value = String::from_utf8_lossy(&bytes).into_owned();
                self.rc = 0;
                Ok(CmdResult {
                    rc: 0,
                    assigns: vec![
                        (name.to_ascii_uppercase(), value),
                        (rest_var.to_ascii_uppercase(), rest),
                    ],
                })
            }
            Err(_) => {
                self.rc = 8;
                Ok(CmdResult {
                    rc: 8,
                    assigns: vec![],
                })
            }
        }
    }

    fn cmd_getrange(&mut self, arg: &str) -> Result<CmdResult, String> {
        let (range_word, dest, string_opt) = match split_two_then_optional_rest(arg) {
            Some(t) => t,
            None => {
                self.rc = -42;
                return Ok(CmdResult {
                    rc: -42,
                    assigns: vec![],
                });
            }
        };
        let source = if let Some(s) = string_opt {
            s
        } else {
            match &self.last_record {
                Some(r) => r.clone(),
                None => {
                    self.rc = 12;
                    return Ok(CmdResult {
                        rc: 12,
                        assigns: vec![],
                    });
                }
            }
        };
        let key = range_word.to_ascii_uppercase();
        let range = if let Some(r) = self.ranges.get(&key) {
            r.clone()
        } else {
            let tokens: Vec<String> = range_word
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            let mut i = 0usize;
            match parse_input_range(&tokens, &mut i) {
                Ok(r) if i == tokens.len() => r,
                _ => {
                    self.rc = 8;
                    return Ok(CmdResult {
                        rc: 8,
                        assigns: vec![],
                    });
                }
            }
        };
        let bytes = source.as_bytes();
        let loc = locate_input_range(bytes, &range);
        let extracted = extract_input_range(bytes, &range);
        let value = String::from_utf8_lossy(&extracted).into_owned();
        self.rc = if loc.present { 0 } else { 8 };
        Ok(CmdResult {
            rc: self.rc,
            assigns: vec![(dest.to_ascii_uppercase(), value)],
        })
    }

    fn cmd_streamstate_all(&mut self, arg: &str) -> Result<CmdResult, String> {
        let words: Vec<&str> = arg.split_whitespace().collect();
        if words.len() != 2 {
            self.rc = -112;
            return Ok(CmdResult {
                rc: -112,
                assigns: vec![],
            });
        }
        let stem = words[1].to_ascii_uppercase();
        let mut lines = Vec::new();
        let mut in_ids: Vec<u32> = self.defined_in.union(&self.connected_in).copied().collect();
        if !in_ids.contains(&0) {
            in_ids.push(0);
        }
        in_ids.sort_unstable();
        in_ids.dedup();
        for n in in_ids {
            let rc = streamstate_rc(
                self.connected_in.contains(&n),
                n == 0 || self.defined_in.contains(&n),
            );
            lines.push(format!("INPUT {n} {rc}"));
        }
        let mut out_ids: Vec<u32> = self
            .defined_out
            .union(&self.connected_out)
            .copied()
            .collect();
        if !out_ids.contains(&0) {
            out_ids.push(0);
        }
        out_ids.sort_unstable();
        out_ids.dedup();
        for n in out_ids {
            let rc = streamstate_rc(
                self.connected_out.contains(&n),
                n == 0 || self.defined_out.contains(&n),
            );
            lines.push(format!("OUTPUT {n} {rc}"));
        }
        let mut assigns = vec![(format!("{stem}.0"), lines.len().to_string())];
        for (i, line) in lines.iter().enumerate() {
            assigns.push((format!("{stem}.{}", i + 1), line.clone()));
        }
        self.rc = 0;
        Ok(CmdResult { rc: 0, assigns })
    }
}

fn streamstate_rc(connected: bool, defined: bool) -> i32 {
    if connected {
        0
    } else if defined {
        12
    } else {
        -4
    }
}

fn split_two_then_rest(arg: &str) -> Option<(String, String, String)> {
    let t = arg.trim_start();
    let mut parts = t.splitn(3, char::is_whitespace);
    let a = parts.next()?.to_string();
    let rest = t[a.len()..].trim_start();
    if rest.is_empty() {
        return None;
    }
    let mut parts2 = rest.splitn(2, char::is_whitespace);
    let b = parts2.next()?.to_string();
    let string = rest[b.len()..].trim_start().to_string();
    if a.is_empty() || b.is_empty() {
        return None;
    }
    Some((a, b, string))
}

fn split_two_then_optional_rest(arg: &str) -> Option<(String, String, Option<String>)> {
    let t = arg.trim_start();
    let mut parts = t.splitn(3, char::is_whitespace);
    let a = parts.next()?.to_string();
    let rest = t[a.len()..].trim_start();
    if rest.is_empty() {
        return None;
    }
    let mut parts2 = rest.splitn(2, char::is_whitespace);
    let b = parts2.next()?.to_string();
    let rem = rest[b.len()..].trim_start();
    let string = if rem.is_empty() {
        None
    } else {
        Some(rem.to_string())
    };
    Some((a, b, string))
}

fn skip_n_words(s: &str, n: usize) -> String {
    let mut rest = s.trim_start();
    for _ in 0..n {
        if let Some(i) = rest.find(char::is_whitespace) {
            rest = rest[i..].trim_start();
        } else {
            return String::new();
        }
    }
    rest.to_string()
}

impl CmdTarget for RexxPipeHost<'_> {
    fn selected_input(&self) -> StreamId {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.selected_input()
        } else {
            self.selected_in
        }
    }
    fn selected_output(&self) -> StreamId {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.selected_output()
        } else {
            self.selected_out
        }
    }
    fn select_input(&mut self, stream: StreamId) {
        self.selected_in = stream;
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.select_input(stream);
        }
    }
    fn select_output(&mut self, stream: StreamId) {
        self.selected_out = stream;
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.select_output(stream);
        }
    }
    fn output_connected(&self, stream: StreamId) -> bool {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.output_connected(stream)
        } else {
            !self.output_severed && self.connected_out.contains(&stream.0)
        }
    }
    fn input_connected(&self, stream: StreamId) -> bool {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.input_connected(stream)
        } else {
            !self.input_severed && self.connected_in.contains(&stream.0)
        }
    }
    fn input_defined(&self, stream: StreamId) -> bool {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.input_defined(stream)
        } else {
            stream.0 == 0
                || self.defined_in.contains(&stream.0)
                || self.connected_in.contains(&stream.0)
        }
    }
    fn output_defined(&self, stream: StreamId) -> bool {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.output_defined(stream)
        } else {
            stream.0 == 0
                || self.defined_out.contains(&stream.0)
                || self.connected_out.contains(&stream.0)
        }
    }
    fn connected_inputs(&self) -> Vec<StreamId> {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.connected_inputs()
        } else {
            let mut v: Vec<StreamId> = self
                .connected_in
                .iter()
                .filter(|n| !self.input_severed || **n != self.selected_in.0)
                .map(|n| StreamId(*n))
                .collect();
            if self.input_severed {
                v.retain(|s| s.0 != self.selected_in.0);
            }
            v.sort_by_key(|s| s.0);
            v
        }
    }
    fn connected_outputs(&self) -> Vec<StreamId> {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.connected_outputs()
        } else {
            let mut v: Vec<StreamId> = self.connected_out.iter().map(|n| StreamId(*n)).collect();
            if self.output_severed {
                v.retain(|s| s.0 != self.selected_out.0);
            }
            v.sort_by_key(|s| s.0);
            v
        }
    }
    fn add_stream(&mut self, input: bool, output: bool, number: Option<u32>) -> u32 {
        if let Some(ctx) = self.ctx.as_mut() {
            return ctx.add_stream(input, output, number);
        }
        let n = number.unwrap_or_else(|| {
            let mut max = 0u32;
            if input || !output {
                max = max.max(self.defined_in.iter().copied().max().unwrap_or(0));
                max = max.max(self.connected_in.iter().copied().max().unwrap_or(0));
            }
            if output || !input {
                max = max.max(self.defined_out.iter().copied().max().unwrap_or(0));
                max = max.max(self.connected_out.iter().copied().max().unwrap_or(0));
            }
            max + 1
        });
        if input {
            self.defined_in.insert(n);
        }
        if output {
            self.defined_out.insert(n);
        }
        n
    }
    fn short(&mut self) {
        self.short_input();
        self.short_output();
    }
    fn short_input(&mut self) {
        self.input_severed = true;
        self.connected_in.remove(&self.selected_in.0);
        self.buf_in.clear();
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.short_input();
        }
    }
    fn short_output(&mut self) {
        self.output_severed = true;
        self.connected_out.remove(&self.selected_out.0);
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.short_output();
        }
    }
    fn commit(&mut self, level: i32) -> Result<(), PipeError> {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.commit(level)?;
        }
        Ok(())
    }
    fn yield_others(&mut self) -> Result<(), PipeError> {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.yield_others()?;
        }
        Ok(())
    }
    fn issue_message(&mut self, text: &str) {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.issue_pipeline_message(text);
        } else {
            issue_pipeline_message(text);
        }
    }
    fn stage_number(&self) -> i32 {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.stage_id().0 as i32 + 1
        } else {
            self.stage_num
        }
    }
    fn write_record(&mut self, rec: Record) -> Result<i32, PipeError> {
        if let Some(ctx) = self.ctx.as_mut() {
            if !ctx.output_connected(ctx.selected_output()) {
                return Ok(12);
            }
            match ctx.write(rec) {
                Ok(()) => Ok(0),
                Err(PipeError::Severed) => Ok(ctx.severed_rc()),
                Err(e) => Err(e),
            }
        } else {
            if self.output_severed || !self.connected_out.contains(&self.selected_out.0) {
                return Ok(self.severed_rc_buf());
            }
            self.buf_out.push(rec.to_string_lossy());
            Ok(0)
        }
    }
    fn set_stage_rc(&mut self, rc: i32) {
        self.stage_rc = Some(rc);
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.set_return_code(rc);
        }
    }
    fn set_eof_report(&mut self, mode: EofReport) {
        self.eof_report = mode;
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.set_eof_report(mode);
        }
    }
    fn eof_report(&self) -> EofReport {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.eof_report()
        } else {
            self.eof_report
        }
    }
    fn set_nocommit(&mut self, yes: bool) {
        self.nocommit = yes;
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.set_nocommit(yes);
        }
    }
    fn input_record_ready(&self, stream: StreamId) -> bool {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.input_record_ready(stream)
        } else {
            stream == self.selected_in && !self.buf_in.is_empty() && !self.input_severed
        }
    }
    fn input_at_eof(&self, stream: StreamId) -> bool {
        if let Some(ctx) = self.ctx.as_ref() {
            ctx.input_at_eof(stream)
        } else {
            stream != self.selected_in || self.buf_in.is_empty() || self.input_severed
        }
    }
    fn wait_any_input(&mut self, streams: &[StreamId]) -> Result<(), PipeError> {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.wait_any_input(streams)
        } else {
            Ok(())
        }
    }
    fn callpipe(&mut self, spec: &str) -> Result<(), PipeError> {
        RexxPipeHost::callpipe(self, spec)
    }
    fn run_rexx(&mut self, path: &str, args: &[String]) -> Result<(), PipeError> {
        let mut spec = format!("*: | {path}");
        for a in args {
            spec.push(' ');
            spec.push_str(a);
        }
        spec.push_str(" | *:");
        RexxPipeHost::callpipe(self, &spec)
    }
}
