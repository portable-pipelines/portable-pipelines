use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, Yielder};

use crate::error::{PipeError, PipeResult, StageOrigin};
use crate::record::{Record, StreamId};
use crate::stage::{ReadOutcome, StageCtx, StageFn, WaitReason, WriteOutcome};
use crate::trace::TraceConfig;

/// Index of a stage within a pipeline network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageId(pub usize);

/// Which end of a nested stage a CALLPIPE connector attaches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanSide {
    Input,
    Output,
}

/// One CALLPIPE connector: loan a caller stream to a nested stage for the call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallPipeAttachment {
    pub side: LoanSide,
    /// Stream on the caller (selected stream, or `*.input.n:`).
    pub caller_stream: StreamId,
    /// Index of the nested stage in the builder passed to [`StageCtx::callpipe`].
    pub nested_stage: usize,
    /// Stream on that nested stage (primary unless a label neighbour).
    pub nested_stream: StreamId,
}

/// How CALLPIPE connectors attach nested pipeline ends to the caller stage.
#[derive(Debug, Clone, Default)]
pub struct CallPipeConnectors {
    pub attachments: Vec<CallPipeAttachment>,
    /// Optional `callpipe …` text for nested errors (message 2).
    pub processing: Option<String>,
}

/// A live loan of a caller connection onto a nested stage.
#[derive(Debug, Clone, Copy)]
struct StreamLoan {
    parent: StageId,
    parent_stream: StreamId,
    side: LoanSide,
}

/// Nested CALLPIPE graph the caller is waiting on.
#[derive(Debug, Clone)]
struct CallPipeSession {
    children: Vec<StageId>,
    processing: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Connection {
    pub writer: StageId,
    pub writer_stream: StreamId,
    pub reader: StageId,
    pub reader_stream: StreamId,
    pub slot: Option<Record>,
    pub severed: bool,
    pub writer_done: bool,
    /// CALLPIPE loans, innermost last. A nested CALLPIPE pushes another
    /// frame so restore and `mark_writer_done` still see the outer owner.
    loan_stack: Vec<StreamLoan>,
}

impl Connection {
    fn new(
        writer: StageId,
        writer_stream: StreamId,
        reader: StageId,
        reader_stream: StreamId,
    ) -> Self {
        Self {
            writer,
            writer_stream,
            reader,
            reader_stream,
            slot: None,
            severed: false,
            writer_done: false,
            loan_stack: Vec::new(),
        }
    }

    fn is_output_loaned(&self) -> bool {
        self.loan_stack.iter().any(|l| l.side == LoanSide::Output)
    }

    fn is_input_loaned(&self) -> bool {
        self.loan_stack.iter().any(|l| l.side == LoanSide::Input)
    }
}

pub(crate) struct StageState {
    pub name: String,
    pub commit_level: i32,
    pub waiting_read: Option<StreamId>,
    /// Additional input streams a stage is waiting on (`StageCtx::read_any`).
    /// The stage is runnable if any listed stream has a record, EOF, or is severed.
    pub waiting_read_any: Vec<StreamId>,
    pub waiting_write: Option<StreamId>,
    pub waiting_commit: Option<i32>,
    /// True while the stage has yielded so every other ready stage can run.
    pub waiting_yield: bool,
    /// Wall-clock deadline for [`StageCtx::wait_until`] (DELAY / BEAT).
    /// PIPESTOP (p.458) wakes these waiters without waiting out the deadline.
    pub waiting_until: Option<Instant>,
    /// Nested CALLPIPE graph this stage spawned and is waiting to finish.
    waiting_callpipe: Option<CallPipeSession>,
    /// Isolated commit set. CALLPIPE nested stages get a fresh group so a
    /// nested COMMIT does not wait on the parked caller.
    pub commit_group: u32,
    pub ended: bool,
    pub error: Option<PipeError>,
    /// CMS stage return code (default 0). AGGRC sets this when last (p.258).
    pub return_code: i32,
    /// Live only while this stage's coroutine body is on the stack.
    pub yielder: *const corosensei::Yielder<(), WaitReason>,
    /// Spec position for CMS messages 1 / 3 / 4 (`None` for synthetic stages).
    pub origin: Option<StageOrigin>,
}

pub(crate) struct Shared {
    pub stages: Vec<StageState>,
    pub connections: Vec<Connection>,
    pub aborted: bool,
    /// PIPESTOP (Author's Edition 1.1.11 pp.458–459): portable stand-in for
    /// posting ECBs and forbidding further waits on external/time events.
    /// Stages parked in `waiting_until` wake and `wait_until` returns
    /// `Aborted`. I/O waiters are not aborted (p.150: only waiting stages).
    pub external_stop: bool,
    /// Global minimum commit currently allowed to proceed (simplified negotiation).
    #[allow(dead_code)]
    pub network_commit: i32,
    pub trace: TraceConfig,
    /// Sibling stages queued by [`StageCtx::spawn_stage`] (ADDPIPE-like).
    /// Drained into the live coroutine list before the next `pick_next`.
    pub pending_spawns: Vec<(StageId, StageFn)>,
    /// Highest CALLPIPE commit group allocated on this network.
    pub next_commit_group: u32,
}

impl Shared {
    pub(crate) fn stage_name(&self, id: StageId) -> &str {
        &self.stages[id.0].name
    }

    pub(crate) fn trace_stage(&self, id: StageId, detail: impl AsRef<str>) {
        if !self.trace.is_enabled() {
            return;
        }
        self.trace.emit(format!(
            "stage[{}] {} {}",
            id.0,
            self.stage_name(id),
            detail.as_ref()
        ));
    }
}

impl Shared {
    fn connection_for_output(&mut self, stage: StageId, stream: StreamId) -> Option<usize> {
        self.connections
            .iter()
            .position(|c| c.writer == stage && c.writer_stream == stream)
    }

    fn connection_for_input(&mut self, stage: StageId, stream: StreamId) -> Option<usize> {
        self.connections
            .iter()
            .position(|c| c.reader == stage && c.reader_stream == stream)
    }

    pub(crate) fn take_input(&mut self, stage: StageId, stream: StreamId) -> ReadOutcome {
        let Some(idx) = self.connection_for_input(stage, stream) else {
            // Null-ended input: immediate EOF.
            return ReadOutcome::Eof;
        };
        let c = &mut self.connections[idx];
        if c.severed {
            return ReadOutcome::Severed;
        }
        if let Some(rec) = c.slot.take() {
            return ReadOutcome::Record(rec);
        }
        if c.writer_done {
            return ReadOutcome::Eof;
        }
        ReadOutcome::Wait
    }

    pub(crate) fn put_output(
        &mut self,
        stage: StageId,
        stream: StreamId,
        record: Record,
    ) -> WriteOutcome {
        let Some(idx) = self.connection_for_output(stage, stream) else {
            return WriteOutcome::NoConnection;
        };
        let c = &mut self.connections[idx];
        if c.severed {
            return WriteOutcome::Severed;
        }
        if c.slot.is_some() {
            return WriteOutcome::Wait;
        }
        c.slot = Some(record);
        WriteOutcome::Accepted
    }

    pub(crate) fn short_streams(&mut self, stage: StageId, inn: StreamId, out: StreamId) {
        self.short_input(stage, inn);
        self.short_output(stage, out);
    }

    pub(crate) fn short_input(&mut self, stage: StageId, stream: StreamId) {
        if let Some(idx) = self.connection_for_input(stage, stream) {
            self.connections[idx].severed = true;
            self.connections[idx].slot = None;
            self.connections[idx].writer_done = true;
        }
    }

    pub(crate) fn short_output(&mut self, stage: StageId, stream: StreamId) {
        if let Some(idx) = self.connection_for_output(stage, stream) {
            // Nested SHORT/SEVER on a loaned output ends the nested
            // contribution without killing the caller's stream. Buffered
            // CALLPIPE collected nested output then let the caller write
            // again; keep that after restore. Any stack frame counts: a
            // nested CALLPIPE must not sever the outer caller's stream.
            if self.connections[idx].is_output_loaned() {
                return;
            }
            self.connections[idx].severed = true;
            self.connections[idx].slot = None;
            self.connections[idx].writer_done = true;
        }
    }

