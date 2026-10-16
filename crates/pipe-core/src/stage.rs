use std::cell::RefCell;
use std::rc::Rc;

use crate::dispatcher::{CallPipeConnectors, Shared, StageId};
use crate::error::{PipeError, PipeResult};
use crate::record::{Record, StreamId};

/// Why a stage suspended into the dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    Read,
    Write,
    Commit,
    CallPipe,
}

fn suspend(shared: &Rc<RefCell<Shared>>, stage_id: StageId, reason: WaitReason) {
    let ptr = {
        let g = shared.borrow();
        g.stages[stage_id.0].yielder
    };
    assert!(!ptr.is_null(), "StageCtx used outside of a running stage");
    // Safety: pointer is set while this stage's coroutine body is live.
    unsafe { (*ptr).suspend(reason) };
}

/// Entry point for a stage body. Invoked once inside a stackful coroutine.
pub type StageFn = Box<dyn FnOnce(&mut StageCtx)>;

/// Dispatcher services exposed to stages (CMS READ / WRITE / COMMIT / …).
pub struct StageCtx {
    pub(crate) stage_id: StageId,
    pub(crate) shared: Rc<RefCell<Shared>>,
    selected_in: StreamId,
    selected_out: StreamId,
    /// Highest commit level reached by this stage.
    commit_level: i32,
    /// Record currently delayed (CMS DELAY).
    delayed: Option<Record>,
}

impl StageCtx {
    pub(crate) fn new(stage_id: StageId, shared: Rc<RefCell<Shared>>) -> Self {
        Self {
            stage_id,
            shared,
            selected_in: StreamId::PRIMARY,
            selected_out: StreamId::PRIMARY,
            commit_level: crate::COMMIT_INITIAL,
            delayed: None,
        }
    }

    pub fn stage_id(&self) -> StageId {
        self.stage_id
    }

    pub fn select_input(&mut self, stream: StreamId) {
        self.selected_in = stream;
    }

    pub fn select_output(&mut self, stream: StreamId) {
        self.selected_out = stream;
    }

    pub fn selected_input(&self) -> StreamId {
        self.selected_in
    }

    pub fn selected_output(&self) -> StreamId {
        self.selected_out
    }

    pub fn commit_level(&self) -> i32 {
        self.commit_level
    }

    /// Commit to `level` (CMS COMMIT). Negotiates with peers before returning.
    pub fn commit(&mut self, level: i32) -> PipeResult<()> {
        {
            let g = self.shared.borrow();
            g.trace_stage(self.stage_id, format!("COMMIT {level} request"));
        }
        {
            let mut g = self.shared.borrow_mut();
            g.stages[self.stage_id.0]
                .waiting_commit
                .replace(level);
            g.stages[self.stage_id.0].commit_level = level;
        }
        let mut waited = false;
        loop {
            let ready = {
                let g = self.shared.borrow();
                g.commit_satisfied(self.stage_id, level)
            };
            if ready {
                let mut g = self.shared.borrow_mut();
                g.stages[self.stage_id.0].waiting_commit = None;
                self.commit_level = level;
                let detail = if waited {
                    format!("COMMIT {level} proceed (after wait)")
                } else {
                    format!("COMMIT {level} proceed")
                };
                g.trace_stage(self.stage_id, detail);
                return Ok(());
            }
            waited = true;
            {
                let g = self.shared.borrow();
                g.trace_stage(self.stage_id, format!("COMMIT {level} wait"));
            }
            suspend(&self.shared, self.stage_id, WaitReason::Commit);
            if self.shared.borrow().aborted {
                return Err(PipeError::Aborted);
            }
        }
    }

    /// Read one record from the selected input stream. `Ok(None)` is end-of-file.
    pub fn read(&mut self) -> PipeResult<Option<Record>> {
        if let Some(rec) = self.delayed.take() {
            {
                let g = self.shared.borrow();
                let preview = g.trace.preview(&rec);
                g.trace_stage(
                    self.stage_id,
                    format!(
                        "READ stream={} len={} (delayed){preview}",
                        self.selected_in.0,
                        rec.len()
                    ),
                );
            }
            return Ok(Some(rec));
        }
        loop {
            let outcome = {
                let mut g = self.shared.borrow_mut();
                g.take_input(self.stage_id, self.selected_in)
            };
            match outcome {
                ReadOutcome::Record(r) => {
                    {
                        let g = self.shared.borrow();
                        let preview = g.trace.preview(&r);
                        g.trace_stage(
                            self.stage_id,
                            format!(
                                "READ stream={} len={}{preview}",
                                self.selected_in.0,
                                r.len()
                            ),
                        );
                    }
                    return Ok(Some(r));
                }
                ReadOutcome::Eof => {
                    {
                        let g = self.shared.borrow();
                        g.trace_stage(
                            self.stage_id,
                            format!("READ stream={} EOF", self.selected_in.0),
                        );
                    }
                    return Ok(None);
                }
                ReadOutcome::Severed => {
                    {
                        let g = self.shared.borrow();
                        g.trace_stage(
                            self.stage_id,
                            format!("READ stream={} SEVERED", self.selected_in.0),
                        );
                    }
                    return Err(PipeError::Severed);
                }
                ReadOutcome::Wait => {
                    {
                        let mut g = self.shared.borrow_mut();
                        g.stages[self.stage_id.0].waiting_read = Some(self.selected_in);
                        g.trace_stage(
                            self.stage_id,
                            format!("READ stream={} wait", self.selected_in.0),
                        );
                    }
                    suspend(&self.shared, self.stage_id, WaitReason::Read);
                    {
                        let mut g = self.shared.borrow_mut();
                        g.stages[self.stage_id.0].waiting_read = None;
                    }
                    if self.shared.borrow().aborted {
                        return Err(PipeError::Aborted);
                    }
                }
            }
        }
    }

