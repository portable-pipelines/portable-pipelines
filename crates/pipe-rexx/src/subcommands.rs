use pipe_core::{Record, StageCtx, StreamId};
use pipe_core::{PipeError, PipeResult};

/// Pipeline subcommands exposed to REXX stages (CMS-style).
pub struct RexxPipeHost<'a> {
    pub ctx: &'a mut StageCtx,
    pub last_record: Option<String>,
    pub rc: i32,
}

impl<'a> RexxPipeHost<'a> {
    pub fn new(ctx: &'a mut StageCtx) -> Self {
        Self {
            ctx,
            last_record: None,
            rc: 0,
        }
    }

    /// READTO varname — read one record into the variable (stored in last_record;
    /// interpreter assigns the name).
    pub fn readto(&mut self) -> PipeResult<Option<String>> {
        match self.ctx.read() {
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
    }

    /// OUTPUT — write a string as one record (IBM pipeline subcommand).
    pub fn output(&mut self, data: &str) -> PipeResult<()> {
        self.ctx.write(Record::from(data))?;
        self.rc = 0;
        Ok(())
    }

    pub fn short(&mut self) {
        self.ctx.short();
        self.rc = 0;
    }

    pub fn select_input(&mut self, n: u32) {
        self.ctx.select_input(StreamId(n));
    }

    pub fn select_output(&mut self, n: u32) {
        self.ctx.select_output(StreamId(n));
    }

    pub fn callpipe(&mut self, spec: &str) -> PipeResult<()> {
        crate::callpipe_spec(self.ctx, spec)?;
        self.rc = 0;
        Ok(())
    }

    pub fn addpipe(&mut self, spec: &str) -> PipeResult<()> {
        // ADDPIPE: for v1, treat like CALLPIPE (attach nested network).
        self.callpipe(spec)
    }

    pub fn commit(&mut self, level: i32) -> PipeResult<()> {
        self.ctx.commit(level)?;
        self.rc = 0;
        Ok(())
    }

    pub fn set_rc_error(&mut self, e: &PipeError) {
        self.rc = 8;
        let _ = e;
    }
}