    /// Mark the selected output at end-of-file without SHORT.
    ///
    /// The consumer's next `read` is `Ok(None)`, not `PipeError::Severed`.
    /// Unlike [`Self::short_output`], an in-flight slot is left alone (the
    /// caller waits until it is empty) and the connection stays "connected"
    /// so `output_connected` remains true until the stage ends.
    pub(crate) fn close_output(&mut self, stage: StageId, stream: StreamId) {
        if let Some(idx) = self.connection_for_output(stage, stream) {
            if self.connections[idx].is_output_loaned() {
                return;
            }
            self.connections[idx].writer_done = true;
        }
    }

    pub(crate) fn input_is_loaned(&self, stage: StageId, stream: StreamId) -> bool {
        self.connections
            .iter()
            .any(|c| c.reader == stage && c.reader_stream == stream && c.is_input_loaned())
    }

    pub(crate) fn input_has_record(&self, reader: StageId, stream: StreamId) -> bool {
        self.connections.iter().any(|c| {
            c.reader == reader && c.reader_stream == stream && !c.severed && c.slot.is_some()
        })
    }

    pub(crate) fn input_can_progress(&self, reader: StageId, stream: StreamId) -> bool {
        match self
            .connections
            .iter()
            .find(|c| c.reader == reader && c.reader_stream == stream)
        {
            Some(c) => c.slot.is_some() || c.writer_done || c.severed,
            None => true,
        }
    }

    pub(crate) fn output_can_accept(&self, writer: StageId, stream: StreamId) -> bool {
        match self
            .connections
            .iter()
            .find(|c| c.writer == writer && c.writer_stream == stream)
        {
            None => true,
            Some(c) => c.severed || c.slot.is_none(),
        }
    }

    pub(crate) fn mark_writer_done(&mut self, stage: StageId) {
        for c in &mut self.connections {
            if c.writer != stage {
                continue;
            }
            // Nested end on a loaned output must not EOF the caller's downstream.
            // The stack keeps the outer owner when CALLPIPE is nested in CALLPIPE.
            if c.is_output_loaned() {
                continue;
            }
            c.writer_done = true;
        }
    }

    /// Simplified commit: a stage waiting for level L may proceed when every
    /// live stage has either ended or reached commit_level >= L, or is also
    /// waiting for a commit >= L.
    fn new_stage_state(name: String, origin: Option<StageOrigin>) -> StageState {
        StageState {
            name,
            commit_level: crate::COMMIT_INITIAL,
            waiting_read: None,
            waiting_read_any: Vec::new(),
            waiting_write: None,
            waiting_commit: None,
            waiting_yield: false,
            waiting_until: None,
            waiting_callpipe: None,
            commit_group: 0,
            ended: false,
            error: None,
            return_code: 0,
            yielder: std::ptr::null(),
            origin,
        }
    }

    pub(crate) fn spawn_stage(&mut self, name: String, entry: StageFn) -> StageId {
        let id = StageId(self.stages.len());
        self.stages.push(Self::new_stage_state(name, None));
        self.pending_spawns.push((id, entry));
        id
    }

    pub(crate) fn connect_runtime(
        &mut self,
        writer: StageId,
        writer_stream: StreamId,
        reader: StageId,
        reader_stream: StreamId,
    ) {
        self.connections.push(Connection::new(
            writer,
            writer_stream,
            reader,
            reader_stream,
        ));
    }

    pub(crate) fn retarget_reader(
        &mut self,
        old_reader: StageId,
        old_stream: StreamId,
        new_reader: StageId,
        new_stream: StreamId,
    ) {
        for c in &mut self.connections {
            if c.reader == old_reader && c.reader_stream == old_stream && !c.severed {
                c.reader = new_reader;
                c.reader_stream = new_stream;
            }
        }
    }

    pub(crate) fn retarget_writer(
        &mut self,
        old_writer: StageId,
        old_stream: StreamId,
        new_writer: StageId,
        new_stream: StreamId,
    ) {
        for c in &mut self.connections {
            if c.writer == old_writer && c.writer_stream == old_stream && !c.severed {
                c.writer = new_writer;
                c.writer_stream = new_stream;
            }
        }
    }