    /// Write one record to the selected output stream.
    pub fn write(&mut self, record: Record) -> PipeResult<()> {
        loop {
            let outcome = {
                let mut g = self.shared.borrow_mut();
                g.put_output(self.stage_id, self.selected_out, record.clone())
            };
            match outcome {
                WriteOutcome::Accepted => {
                    {
                        let g = self.shared.borrow();
                        let preview = g.trace.preview(&record);
                        g.trace_stage(
                            self.stage_id,
                            format!(
                                "WRITE stream={} len={}{preview}",
                                self.selected_out.0,
                                record.len()
                            ),
                        );
                    }
                    return Ok(());
                }
                WriteOutcome::Severed => {
                    {
                        let g = self.shared.borrow();
                        g.trace_stage(
                            self.stage_id,
                            format!("WRITE stream={} SEVERED", self.selected_out.0),
                        );
                    }
                    return Err(PipeError::Severed);
                }
                WriteOutcome::NoConnection => {
                    {
                        let g = self.shared.borrow();
                        let preview = g.trace.preview(&record);
                        g.trace_stage(
                            self.stage_id,
                            format!(
                                "WRITE stream={} discard (null-ended) len={}{preview}",
                                self.selected_out.0,
                                record.len()
                            ),
                        );
                    }
                    return Ok(());
                }
                WriteOutcome::Wait => {
                    {
                        let mut g = self.shared.borrow_mut();
                        g.stages[self.stage_id.0].waiting_write = Some(self.selected_out);
                        g.trace_stage(
                            self.stage_id,
                            format!("WRITE stream={} wait", self.selected_out.0),
                        );
                    }
                    suspend(&self.shared, self.stage_id, WaitReason::Write);
                    {
                        let mut g = self.shared.borrow_mut();
                        g.stages[self.stage_id.0].waiting_write = None;
                    }
                    if self.shared.borrow().aborted {
                        return Err(PipeError::Aborted);
                    }
                }
            }
        }
    }

    /// Delay the just-read record so the next `read` returns it again (CMS DELAY).
    pub fn delay_record(&mut self, record: Record) {
        {
            let g = self.shared.borrow();
            g.trace_stage(
                self.stage_id,
                format!("DELAY len={}", record.len()),
            );
        }
        self.delayed = Some(record);
    }

    /// Sever the selected input and/or output connection (CMS SHORT).
    pub fn short(&mut self) {
        {
            let g = self.shared.borrow();
            g.trace_stage(
                self.stage_id,
                format!(
                    "SHORT in={} out={}",
                    self.selected_in.0, self.selected_out.0
                ),
            );
        }
        let mut g = self.shared.borrow_mut();
        g.short_streams(self.stage_id, self.selected_in, self.selected_out);
    }

    /// Sever only the selected input stream.
    pub fn short_input(&mut self) {
        {
            let g = self.shared.borrow();
            g.trace_stage(
                self.stage_id,
                format!("SHORT input stream={}", self.selected_in.0),
            );
        }
        let mut g = self.shared.borrow_mut();
        g.short_input(self.stage_id, self.selected_in);
    }

    /// Sever only the selected output stream.
    pub fn short_output(&mut self) {
        {
            let g = self.shared.borrow();
            g.trace_stage(
                self.stage_id,
                format!("SHORT output stream={}", self.selected_out.0),
            );
        }
        let mut g = self.shared.borrow_mut();
        g.short_output(self.stage_id, self.selected_out);
    }

    /// Run a nested pipeline specification connected to this stage (CMS CALLPIPE).
    ///
    /// `connectors` describes how `*:` connectors at the ends of the nested
    /// specification attach to this stage's streams.
    pub fn callpipe(
        &mut self,
        build: impl FnOnce(&mut crate::dispatcher::PipelineBuilder),
        connectors: CallPipeConnectors,
    ) -> PipeResult<()> {
        crate::dispatcher::run_callpipe(self, build, connectors)
    }

    /// Mark this stage (and the pipeline) as failed without unwinding.
    pub fn fail(&mut self, msg: impl Into<String>) {
        let mut g = self.shared.borrow_mut();
        g.stages[self.stage_id.0].error = Some(PipeError::stage(msg));
        g.aborted = true;
    }

    /// Convenience: write a UTF-8 string as a record.
    pub fn write_str(&mut self, s: &str) -> PipeResult<()> {
        self.write(Record::from(s))
    }

    /// Read all remaining records into a buffer (for stages like SORT/BUFFER).
    pub fn read_all(&mut self) -> PipeResult<Vec<Record>> {
        let mut out = Vec::new();
        while let Some(r) = self.read()? {
            out.push(r);
        }
        Ok(out)
    }
}

#[derive(Debug)]
pub(crate) enum ReadOutcome {
    Record(Record),
    Eof,
    Severed,
    Wait,
}

#[derive(Debug)]
pub(crate) enum WriteOutcome {
    Accepted,
    Severed,
    NoConnection,
    Wait,
}
