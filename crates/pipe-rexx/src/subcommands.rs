use std::collections::VecDeque;

use pipe_core::{Record, StageCtx, StreamId};
use pipe_core::{PipeError, PipeResult};

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
    selected_in: StreamId,
    selected_out: StreamId,
}

impl<'a> RexxPipeHost<'a> {
    pub fn new(ctx: &'a mut StageCtx) -> Self {
        Self {
            ctx: Some(ctx),
            buf_in: VecDeque::new(),
            buf_out: Vec::new(),
            last_record: None,
            rc: 0,
            selected_in: StreamId::PRIMARY,
            selected_out: StreamId::PRIMARY,
        }
    }

    /// Host that serves `READTO` / `OUTPUT` from memory (for Regina).
    pub fn buffered(input: Vec<String>) -> RexxPipeHost<'static> {
        RexxPipeHost {
            ctx: None,
            buf_in: input.into(),
            buf_out: Vec::new(),
            last_record: None,
            rc: 0,
            selected_in: StreamId::PRIMARY,
            selected_out: StreamId::PRIMARY,
        }
    }

    pub fn into_output(self) -> Vec<String> {
        self.buf_out
    }

    /// READTO varname — read one record into the variable (stored in last_record;
    /// interpreter assigns the name).
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
                    self.rc = 12; // CMS-like end-of-file RC
                    self.last_record = None;
                    Ok(None)
                }
                Err(e) => {
                    self.rc = 8;
                    Err(e)
                }
            }
        } else {
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

    /// OUTPUT — write a string as one record (IBM pipeline subcommand, p.648).
    /// The caller supplies the remainder after `OUTPUT` and one blank, exactly.
    pub fn output(&mut self, data: &str) -> PipeResult<()> {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.write(Record::from(data))?;
        } else {
            self.buf_out.push(data.to_string());
        }
        self.rc = 0;
        Ok(())
    }

    pub fn short(&mut self) {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.short();
        }
        self.rc = 0;
    }

    pub fn select_input(&mut self, n: u32) {
        self.selected_in = StreamId(n);
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.select_input(StreamId(n));
        }
    }

    pub fn select_output(&mut self, n: u32) {
        self.selected_out = StreamId(n);
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.select_output(StreamId(n));
        }
    }

    pub fn callpipe(&mut self, spec: &str) -> PipeResult<()> {
        let Some(ctx) = self.ctx.as_mut() else {
            return Err(PipeError::stage(
                "CALLPIPE under Regina requires live streams (not yet buffered)",
            ));
        };
        crate::callpipe_spec(ctx, spec)?;
        self.rc = 0;
        Ok(())
    }

    pub fn addpipe(&mut self, spec: &str) -> PipeResult<()> {
        self.callpipe(spec)
    }

    pub fn commit(&mut self, level: i32) -> PipeResult<()> {
        if let Some(ctx) = self.ctx.as_mut() {
            ctx.commit(level)?;
        }
        self.rc = 0;
        Ok(())
    }

    pub fn set_rc_error(&mut self, e: &PipeError) {
        self.rc = 8;
        let _ = e;
    }
}