    pub(crate) fn commit_satisfied(&self, stage: StageId, level: i32) -> bool {
        let group = self.stages[stage.0].commit_group;
        for s in &self.stages {
            if s.ended {
                continue;
            }
            if s.commit_group != group {
                continue;
            }
            if s.commit_level >= level {
                continue;
            }
            if let Some(w) = s.waiting_commit {
                if w >= level {
                    continue;
                }
            }
            return false;
        }
        true
    }
}

/// Specification for one stage before the dispatcher runs.
pub struct StageSpec {
    pub name: String,
    pub entry: StageFn,
    pub origin: Option<StageOrigin>,
}

/// Edge between an output stream and an input stream.
#[derive(Debug, Clone)]
pub struct ConnectionSpec {
    pub writer: StageId,
    pub writer_stream: StreamId,
    pub reader: StageId,
    pub reader_stream: StreamId,
}

/// Build a pipeline graph then run it.
pub struct PipelineBuilder {
    stages: Vec<StageSpec>,
    connections: Vec<ConnectionSpec>,
    trace: TraceConfig,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            connections: Vec::new(),
            trace: TraceConfig::off(),
        }
    }

    pub fn set_trace(&mut self, trace: TraceConfig) {
        self.trace = trace;
    }

    pub fn with_trace(mut self, trace: TraceConfig) -> Self {
        self.trace = trace;
        self
    }

    pub fn add_stage(&mut self, name: impl Into<String>, entry: StageFn) -> StageId {
        self.add_stage_with_origin(name, entry, None)
    }

    /// Like [`Self::add_stage`], with CMS stage/pipeline origin for error context.
    pub fn add_stage_with_origin(
        &mut self,
        name: impl Into<String>,
        entry: StageFn,
        origin: Option<StageOrigin>,
    ) -> StageId {
        let id = StageId(self.stages.len());
        self.stages.push(StageSpec {
            name: name.into(),
            entry,
            origin,
        });
        id
    }

    pub fn connect(
        &mut self,
        writer: StageId,
        writer_stream: StreamId,
        reader: StageId,
        reader_stream: StreamId,
    ) {
        self.connections.push(ConnectionSpec {
            writer,
            writer_stream,
            reader,
            reader_stream,
        });
    }

    /// Connect primary output of `writer` to primary input of `reader`.
    pub fn connect_primary(&mut self, writer: StageId, reader: StageId) {
        self.connect(writer, StreamId::PRIMARY, reader, StreamId::PRIMARY);
    }

    pub fn run(self) -> PipeResult<()> {
        self.run_with_return_code().map(|_| ())
    }

    /// Run the network and return the CMS aggregate return code of every
    /// stage (most negative if any is negative, else the largest; p.219).
    /// A nonzero code is not a pipeline error: AGGRC as last stage is
    /// expected to return the aggregate (p.258).
    pub fn run_with_return_code(self) -> PipeResult<i32> {
        Dispatcher::run(self.stages, self.connections, self.trace)
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

enum CoState {
    NotStarted(StageFn),
    // Coroutine yields WaitReason and is resumed with ().
    Running(Coroutine<(), WaitReason, ()>),
    Done,
}

/// Coroutine dispatcher: resumes ready stages with drain (consumer) bias.
pub struct Dispatcher;

impl Dispatcher {
    pub fn run(
        stages: Vec<StageSpec>,
        connections: Vec<ConnectionSpec>,
        trace: TraceConfig,
    ) -> PipeResult<i32> {
        if trace.is_enabled() {
            trace.emit(format!(
                "dispatcher start ({} stages, {} connections)",
                stages.len(),
                connections.len()
            ));
        }
        let shared = Rc::new(RefCell::new(Shared {
            stages: stages
                .iter()
                .map(|s| Shared::new_stage_state(s.name.clone(), s.origin.clone()))
                .collect(),
            connections: connections
                .into_iter()
                .map(|c| Connection::new(c.writer, c.writer_stream, c.reader, c.reader_stream))
                .collect(),
            aborted: false,
            external_stop: false,
            network_commit: crate::COMMIT_INITIAL, // reserved for richer commit lattices
            trace,
            pending_spawns: Vec::new(),
            next_commit_group: 0,
        }));

        let mut cos: Vec<CoState> = stages
            .into_iter()
            .map(|s| CoState::NotStarted(s.entry))
            .collect();

        // Kick every stage once so producers can start.
        let mut i = 0;
        while i < cos.len() {
            resume_stage(i, &mut cos, &shared)?;
            absorb_pending(&mut cos, &shared)?;
            i += 1;
        }

        // Main scheduling loop — drain-biased.
        let mut idle_passes = 0;
        loop {
            absorb_pending(&mut cos, &shared)?;
            if shared.borrow().stages.iter().all(|s| s.ended) {
                break;
            }
            // Clone the error first so the RefCell borrow ends before borrow_mut
            // (if-let would otherwise keep the temporary Ref alive across the body).
            let stage_err = shared.borrow().stages.iter().find_map(|s| s.error.clone());
            if let Some(err) = stage_err {
                let err = wrap_nested_callpipe_error(&shared.borrow(), err);
                abort_and_drain(&mut cos, &shared);
                return Err(err);
            }

            let next = pick_next(&shared.borrow());
            if let Some(i) = next {
                idle_passes = 0;
                resume_stage(i, &mut cos, &shared)?;
                absorb_pending(&mut cos, &shared)?;
            } else {
                idle_passes += 1;
                // Progress may still exist: a write filled a slot — wake readers.
                let mut progressed = false;
                // Only resume a reader for the stream it is waiting on.
                // LOOKUP reads the secondary to EOF first; a primary record
                // already in the slot must not wake that wait (1.1.11 p.418).
                let readers = {
                    let g = shared.borrow();
                    g.stages
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| {
                            if s.ended {
                                return None;
                            }
                            if let Some(stream) = s.waiting_read {
                                if g.input_can_progress(StageId(i), stream) {
                                    return Some(i);
                                }
                            }
                            if !s.waiting_read_any.is_empty()
                                && s.waiting_read_any
                                    .iter()
                                    .any(|&stream| g.input_can_progress(StageId(i), stream))
                            {
                                return Some(i);
                            }
                            None
                        })
                        .collect::<Vec<_>>()
                };
                for i in readers {
                    resume_stage(i, &mut cos, &shared)?;
                    progressed = true;
                }
                // Wake a writer only for the stream it is waiting on.
                // FANOUT waiting on a full secondary must not be resumed
                // just because its primary slot is empty (that livelocks a
                // fanout/fanin stall instead of reporting it).
                let writers = {
                    let g = shared.borrow();
                    g.stages
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| {
                            if s.ended {
                                return None;
                            }
                            let stream = s.waiting_write?;
                            if g.output_can_accept(StageId(i), stream) {
                                Some(i)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                };
                for i in writers {
                    resume_stage(i, &mut cos, &shared)?;
                    progressed = true;
                }
                // Wake commit waiters if negotiation may have completed.
                for i in 0..shared.borrow().stages.len() {
                    if shared.borrow().stages[i].waiting_commit.is_some() {
                        resume_stage(i, &mut cos, &shared)?;
                        progressed = true;
                    }
                }
                if !progressed {
                    // DELAY (p.321) / BEAT (p.271): if only wall-clock waiters
                    // remain, sleep until the nearest deadline instead of
                    // declaring a stall.
                    if let Some(deadline) = nearest_sleep_deadline(&shared.borrow()) {
                        let now = Instant::now();
                        if deadline > now {
                            std::thread::sleep(deadline - now);
                        }
                        idle_passes = 0;
                        continue;
                    }
                    if idle_passes > 64 {
                        // Deadlock / stall — wake everyone so suspended
                        // coroutines observe abort and exit before we drop
                        // them (corosensei cannot drop a parked stack).
                        let (friendly, raw, origin) = {
                            let g = shared.borrow();
                            let raw = raw_stall_dump(&g);
                            if g.trace.is_enabled() {
                                g.trace.emit(format!("stall dump: {raw}"));
                            }
                            let friendly = friendly_stall_message(&g);
                            let origin = g
                                .stages
                                .iter()
                                .find(|s| !s.ended && stall_wait_text(s).is_some())
                                .and_then(|s| s.origin.clone());
                            (friendly, raw, origin)
                        };
                        let _ = raw;
                        abort_and_drain(&mut cos, &shared);
                        let err = PipeError::msg(friendly);
                        return Err(match origin {
                            Some(o) => err.with_origin(o),
                            None => err,
                        });
                    }
                } else {
                    idle_passes = 0;
                }
            }
        }

        if shared.borrow().trace.is_enabled() {
            shared.borrow().trace.emit("dispatcher end");
        }
        let pipe_rc =
            crate::aggregate_return_codes(shared.borrow().stages.iter().map(|s| s.return_code));
        Ok(pipe_rc)
    }
}

fn raw_stall_dump(shared: &Shared) -> String {
    shared
        .stages
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.ended)
        .map(|(i, s)| {
            format!(
                "{}[{}]: read={:?} read_any={:?} write={:?} commit={:?} yield={} until={:?} callpipe={}",
                s.name,
                i,
                s.waiting_read,
                s.waiting_read_any,
                s.waiting_write,
                s.waiting_commit,
                s.waiting_yield,
                s.waiting_until,
                s.waiting_callpipe.is_some()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn stall_wait_text(s: &StageState) -> Option<String> {
    if s.waiting_callpipe.is_some() {
        return Some("waiting for CALLPIPE".into());
    }
    if let Some(stream) = s.waiting_write {
        return Some(format!("waiting to write to output stream {}", stream.0));
    }
    if let Some(stream) = s.waiting_read {
        return Some(format!("waiting to read from input stream {}", stream.0));
    }
    if !s.waiting_read_any.is_empty() {
        let streams = s
            .waiting_read_any
            .iter()
            .map(|st| st.0.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!("waiting to read from input stream {streams}"));
    }
    if s.waiting_commit.is_some() {
        return Some("waiting to commit".into());
    }
    if s.waiting_until.is_some() {
        return Some("waiting for a timer".into());
    }
    if s.waiting_yield {
        return Some("waiting to yield".into());
    }
    None
}

fn callpipe_children_done(shared: &Shared, session: &CallPipeSession) -> bool {
    if shared.aborted {
        return true;
    }
    session
        .children
        .iter()
        .all(|id| id.0 < shared.stages.len() && shared.stages[id.0].ended)
}

fn wrap_nested_callpipe_error(shared: &Shared, err: PipeError) -> PipeError {
    for s in &shared.stages {
        let Some(session) = s.waiting_callpipe.as_ref() else {
            continue;
        };
        let owns = session
            .children
            .iter()
            .any(|id| id.0 < shared.stages.len() && shared.stages[id.0].error.is_some());
        if owns {
            if let Some(p) = &session.processing {
                return err.with_processing(p);
            }
        }
    }
    err
}

fn friendly_stall_message(shared: &Shared) -> String {
    let mut lines = vec!["pipeline stalled (no stage can make progress)".to_string()];
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        let Some(wait) = stall_wait_text(s) else {
            continue;
        };
        let (num, verb) = match &s.origin {
            Some(o) => (o.stage, s.name.clone()),
            None => ((i + 1) as u32, s.name.clone()),
        };
        lines.push(format!("  stage {num}  {verb}   {wait}"));
    }
    lines.push(
        "Hint: a stage is waiting on a stream that another stage will not service until it finishes. Check the order FANIN reads its streams, or consider FANINANY."
            .into(),
    );
    lines.join("\n")
}

/// Mark the network aborted and resume every live stage until they exit so
/// parked coroutines are not dropped (corosensei cannot unwind a parked stack).
fn abort_and_drain(cos: &mut Vec<CoState>, shared: &Rc<RefCell<Shared>>) {
    shared.borrow_mut().aborted = true;
    let _ = absorb_pending(cos, shared);
    for _ in 0..64 {
        if shared.borrow().stages.iter().all(|s| s.ended) {
            return;
        }
        for i in 0..cos.len() {
            if !shared.borrow().stages[i].ended {
                let _ = resume_stage(i, cos, shared);
            }
        }
        let _ = absorb_pending(cos, shared);
    }
}

/// Start stages queued by [`StageCtx::spawn_stage`] so `pick_next` never
/// sees a live `StageState` without a matching coroutine.
fn absorb_pending(cos: &mut Vec<CoState>, shared: &Rc<RefCell<Shared>>) -> PipeResult<()> {
    loop {
        let pending = {
            let mut g = shared.borrow_mut();
            std::mem::take(&mut g.pending_spawns)
        };
        if pending.is_empty() {
            return Ok(());
        }
        let aborted = shared.borrow().aborted;
        if aborted {
            let mut g = shared.borrow_mut();
            for (id, _entry) in pending {
                if id.0 < g.stages.len() {
                    g.stages[id.0].ended = true;
                    g.mark_writer_done(id);
                }
            }
            continue;
        }
        for (id, entry) in pending {
            while cos.len() < id.0 {
                cos.push(CoState::Done);
            }
            if cos.len() == id.0 {
                cos.push(CoState::NotStarted(entry));
            } else {
                cos[id.0] = CoState::NotStarted(entry);
            }
            resume_stage(id.0, cos, shared)?;
        }
    }
}

fn still_sleeping(s: &StageState, stop_external: bool) -> bool {
    // PIPESTOP (p.458) posts the portable ECB stand-in: a waiter whose
    // deadline has not arrived must still be runnable so the dispatcher
    // does not `thread::sleep` until that deadline.
    if stop_external {
        return false;
    }
    match s.waiting_until {
        Some(t) => Instant::now() < t,
        None => false,
    }
}

fn nearest_sleep_deadline(shared: &Shared) -> Option<Instant> {
    if shared.external_stop {
        return None;
    }
    shared
        .stages
        .iter()
        .filter(|s| !s.ended)
        .filter_map(|s| s.waiting_until)
        .min()
}

/// Prefer runnable consumers (waiting read with data/eof available), then
/// writers whose slot is free, then commit waiters that are satisfied, then
/// wall-clock waiters whose deadline has arrived (DELAY; BEAT may also be
/// waiting_read so an arriving record wins via step 1), then any non-ended
/// stage that is not clearly blocked forever, then yielders.
fn pick_next(shared: &Shared) -> Option<usize> {
    // 1. Readers that can make progress (drain bias).
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        if let Some(stream) = s.waiting_read {
            if shared.input_can_progress(StageId(i), stream) {
                return Some(i);
            }
        }
        if !s.waiting_read_any.is_empty()
            && s.waiting_read_any
                .iter()
                .any(|&stream| shared.input_can_progress(StageId(i), stream))
        {
            return Some(i);
        }
    }
    // 2. Writers that can place a record.
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        if let Some(stream) = s.waiting_write {
            if let Some(c) = shared
                .connections
                .iter()
                .find(|c| c.writer == StageId(i) && c.writer_stream == stream)
            {
                // Severed: the writer must run to observe the failure.
                // Empty slot: the writer can place a record.
                if c.severed || c.slot.is_none() {
                    return Some(i);
                }
            } else {
                return Some(i);
            }
        }
    }
    // 3. Commit waiters that are satisfied.
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        if let Some(level) = s.waiting_commit {
            if shared.commit_satisfied(StageId(i), level) {
                return Some(i);
            }
        }
    }
    // 4. Wall-clock waiters whose deadline has arrived (or PIPESTOP posted
    // the portable ECB stand-in). DELAY only sets `waiting_until`; BEAT also
    // sets `waiting_read`, so without this step a quiet timeout would never
    // resume while input is still live.
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        if s.waiting_until.is_some() && !still_sleeping(s, shared.external_stop) {
            return Some(i);
        }
    }
    // 4b. CALLPIPE waiters whose nested stages have all ended (or the
    // network aborted). Must not fall through to step 5 as "ready".
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        if let Some(session) = &s.waiting_callpipe {
            if callpipe_children_done(shared, session) {
                return Some(i);
            }
        }
    }
    // 5. Stages not yet started / not waiting (ready).
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        if s.waiting_callpipe.is_some() {
            continue;
        }
        if s.waiting_read.is_none()
            && s.waiting_read_any.is_empty()
            && s.waiting_write.is_none()
            && s.waiting_commit.is_none()
            && !s.waiting_yield
            && !still_sleeping(s, shared.external_stop)
        {
            return Some(i);
        }
    }
    // 6. Yield waiters: resume only after every other currently-ready stage
    // has had a chance (ELASTIC listening-mode termination, 1.1.11 p.335).
    for (i, s) in shared.stages.iter().enumerate() {
        if !s.ended && s.waiting_yield {
            return Some(i);
        }
    }
    None
}

fn resume_stage(index: usize, cos: &mut [CoState], shared: &Rc<RefCell<Shared>>) -> PipeResult<()> {
    if shared.borrow().stages[index].ended {
        return Ok(());
    }

    let state = std::mem::replace(&mut cos[index], CoState::Done);
    match state {
        CoState::Done => Ok(()),
        CoState::NotStarted(entry) => {
            let shared_cl = Rc::clone(shared);
            let stage_id = StageId(index);
            let mut coroutine = Coroutine::with_stack(
                DefaultStack::default(),
                move |yielder: &Yielder<(), WaitReason>, ()| {
                    shared_cl.borrow_mut().stages[stage_id.0].yielder =
                        yielder as *const Yielder<(), WaitReason>;
                    {
                        let g = shared_cl.borrow();
                        g.trace_stage(stage_id, "start");
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut ctx = StageCtx::new(stage_id, Rc::clone(&shared_cl));
                        // Auto-commit to 0 before stage body (CMS-like default).
                        let _ = ctx.commit(0);
                        entry(&mut ctx);
                    }));
                    shared_cl.borrow_mut().stages[stage_id.0].yielder = std::ptr::null();
                    if let Err(payload) = result {
                        let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                            (*s).to_string()
                        } else if let Some(s) = payload.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "stage panicked".to_string()
                        };
                        let mut g = shared_cl.borrow_mut();
                        g.stages[stage_id.0].error = Some(PipeError::stage(format!(
                            "{}: {msg}",
                            g.stages[stage_id.0].name
                        )));
                        g.aborted = true;
                        g.trace_stage(stage_id, format!("error {msg}"));
                    }
                },
            );
            after_resume(index, coroutine.resume(()), coroutine, cos, shared)
        }
        CoState::Running(mut coroutine) => {
            after_resume(index, coroutine.resume(()), coroutine, cos, shared)
        }
    }
}

fn after_resume(
    index: usize,
    result: CoroutineResult<WaitReason, ()>,
    coroutine: Coroutine<(), WaitReason, ()>,
    cos: &mut [CoState],
    shared: &Rc<RefCell<Shared>>,
) -> PipeResult<()> {
    match result {
        CoroutineResult::Yield(_reason) => {
            cos[index] = CoState::Running(coroutine);
            Ok(())
        }
        CoroutineResult::Return(()) => {
            {
                let g = shared.borrow();
                g.trace_stage(StageId(index), "end");
            }
            let mut g = shared.borrow_mut();
            g.stages[index].ended = true;
            g.stages[index].waiting_read = None;
            g.stages[index].waiting_read_any.clear();
            g.stages[index].waiting_write = None;
            g.stages[index].waiting_commit = None;
            g.stages[index].waiting_yield = false;
            g.stages[index].waiting_until = None;
            g.stages[index].waiting_callpipe = None;
            g.mark_writer_done(StageId(index));
            cos[index] = CoState::Done;
            Ok(())
        }
    }
}

/// Run a nested pipeline for CALLPIPE from within a parent stage.
///
/// Nested stages are spliced into the parent's dispatcher. Connector
/// attachments loan the caller's live connections for the duration of the
/// call so records move one at a time. The caller yields
/// [`WaitReason::CallPipe`] until every nested stage ends, then the loans
/// are restored.
pub(crate) fn run_callpipe(
    parent: &mut StageCtx,
    build: impl FnOnce(&mut PipelineBuilder),
    connectors: CallPipeConnectors,
) -> PipeResult<()> {
    let nest_trace = parent.shared.borrow().trace.clone();
    parent
        .shared
        .borrow()
        .trace_stage(parent.stage_id, "CALLPIPE enter");

    let mut builder = PipelineBuilder::new().with_trace(nest_trace);
    build(&mut builder);

    let result = run_callpipe_loaned(parent, builder, connectors);

    parent
        .shared
        .borrow()
        .trace_stage(parent.stage_id, "CALLPIPE exit");
    result
}

fn run_callpipe_loaned(
    parent: &mut StageCtx,
    builder: PipelineBuilder,
    connectors: CallPipeConnectors,
) -> PipeResult<()> {
    if parent.shared.borrow().aborted {
        return Err(PipeError::Aborted);
    }

    let group = {
        let mut g = parent.shared.borrow_mut();
        g.next_commit_group = g.next_commit_group.saturating_add(1);
        g.next_commit_group
    };

    let children = spawn_graph(&parent.shared, builder, group);
    apply_loans(
        &parent.shared,
        parent.stage_id,
        &connectors.attachments,
        &children,
    );

    if children.is_empty() {
        return Ok(());
    }

    {
        let mut g = parent.shared.borrow_mut();
        g.stages[parent.stage_id.0].waiting_callpipe = Some(CallPipeSession {
            children: children.clone(),
            processing: connectors.processing.clone(),
        });
        g.trace_stage(
            parent.stage_id,
            format!(
                "CALLPIPE wait ({} nested stages, {} loans, commit group {group})",
                children.len(),
                connectors.attachments.len()
            ),
        );
    }

    loop {
        let done = {
            let g = parent.shared.borrow();
            if g.aborted {
                true
            } else {
                children.iter().all(|id| g.stages[id.0].ended)
            }
        };
        if done {
            break;
        }
        crate::stage::suspend_callpipe(&parent.shared, parent.stage_id);
    }

    restore_loans(&parent.shared, parent.stage_id);
    {
        let mut g = parent.shared.borrow_mut();
        g.stages[parent.stage_id.0].waiting_callpipe = None;
    }

    let child_err = {
        let g = parent.shared.borrow();
        children.iter().find_map(|id| g.stages[id.0].error.clone())
    };
    if let Some(err) = child_err {
        return Err(err);
    }
    if parent.shared.borrow().aborted {
        return Err(PipeError::Aborted);
    }
    Ok(())
}

fn spawn_graph(
    shared: &Rc<RefCell<Shared>>,
    builder: PipelineBuilder,
    commit_group: u32,
) -> Vec<StageId> {
    let (stages, conns) = builder.into_parts();
    let mut g = shared.borrow_mut();
    let offset = g.stages.len();
    let mut ids = Vec::with_capacity(stages.len());
    for spec in stages {
        let id = StageId(g.stages.len());
        let mut state = Shared::new_stage_state(spec.name, spec.origin);
        state.commit_group = commit_group;
        g.stages.push(state);
        g.pending_spawns.push((id, spec.entry));
        ids.push(id);
    }
    for c in conns {
        g.connections.push(Connection::new(
            StageId(c.writer.0 + offset),
            c.writer_stream,
            StageId(c.reader.0 + offset),
            c.reader_stream,
        ));
    }
    ids
}

fn apply_loans(
    shared: &Rc<RefCell<Shared>>,
    parent: StageId,
    attachments: &[CallPipeAttachment],
    children: &[StageId],
) {
    let mut g = shared.borrow_mut();
    for a in attachments {
        let Some(&nested) = children.get(a.nested_stage) else {
            continue;
        };
        let loan = StreamLoan {
            parent,
            parent_stream: a.caller_stream,
            side: a.side,
        };
        match a.side {
            LoanSide::Input => {
                for c in &mut g.connections {
                    if c.reader == parent && c.reader_stream == a.caller_stream && !c.severed {
                        c.reader = nested;
                        c.reader_stream = a.nested_stream;
                        c.loan_stack.push(loan);
                        break;
                    }
                }
            }
            LoanSide::Output => {
                for c in &mut g.connections {
                    if c.writer == parent && c.writer_stream == a.caller_stream && !c.severed {
                        c.writer = nested;
                        c.writer_stream = a.nested_stream;
                        c.loan_stack.push(loan);
                        break;
                    }
                }
            }
        }
    }
}

fn restore_loans(shared: &Rc<RefCell<Shared>>, parent: StageId) {
    let mut g = shared.borrow_mut();
    for c in &mut g.connections {
        let Some(top) = c.loan_stack.last() else {
            continue;
        };
        if top.parent != parent {
            continue;
        }
        let loan = c.loan_stack.pop().expect("stack was not empty");
        match loan.side {
            LoanSide::Input => {
                c.reader = loan.parent;
                c.reader_stream = loan.parent_stream;
            }
            LoanSide::Output => {
                c.writer = loan.parent;
                c.writer_stream = loan.parent_stream;
            }
        }
    }
}

impl PipelineBuilder {
    /// Append another builder's stages and connections (CALLPIPE nested body).
    pub fn extend_from(&mut self, other: PipelineBuilder) {
        let (stages, conns) = other.into_parts();
        self.append_stages(stages, conns);
    }

    pub(crate) fn into_parts(self) -> (Vec<StageSpec>, Vec<ConnectionSpec>) {
        (self.stages, self.connections)
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Nested stages whose primary output is not already connected.
    /// PREFACE / APPEND drain CALLPIPE onto these.
    pub fn primary_output_sinks(&self) -> Vec<StageId> {
        self.stages_without_primary_output(0, self.stages.len())
    }

    pub(crate) fn append_stages(&mut self, stages: Vec<StageSpec>, conns: Vec<ConnectionSpec>) {
        let offset = self.stages.len();
        self.stages.extend(stages);
        for mut c in conns {
            c.writer = StageId(c.writer.0 + offset);
            c.reader = StageId(c.reader.0 + offset);
            self.connections.push(c);
        }
    }

    pub(crate) fn stages_without_primary_output(&self, start: usize, end: usize) -> Vec<StageId> {
        (start..end)
            .filter(|i| {
                !self
                    .connections
                    .iter()
                    .any(|c| c.writer == StageId(*i) && c.writer_stream == StreamId::PRIMARY)
            })
            .map(StageId)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StageOrigin;
    use crate::stage::StageFn;
    use crate::trace::TraceConfig;
    use crate::Record;

    #[test]
    fn trace_captures_read_write_lifecycle() {
        let buf = Rc::new(RefCell::new(Vec::new()));
        let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&buf)));
        let a = b.add_stage(
            "literal",
            Box::new(|ctx| {
                ctx.write(Record::from("hi")).unwrap();
            }),
        );
        let c = b.add_stage(
            "sink",
            Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {}),
        );
        b.connect_primary(a, c);
        b.run().unwrap();

        let lines = buf.borrow().clone();
        let joined = lines.join("\n");
        assert!(joined.contains("dispatcher start"), "{joined}");
        assert!(joined.contains("stage[0] literal start"), "{joined}");
        assert!(joined.contains("WRITE stream=0 len=2"), "{joined}");
        assert!(joined.contains("READ stream=0 len=2"), "{joined}");
        assert!(joined.contains("READ stream=0 EOF"), "{joined}");
        assert!(joined.contains("stage[0] literal end"), "{joined}");
        assert!(joined.contains("dispatcher end"), "{joined}");
    }

    #[test]
    fn linear_literal_to_collector() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);

        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "literal",
            Box::new(|ctx| {
                ctx.write(Record::from("one")).unwrap();
                ctx.write(Record::from("two")).unwrap();
            }),
        );
        let c = b.add_stage(
            "collect",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(a, c);
        b.run().unwrap();

        assert_eq!(
            *collected.borrow(),
            vec!["one".to_string(), "two".to_string()]
        );
    }

    #[test]
    fn transform_middle_stage() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);

        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "src",
            Box::new(|ctx| {
                ctx.write(Record::from("a")).unwrap();
                ctx.write(Record::from("b")).unwrap();
            }),
        );
        let m = b.add_stage(
            "upper",
            Box::new(|ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    let s = r.to_string_lossy().to_uppercase();
                    ctx.write(Record::from(s)).unwrap();
                }
            }),
        );
        let c = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(a, m);
        b.connect_primary(m, c);
        b.run().unwrap();
        assert_eq!(*collected.borrow(), vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn null_ended_input_eof() {
        let collected = Rc::new(RefCell::new(0usize));
        let n = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        b.add_stage(
            "only",
            Box::new(move |ctx| {
                assert!(ctx.read().unwrap().is_none());
                *n.borrow_mut() = 1;
            }),
        );
        b.run().unwrap();
        assert_eq!(*collected.borrow(), 1);
    }

    #[test]
    fn stage_fail_returns_error_without_refcell_panic() {
        let mut b = PipelineBuilder::new();
        let a = b.add_stage(
            "boom",
            Box::new(|ctx| {
                ctx.fail("deliberate stage failure");
            }),
        );
        let c = b.add_stage(
            "sink",
            Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {}),
        );
        b.connect_primary(a, c);
        let err = b.run().expect_err("stage fail should surface as Err");
        assert!(
            err.to_string().contains("deliberate stage failure"),
            "err={err}"
        );
    }

    #[test]
    fn stage_fail_appends_cms_origin() {
        let mut b = PipelineBuilder::new();
        let origin = StageOrigin {
            pipeline: 1,
            stage: 2,
            running: "boom x".into(),
            name: None,
        };
        let a = b.add_stage_with_origin(
            "boom",
            Box::new(|ctx| {
                ctx.fail("deliberate stage failure");
            }),
            Some(origin),
        );
        let c = b.add_stage(
            "sink",
            Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {}),
        );
        b.connect_primary(a, c);
        let err = b.run().expect_err("stage fail should surface as Err");
        assert_eq!(
            err.to_string(),
            "deliberate stage failure\n\
             ... Issued from stage 2 of pipeline 1\n\
             ... Running \"boom x\""
        );
    }

    #[test]
    fn stall_message_is_plain_words() {
        // Two records through FANOUT into FANIN without a buffer: stall.
        use crate::stage::StageFn;
        let emit: StageFn = Box::new(|ctx| {
            let _ = ctx.write(Record::from("a"));
            let _ = ctx.write(Record::from("b"));
        });
        let fanout: StageFn = Box::new(|ctx| {
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r.clone()).is_err() {
                    break;
                }
                ctx.select_output(StreamId::SECONDARY);
                if ctx.write(r).is_err() {
                    break;
                }
                ctx.select_output(StreamId::PRIMARY);
            }
        });
        let fanin: StageFn = Box::new(|ctx| {
            ctx.select_input(StreamId::PRIMARY);
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r).is_err() {
                    return;
                }
            }
            ctx.select_input(StreamId::SECONDARY);
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r).is_err() {
                    return;
                }
            }
        });
        let sink: StageFn = Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {});
        let mut b = PipelineBuilder::new();
        let src = b.add_stage("src", emit);
        let fo = b.add_stage("FANOUT", fanout);
        let fi = b.add_stage("FANIN", fanin);
        let snk = b.add_stage("CONSOLE", sink);
        b.connect_primary(src, fo);
        b.connect(fo, StreamId::PRIMARY, fi, StreamId::PRIMARY);
        b.connect(fo, StreamId::SECONDARY, fi, StreamId::SECONDARY);
        b.connect_primary(fi, snk);
        let err = b.run().expect_err("expected stall");
        let msg = err.to_string();
        assert!(
            msg.contains("pipeline stalled (no stage can make progress)"),
            "{msg}"
        );
        assert!(
            msg.contains("waiting to write") || msg.contains("waiting to read"),
            "{msg}"
        );
        assert!(msg.contains("Hint:"), "{msg}");
        assert!(
            !msg.contains("read=None") && !msg.contains("write=Some"),
            "raw dump leaked into the stall message: {msg}"
        );
    }

    #[test]
    fn stall_trace_emits_raw_dump() {
        use crate::stage::StageFn;
        let emit: StageFn = Box::new(|ctx| {
            let _ = ctx.write(Record::from("a"));
            let _ = ctx.write(Record::from("b"));
        });
        let fanout: StageFn = Box::new(|ctx| {
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r.clone()).is_err() {
                    break;
                }
                ctx.select_output(StreamId::SECONDARY);
                if ctx.write(r).is_err() {
                    break;
                }
                ctx.select_output(StreamId::PRIMARY);
            }
        });
        let fanin: StageFn = Box::new(|ctx| {
            ctx.select_input(StreamId::PRIMARY);
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r).is_err() {
                    return;
                }
            }
            ctx.select_input(StreamId::SECONDARY);
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r).is_err() {
                    return;
                }
            }
        });
        let sink: StageFn = Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {});
        let buf = Rc::new(RefCell::new(Vec::new()));
        let mut b = PipelineBuilder::new().with_trace(TraceConfig::capture(Rc::clone(&buf)));
        let src = b.add_stage("src", emit);
        let fo = b.add_stage("FANOUT", fanout);
        let fi = b.add_stage("FANIN", fanin);
        let snk = b.add_stage("CONSOLE", sink);
        b.connect_primary(src, fo);
        b.connect(fo, StreamId::PRIMARY, fi, StreamId::PRIMARY);
        b.connect(fo, StreamId::SECONDARY, fi, StreamId::SECONDARY);
        b.connect_primary(fi, snk);
        let err = b.run().expect_err("expected stall");
        let msg = err.to_string();
        assert!(
            msg.contains("pipeline stalled (no stage can make progress)"),
            "{msg}"
        );
        assert!(
            !msg.contains("read=None") && !msg.contains("write=Some"),
            "raw dump leaked into the stall message: {msg}"
        );
        let joined = buf.borrow().join("\n");
        assert!(
            joined.contains("stall dump:") && joined.contains("read="),
            "expected raw dump under trace: {joined}"
        );
    }

    #[test]
    fn spawn_stage_splices_into_running_dispatcher() {
        // IF-style ADDPIPE: a gateway spawns a copier and retargets its
        // streams so the copier sees the original connections.
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("a"));
                let _ = ctx.write(Record::from("b"));
            }),
        );
        let gate = b.add_stage(
            "gate",
            Box::new(|ctx| {
                let copier = ctx.spawn_stage(
                    "copy",
                    Box::new(|ctx| {
                        while let Ok(Some(r)) = ctx.read() {
                            if ctx.write(r).is_err() {
                                break;
                            }
                        }
                    }),
                );
                ctx.retarget_input(StreamId::PRIMARY, copier, StreamId::PRIMARY);
                ctx.retarget_output(StreamId::PRIMARY, copier, StreamId::PRIMARY);
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(src, gate);
        b.connect_primary(gate, sink);
        b.run().unwrap();
        assert_eq!(*collected.borrow(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn wait_output_resumes_after_the_consumer_takes_the_record() {
        // EOFBACK (p.337): after WRITE fills the slot, wait_output must not
        // proceed until the consumer has read (or the stream severs).
        let events = Rc::new(RefCell::new(Vec::new()));
        let writer_log = Rc::clone(&events);
        let reader_log = Rc::clone(&events);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(move |ctx| {
                let _ = ctx.write(Record::from("only"));
                writer_log.borrow_mut().push("wrote".into());
                let _ = ctx.wait_output();
                writer_log.borrow_mut().push("consumed".into());
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                reader_log.borrow_mut().push("before-read".into());
                let got = ctx.read().ok().flatten().map(|r| r.to_string_lossy());
                reader_log.borrow_mut().push(format!("read:{got:?}"));
            }),
        );
        b.connect_primary(src, sink);
        b.run().unwrap();
        let got = events.borrow().clone();
        let wrote = got.iter().position(|e| e == "wrote").expect("wrote");
        let read = got
            .iter()
            .position(|e| e.starts_with("read:"))
            .expect("read");
        let consumed = got.iter().position(|e| e == "consumed").expect("consumed");
        assert_eq!(got[read], "read:Some(\"only\")", "{got:?}");
        assert!(
            wrote < consumed && read < consumed,
            "wait_output must resume only after the consumer reads; {got:?}"
        );
    }

    #[test]
    fn wait_until_does_not_freeze_other_stages() {
        // DELAY (p.321): a wall-clock wait must suspend only this stage so a
        // sibling pipeline can finish while the waiter is parked.
        use std::time::{Duration, Instant};

        let fast_at = Rc::new(RefCell::new(None));
        let slow_at = Rc::new(RefCell::new(None));
        let fast_sink = Rc::clone(&fast_at);
        let slow_sink = Rc::clone(&slow_at);
        let mut b = PipelineBuilder::new();
        let slow_src = b.add_stage(
            "slow_src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("slow"));
            }),
        );
        let slow = b.add_stage(
            "slow",
            Box::new(move |ctx| {
                let rec = ctx.read().ok().flatten();
                let _ = ctx.wait_until(Instant::now() + Duration::from_millis(40));
                if let Some(r) = rec {
                    let _ = ctx.write(r);
                }
            }),
        );
        let slow_out = b.add_stage(
            "slow_out",
            Box::new(move |ctx| {
                if ctx.read().ok().flatten().is_some() {
                    *slow_sink.borrow_mut() = Some(Instant::now());
                }
            }),
        );
        let fast_src = b.add_stage(
            "fast_src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("fast"));
            }),
        );
        let fast_out = b.add_stage(
            "fast_out",
            Box::new(move |ctx| {
                if ctx.read().ok().flatten().is_some() {
                    *fast_sink.borrow_mut() = Some(Instant::now());
                }
            }),
        );
        b.connect_primary(slow_src, slow);
        b.connect_primary(slow, slow_out);
        b.connect_primary(fast_src, fast_out);
        b.run().unwrap();
        let fast = fast_at.borrow().expect("fast");
        let slow = slow_at.borrow().expect("slow");
        assert!(
            slow.saturating_duration_since(fast) >= Duration::from_millis(15),
            "fast path must complete while DELAY is waiting; lag={:?}",
            slow.saturating_duration_since(fast)
        );
    }

    #[test]
    fn wait_until_or_input_wakes_on_a_record_before_the_deadline() {
        // BEAT (p.271): a quiet-interval wait must copy an arriving record
        // without waiting out the rest of the interval.
        use std::time::{Duration, Instant};

        let copied_at = Rc::new(RefCell::new(None));
        let sink_at = Rc::clone(&copied_at);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.wait_until(Instant::now() + Duration::from_millis(15));
                let _ = ctx.write(Record::from("late"));
            }),
        );
        let beat = b.add_stage(
            "beat",
            Box::new(|ctx| {
                let input = ctx.wait_until_or_input(
                    Instant::now() + Duration::from_millis(120),
                    StreamId::PRIMARY,
                );
                assert!(
                    matches!(input, Ok(true)),
                    "record must win over the deadline: {input:?}"
                );
                if let Ok(Some(r)) = ctx.read() {
                    let _ = ctx.write(r);
                }
            }),
        );
        let snk = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                if ctx.read().ok().flatten().is_some() {
                    *sink_at.borrow_mut() = Some(Instant::now());
                }
            }),
        );
        b.connect_primary(src, beat);
        b.connect_primary(beat, snk);
        let start = Instant::now();
        b.run().unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(80),
            "arriving record must not wait out the 120ms interval; elapsed={elapsed:?}"
        );
        assert!(copied_at.borrow().is_some());
    }

    #[test]
    fn stop_external_waits_wakes_wait_until_before_the_deadline() {
        // PIPESTOP (pp.458–459): posting the portable ECB stand-in must
        // terminate a DELAY-style wait without sleeping until the deadline,
        // and must not abort a sibling that is only copying records (p.150).
        use std::time::{Duration, Instant};

        let copied = Rc::new(RefCell::new(Vec::new()));
        let delay_wrote = Rc::new(RefCell::new(false));
        let out = Rc::clone(&copied);
        let delay_flag = Rc::clone(&delay_wrote);
        let mut b = PipelineBuilder::new();
        let delay_src = b.add_stage(
            "delay_src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("slow"));
            }),
        );
        let delay = b.add_stage(
            "delay",
            Box::new(move |ctx| {
                let rec = ctx.read().ok().flatten();
                if ctx
                    .wait_until(Instant::now() + Duration::from_millis(400))
                    .is_err()
                {
                    return;
                }
                if rec.is_some() {
                    *delay_flag.borrow_mut() = true;
                    let _ = ctx.write(Record::from("late"));
                }
            }),
        );
        let delay_sink = b.add_stage(
            "delay_sink",
            Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {}),
        );
        let copy_src = b.add_stage(
            "copy_src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("run"));
            }),
        );
        let copy_sink = b.add_stage(
            "copy_sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        let stop_src = b.add_stage(
            "stop_src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("go"));
            }),
        );
        let stop = b.add_stage(
            "pipestop",
            Box::new(|ctx| {
                if let Ok(Some(r)) = ctx.read() {
                    ctx.stop_external_waits();
                    let _ = ctx.write(r);
                }
            }),
        );
        let stop_sink = b.add_stage(
            "stop_sink",
            Box::new(move |ctx| while let Ok(Some(_)) = ctx.read() {}),
        );
        b.connect_primary(delay_src, delay);
        b.connect_primary(delay, delay_sink);
        b.connect_primary(copy_src, copy_sink);
        b.connect_primary(stop_src, stop);
        b.connect_primary(stop, stop_sink);
        let start = Instant::now();
        b.run().unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(200),
            "PIPESTOP must not wait out the 400ms delay; elapsed={elapsed:?}"
        );
        assert_eq!(*copied.borrow(), vec!["run".to_string()]);
        assert!(
            !*delay_wrote.borrow(),
            "DELAY must terminate without writing the waited record"
        );
    }

    fn copier() -> StageFn {
        Box::new(|ctx| {
            while let Ok(Some(r)) = ctx.read() {
                if ctx.write(r).is_err() {
                    break;
                }
            }
        })
    }

    fn both_loans() -> CallPipeConnectors {
        CallPipeConnectors {
            attachments: vec![
                CallPipeAttachment {
                    side: LoanSide::Input,
                    caller_stream: StreamId::PRIMARY,
                    nested_stage: 0,
                    nested_stream: StreamId::PRIMARY,
                },
                CallPipeAttachment {
                    side: LoanSide::Output,
                    caller_stream: StreamId::PRIMARY,
                    nested_stage: 0,
                    nested_stream: StreamId::PRIMARY,
                },
            ],
            processing: None,
        }
    }

    #[test]
    fn callpipe_loans_stream_and_restore_allows_a_footer() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("a"));
                let _ = ctx.write(Record::from("b"));
            }),
        );
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.callpipe(
                    |pb| {
                        let _ = pb.add_stage("copy", copier());
                    },
                    both_loans(),
                )
                .unwrap();
                let _ = ctx.write(Record::from("footer"));
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(src, caller);
        b.connect_primary(caller, sink);
        b.run().unwrap();
        assert_eq!(
            *collected.borrow(),
            vec!["a".to_string(), "b".to_string(), "footer".to_string()]
        );
    }

    #[test]
    fn callpipe_loan_keeps_a_peeked_slot() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("hello"));
            }),
        );
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                let peeked = ctx.peek().map(|r| r.to_string_lossy());
                assert_eq!(peeked.as_deref(), Some("hello"));
                ctx.callpipe(
                    |pb| {
                        let _ = pb.add_stage("copy", copier());
                    },
                    both_loans(),
                )
                .unwrap();
                assert!(
                    ctx.peek().is_none(),
                    "nested copy must consume the peeked record"
                );
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(src, caller);
        b.connect_primary(caller, sink);
        b.run().unwrap();
        assert_eq!(*collected.borrow(), vec!["hello".to_string()]);
    }

    #[test]
    fn callpipe_loan_leftover_unread_record() {
        let leftover = Rc::new(RefCell::new(Vec::new()));
        let left = Rc::clone(&leftover);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from(".end"));
                let _ = ctx.write(Record::from("after"));
            }),
        );
        let caller = b.add_stage(
            "caller",
            Box::new(move |ctx| {
                ctx.callpipe(
                    |pb| {
                        let _ = pb.add_stage(
                            "peek",
                            Box::new(|c| {
                                assert!(c.input_connected(StreamId::PRIMARY), "input loan missing");
                                loop {
                                    if let Some(r) = c.peek() {
                                        assert_eq!(r.to_string_lossy(), ".end");
                                        break;
                                    }
                                    if c.input_at_eof(StreamId::PRIMARY) {
                                        panic!("eof before leftover");
                                    }
                                    c.wait_any_input(&[StreamId::PRIMARY])
                                        .expect("wait for leftover");
                                }
                            }),
                        );
                    },
                    CallPipeConnectors {
                        attachments: vec![CallPipeAttachment {
                            side: LoanSide::Input,
                            caller_stream: StreamId::PRIMARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        }],
                        processing: None,
                    },
                )
                .unwrap();
                while let Ok(Some(r)) = ctx.read() {
                    left.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(src, caller);
        b.run().unwrap();
        assert_eq!(
            *leftover.borrow(),
            vec![".end".to_string(), "after".to_string()]
        );
    }

    #[test]
    fn callpipe_nested_commit_does_not_wait_on_caller() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.callpipe(
                    |pb| {
                        let _ = pb.add_stage(
                            "nested",
                            Box::new(|c| {
                                c.commit(1)
                                    .expect("nested COMMIT 1 must not stall on caller");
                                let _ = c.write(Record::from("ok"));
                            }),
                        );
                    },
                    CallPipeConnectors {
                        attachments: vec![CallPipeAttachment {
                            side: LoanSide::Output,
                            caller_stream: StreamId::PRIMARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        }],
                        processing: None,
                    },
                )
                .unwrap();
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(caller, sink);
        b.run().unwrap();
        assert_eq!(*collected.borrow(), vec!["ok".to_string()]);
    }

    #[test]
    fn callpipe_nested_short_output_does_not_kill_caller() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.callpipe(
                    |pb| {
                        let _ = pb.add_stage(
                            "short",
                            Box::new(|c| {
                                let _ = c.write(Record::from("nested"));
                                c.sever_output();
                            }),
                        );
                    },
                    CallPipeConnectors {
                        attachments: vec![CallPipeAttachment {
                            side: LoanSide::Output,
                            caller_stream: StreamId::PRIMARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        }],
                        processing: None,
                    },
                )
                .unwrap();
                ctx.write(Record::from("after"))
                    .expect("caller output must survive nested SHORT");
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(caller, sink);
        b.run().unwrap();
        assert_eq!(
            *collected.borrow(),
            vec!["nested".to_string(), "after".to_string()]
        );
    }

    #[test]
    fn nested_callpipe_restore_allows_outer_footer() {
        // Inner CALLPIPE must push a loan frame, not overwrite the outer
        // owner. After both restores the original caller can still write.
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("ab"));
            }),
        );
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.callpipe(
                    |pb| {
                        let _ = pb.add_stage(
                            "inner",
                            Box::new(|c| {
                                c.callpipe(
                                    |inner| {
                                        let _ = inner.add_stage("copy", copier());
                                    },
                                    both_loans(),
                                )
                                .unwrap();
                            }),
                        );
                    },
                    both_loans(),
                )
                .unwrap();
                ctx.write(Record::from("footer"))
                    .expect("outer caller output must survive nested CALLPIPE");
            }),
        );
        let sink = b.add_stage(
            "sink",
            Box::new(move |ctx| {
                while let Ok(Some(r)) = ctx.read() {
                    out.borrow_mut().push(r.to_string_lossy());
                }
            }),
        );
        b.connect_primary(src, caller);
        b.connect_primary(caller, sink);
        b.run().unwrap();
        assert_eq!(
            *collected.borrow(),
            vec!["ab".to_string(), "footer".to_string()]
        );
    }

    #[test]
    fn callpipe_nested_waiting_on_caller_stalls() {
        // Nested stage reads a stream only the parked caller would write.
        let mut b = PipelineBuilder::new();
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                let _ = ctx.callpipe(
                    |pb| {
                        let _ = pb.add_stage(
                            "nested",
                            Box::new(|c| {
                                let _ = c.read();
                            }),
                        );
                    },
                    CallPipeConnectors {
                        attachments: vec![CallPipeAttachment {
                            side: LoanSide::Input,
                            caller_stream: StreamId::SECONDARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                        }],
                        processing: None,
                    },
                );
                ctx.select_output(StreamId::SECONDARY);
                let _ = ctx.write(Record::from("too late"));
            }),
        );
        b.connect(caller, StreamId::SECONDARY, caller, StreamId::SECONDARY);
        let err = b.run().expect_err("expected stall");
        let msg = err.to_string();
        assert!(
            msg.contains("pipeline stalled (no stage can make progress)"),
            "{msg}"
        );
        assert!(msg.contains("waiting for CALLPIPE"), "{msg}");
    }

    #[test]
    fn callpipe_internal_stall_lists_waiting_for_callpipe() {
        let mut b = PipelineBuilder::new();
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                let _ = ctx.callpipe(
                    |pb| {
                        let emit: StageFn = Box::new(|ctx| {
                            let _ = ctx.write(Record::from("a"));
                            let _ = ctx.write(Record::from("b"));
                        });
                        let fanout: StageFn = Box::new(|ctx| {
                            while let Ok(Some(r)) = ctx.read() {
                                if ctx.write(r.clone()).is_err() {
                                    break;
                                }
                                ctx.select_output(StreamId::SECONDARY);
                                if ctx.write(r).is_err() {
                                    break;
                                }
                                ctx.select_output(StreamId::PRIMARY);
                            }
                        });
                        let fanin: StageFn = Box::new(|ctx| {
                            ctx.select_input(StreamId::PRIMARY);
                            while let Ok(Some(r)) = ctx.read() {
                                if ctx.write(r).is_err() {
                                    return;
                                }
                            }
                            ctx.select_input(StreamId::SECONDARY);
                            while let Ok(Some(r)) = ctx.read() {
                                if ctx.write(r).is_err() {
                                    return;
                                }
                            }
                        });
                        let sink: StageFn = Box::new(|ctx| while let Ok(Some(_)) = ctx.read() {});
                        let src = pb.add_stage("src", emit);
                        let fo = pb.add_stage("FANOUT", fanout);
                        let fi = pb.add_stage("FANIN", fanin);
                        let snk = pb.add_stage("CONSOLE", sink);
                        pb.connect_primary(src, fo);
                        pb.connect(fo, StreamId::PRIMARY, fi, StreamId::PRIMARY);
                        pb.connect(fo, StreamId::SECONDARY, fi, StreamId::SECONDARY);
                        pb.connect_primary(fi, snk);
                    },
                    CallPipeConnectors::default(),
                );
            }),
        );
        let _ = caller;
        let err = b.run().expect_err("expected stall");
        let msg = err.to_string();
        assert!(
            msg.contains("pipeline stalled (no stage can make progress)"),
            "{msg}"
        );
        assert!(msg.contains("waiting for CALLPIPE"), "{msg}");
    }
}
