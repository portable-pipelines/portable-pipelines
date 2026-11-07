use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Longest host-thread sleep while only DELAY/BEAT waiters remain.
///
/// `thread::sleep(remaining)` on macOS (and some VMs) can overshoot a
/// 15ms deadline by 100ms or more. That misses a 10ms BEAT interval and
/// makes a 15ms producer look like it waited out a 120ms quiet window.
/// Short slices let the dispatcher re-check the nearest deadline.
const SLEEP_SLICE: Duration = Duration::from_millis(1);

use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, Yielder};

use crate::debug::{
    DebugConfig, DebugCtl, DebugEvent, DebugEventKind, DebugPolicy, DebugRemote, LiveStage,
    RecordPreview, SessionStatus, SlotSnapshot, StepOutcome,
};
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

/// One ADDPIPE connector: insert a nested pipeline on a caller stream.
///
/// For an input connector, `nested_stage` is the first nested stage
/// (upstream is retargeted onto it) and `extra_stage` is the last
/// (a new connection writes into the caller). For an output connector
/// those roles reverse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddPipeAttachment {
    pub side: LoanSide,
    /// Stream on the caller (selected stream, `*.input.n:`, or a name).
    pub caller_stream: StreamId,
    /// Nested stage that takes the existing connection.
    pub nested_stage: usize,
    pub nested_stream: StreamId,
    /// Nested stage for the extra connection that keeps the caller attached.
    pub extra_stage: usize,
    pub extra_stream: StreamId,
}

/// How ADDPIPE connectors insert nested stages onto the caller's path.
#[derive(Debug, Clone, Default)]
pub struct AddPipeConnectors {
    pub attachments: Vec<AddPipeAttachment>,
    /// Optional `addpipe …` text for nested errors (message 2).
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
    /// True when the reader has peeked the slot without consuming it.
    pub reader_peeked: bool,
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
            reader_peeked: false,
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
    /// Waiting for the next framed command from an out-of-process helper.
    pub waiting_helper: bool,
    /// Parked at a debug hold. `pick_next` skips these while the controller is holding.
    pub waiting_debug: bool,
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
    /// Present only for [`DebugSession`]. Ordinary `Dispatcher::run` leaves this `None`.
    pub debug: Option<Rc<RefCell<DebugCtl>>>,
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

    pub(crate) fn emit_debug(&mut self, id: StageId, kind: DebugEventKind) {
        let Some(dbg) = self.debug.clone() else {
            return;
        };
        let name = self.stages[id.0].name.clone();
        dbg.borrow_mut().push(DebugEvent {
            stage: id,
            name,
            kind,
        });
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
            c.reader_peeked = false;
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
            self.connections[idx].reader_peeked = false;
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

    /// Sever remaining inputs of a stage that has just ended.
    ///
    /// CMS tears down every stream when a stage ends. Downstream already sees
    /// EOF via [`Self::mark_writer_done`]. Upstream writers still parked on
    /// the one-record slot need the input side SHORT so OUTPUT returns RC 12
    /// and they can stop. Nested stages that were writing toward this input
    /// (ADDPIPE insert-between) then end and sever their own inputs in turn.
    ///
    /// A CALLPIPE loan is left alone: restore still needs leftover records
    /// (TOLABEL). ADDPIPE retargets rather than loans, so those connections
    /// are severed.
    pub(crate) fn sever_remaining_inputs(&mut self, stage: StageId) {
        for c in &mut self.connections {
            if c.reader != stage || c.severed {
                continue;
            }
            if c.is_input_loaned() {
                continue;
            }
            c.severed = true;
            c.slot = None;
            c.reader_peeked = false;
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
            waiting_helper: false,
            waiting_debug: false,
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

    /// Build a debug session without starting any stage coroutine.
    pub fn into_debug_session(self, config: DebugConfig) -> DebugSession {
        DebugSession::from_parts(self.stages, self.connections, self.trace, Some(config))
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

/// Headless debugger over a live dispatcher.
///
/// Construction does not start coroutines and does not run the network.
/// Call [`Self::run`] or [`Self::step_stage`] to start. [`Self::stop`] and
/// `Drop` drain parked stacks only after a start.
///
/// Record cycles are dispatcher I/O observations on one selected stage, not
/// a proven 1-in-1-out mapping. FANOUT can offer several writes in one cycle.
/// SORT ingest is a read with no write. A LOCATE miss is a discarded write.
/// CALLPIPE returns [`StepOutcome::BlockedOnCallPipe`]; step the nested
/// `StageId`s. RUNPIPE stays a single stage: nested PIPE text is data.
///
/// Pause and step take effect at the next `StageCtx` read, write, or short.
/// A long CPU loop, a blocking host read (CONSOLE), or a host file syscall
/// already in flight is not interrupted. DELAY sleeps in 1ms dispatcher
/// slices; a pause request is seen between slices. `stop` does not undo
/// bytes already written to a host file.
pub struct DebugSession {
    shared: Rc<RefCell<Shared>>,
    cos: Vec<CoState>,
    idle_passes: u32,
    kicked: bool,
    status: SessionStatus,
    remote: DebugRemote,
}

impl DebugSession {
    pub(crate) fn from_parts(
        stages: Vec<StageSpec>,
        connections: Vec<ConnectionSpec>,
        trace: TraceConfig,
        debug: Option<DebugConfig>,
    ) -> Self {
        if trace.is_enabled() {
            trace.emit(format!(
                "dispatcher start ({} stages, {} connections)",
                stages.len(),
                connections.len()
            ));
        }
        let debug = debug.map(|c| Rc::new(RefCell::new(DebugCtl::new(c))));
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
            network_commit: crate::COMMIT_INITIAL,
            trace,
            pending_spawns: Vec::new(),
            next_commit_group: 0,
            debug,
        }));
        let cos: Vec<CoState> = stages
            .into_iter()
            .map(|s| CoState::NotStarted(s.entry))
            .collect();
        Self {
            shared,
            cos,
            idle_passes: 0,
            kicked: false,
            status: SessionStatus::Idle,
            remote: DebugRemote::new(),
        }
    }

    /// Handle the UI thread may clone. Setting flags does not move this session.
    pub fn remote(&self) -> DebugRemote {
        self.remote.clone()
    }

    pub fn status(&self) -> SessionStatus {
        self.status.clone()
    }

    pub fn events(&self) -> Vec<DebugEvent> {
        let g = self.shared.borrow();
        match &g.debug {
            Some(d) => d.borrow().events.iter().cloned().collect(),
            None => Vec::new(),
        }
    }

    pub fn live_stages(&self) -> Vec<LiveStage> {
        self.shared
            .borrow()
            .stages
            .iter()
            .enumerate()
            .map(|(i, s)| LiveStage {
                id: StageId(i),
                name: s.name.clone(),
                ended: s.ended,
                return_code: s.return_code,
                waiting_debug: s.waiting_debug,
                waiting_callpipe: s.waiting_callpipe.is_some(),
                waiting_helper: s.waiting_helper,
                commit_group: s.commit_group,
            })
            .collect()
    }

    pub fn slots(&self) -> Vec<SlotSnapshot> {
        let g = self.shared.borrow();
        let max_bytes = g
            .debug
            .as_ref()
            .map(|d| d.borrow().max_record_bytes)
            .unwrap_or(4096);
        g.connections
            .iter()
            .map(|c| SlotSnapshot {
                writer: c.writer,
                writer_stream: c.writer_stream,
                reader: c.reader,
                reader_stream: c.reader_stream,
                record: c
                    .slot
                    .as_ref()
                    .map(|r| RecordPreview::from_record(r, max_bytes)),
                peeked: c.reader_peeked,
                severed: c.severed,
                writer_done: c.writer_done,
            })
            .collect()
    }

    pub fn stage_by_name(&self, name: &str) -> Option<StageId> {
        let key = name.to_ascii_uppercase();
        self.shared
            .borrow()
            .stages
            .iter()
            .enumerate()
            .find_map(|(i, s)| {
                if s.name.to_ascii_uppercase() == key {
                    Some(StageId(i))
                } else {
                    None
                }
            })
    }

    pub fn aggregate_rc(&self) -> i32 {
        crate::aggregate_return_codes(self.shared.borrow().stages.iter().map(|s| s.return_code))
    }

    /// Run until the pipeline ends, fails, or a pause request takes effect.
    pub fn run(&mut self) -> SessionStatus {
        if self.status.is_terminal() {
            return self.status.clone();
        }
        if let Some(d) = self.shared.borrow().debug.clone() {
            d.borrow_mut().begin_run();
        }
        self.status = SessionStatus::Running;
        self.drive()
    }

    /// One dispatcher quantum. Callers that must stay responsive (the GUI
    /// worker) loop this and read Stop/Shutdown between records.
    ///
    /// Does not run a paused or idle session to completion: it does at most
    /// one pick/resume or one DELAY/helper sleep slice.
    pub fn poll(&mut self) -> SessionStatus {
        if self.status.is_terminal() {
            return self.status.clone();
        }
        if matches!(self.status, SessionStatus::Idle | SessionStatus::Paused) {
            if let Some(d) = self.shared.borrow().debug.clone() {
                d.borrow_mut().begin_run();
            }
            self.status = SessionStatus::Running;
        }
        self.drive_limited(1)
    }

    pub fn request_pause(&mut self) {
        if let Some(d) = self.shared.borrow().debug.clone() {
            d.borrow_mut().request_pause();
        }
    }

    /// Abort and drain parked coroutines. Already-written host files stay written.
    ///
    /// A session that was never started does not run stages on stop.
    pub fn stop(&mut self) -> SessionStatus {
        if !self.kicked {
            self.status = SessionStatus::Stopped { rc: 0 };
            return self.status.clone();
        }
        abort_and_drain(&mut self.cos, &self.shared);
        let rc = self.aggregate_rc();
        self.status = SessionStatus::Stopped { rc };
        self.status.clone()
    }

    /// Advance until the selected stage completes one observable record cycle.
    pub fn step_stage(&mut self, selected: StageId) -> StepOutcome {
        if let Some(out) = self.start_step(selected) {
            return out;
        }
        loop {
            if let Some(out) = self.poll_step(selected) {
                return out;
            }
        }
    }

    /// Begin a selected-stage step. `Some` means the step cannot start.
    pub fn start_step(&mut self, selected: StageId) -> Option<StepOutcome> {
        if let SessionStatus::Failed(msg) = &self.status {
            return Some(StepOutcome::Failed(msg.clone()));
        }
        if matches!(self.status, SessionStatus::Stopped { .. }) {
            return Some(StepOutcome::Stopped {
                rc: self.aggregate_rc(),
            });
        }
        if matches!(self.status, SessionStatus::Ended { .. }) {
            return Some(StepOutcome::PipelineEnded {
                rc: self.aggregate_rc(),
            });
        }
        let n_stages = self.shared.borrow().stages.len();
        if selected.0 >= n_stages {
            return Some(StepOutcome::NoProgress {
                reason: format!("no such stage {}", selected.0),
            });
        }
        {
            let g = self.shared.borrow();
            let s = &g.stages[selected.0];
            if s.ended {
                return Some(StepOutcome::Ended { rc: s.return_code });
            }
            if s.waiting_callpipe.is_some() {
                let children = s
                    .waiting_callpipe
                    .as_ref()
                    .map(|c| c.children.clone())
                    .unwrap_or_default();
                return Some(StepOutcome::BlockedOnCallPipe { children });
            }
        }
        if let Some(d) = self.shared.borrow().debug.clone() {
            d.borrow_mut().begin_step(selected);
        }
        self.status = SessionStatus::Running;
        None
    }

    /// One quantum of a selected-stage step. `None` means keep polling.
    pub fn poll_step(&mut self, selected: StageId) -> Option<StepOutcome> {
        let poll = self.drive_limited(1);
        match poll {
            SessionStatus::Failed(msg) => Some(StepOutcome::Failed(msg)),
            SessionStatus::Stopped { rc } => Some(StepOutcome::Stopped { rc }),
            SessionStatus::Ended { rc } => Some(
                self.take_step_outcome(selected)
                    .unwrap_or(StepOutcome::PipelineEnded { rc }),
            ),
            SessionStatus::Paused => {
                if let Some(out) = self.take_step_outcome(selected) {
                    self.status = SessionStatus::Paused;
                    Some(out)
                } else if self
                    .shared
                    .borrow()
                    .stages
                    .get(selected.0)
                    .is_some_and(|s| s.waiting_helper)
                {
                    self.status = SessionStatus::Paused;
                    Some(StepOutcome::BlockedOnHelper)
                } else {
                    Some(StepOutcome::NoProgress {
                        reason: "selected stage did not complete an observable record cycle".into(),
                    })
                }
            }
            SessionStatus::Running | SessionStatus::Idle => {
                if let Some(out) = self.take_step_outcome(selected) {
                    self.status = SessionStatus::Paused;
                    Some(out)
                } else {
                    None
                }
            }
        }
    }

    fn take_step_outcome(&self, selected: StageId) -> Option<StepOutcome> {
        let g = self.shared.borrow();
        let dbg = g.debug.as_ref()?;
        let ctl = dbg.borrow();
        let step = ctl.step.as_ref()?;
        let ended = g.stages[selected.0].ended;
        let rc = g.stages[selected.0].return_code;
        let children = g.stages[selected.0]
            .waiting_callpipe
            .as_ref()
            .map(|c| c.children.clone())
            .unwrap_or_default();
        step.outcome(ended, rc, &children)
    }

    pub(crate) fn run_to_completion(&mut self) -> PipeResult<i32> {
        match self.run() {
            SessionStatus::Ended { rc } => Ok(rc),
            SessionStatus::Stopped { rc } => Ok(rc),
            SessionStatus::Failed(msg) => Err(PipeError::msg(msg)),
            other => Err(PipeError::msg(format!(
                "dispatcher stopped unexpectedly ({other:?})"
            ))),
        }
    }

    fn observe_remote(&mut self) -> Option<SessionStatus> {
        if self.remote.take_stop() {
            return Some(self.stop());
        }
        if self.remote.take_pause() {
            self.request_pause();
        }
        None
    }

    fn kick_if_needed(&mut self) -> PipeResult<()> {
        if self.kicked {
            return Ok(());
        }
        self.kicked = true;
        let mut i = 0;
        while i < self.cos.len() {
            resume_stage(i, &mut self.cos, &self.shared)?;
            absorb_pending(&mut self.cos, &self.shared)?;
            if self.debug_should_pause() {
                return Ok(());
            }
            i += 1;
        }
        Ok(())
    }

    fn debug_should_pause(&self) -> bool {
        let g = self.shared.borrow();
        let Some(d) = g.debug.as_ref() else {
            return false;
        };
        let ctl = d.borrow();
        match ctl.policy {
            DebugPolicy::Run => false,
            DebugPolicy::Pause => ctl.boundary_since_resume,
            DebugPolicy::Step => ctl.step_complete,
        }
    }

    fn park_debug(&mut self) {
        if let Some(d) = self.shared.borrow().debug.clone() {
            d.borrow_mut().park();
        }
        self.status = SessionStatus::Paused;
    }

    fn drive(&mut self) -> SessionStatus {
        self.drive_limited(usize::MAX)
    }

    fn drive_limited(&mut self, quanta: usize) -> SessionStatus {
        if let Some(st) = self.observe_remote() {
            return st;
        }
        if let Err(e) = self.kick_if_needed() {
            abort_and_drain(&mut self.cos, &self.shared);
            self.status = SessionStatus::Failed(e.to_string());
            return self.status.clone();
        }
        if let Some(st) = self.observe_remote() {
            return st;
        }
        if self.debug_should_pause() {
            let _ = absorb_pending(&mut self.cos, &self.shared);
            self.park_debug();
            return self.status.clone();
        }
        let mut used = 0usize;
        loop {
            if quanta != usize::MAX && used >= quanta {
                self.status = SessionStatus::Running;
                return self.status.clone();
            }
            used = used.saturating_add(1);
            if let Some(st) = self.observe_remote() {
                return st;
            }
            if let Err(e) = absorb_pending(&mut self.cos, &self.shared) {
                abort_and_drain(&mut self.cos, &self.shared);
                self.status = SessionStatus::Failed(e.to_string());
                return self.status.clone();
            }
            if self.shared.borrow().stages.iter().all(|s| s.ended) {
                if self.shared.borrow().trace.is_enabled() {
                    self.shared.borrow().trace.emit("dispatcher end");
                }
                let rc = self.aggregate_rc();
                self.status = SessionStatus::Ended { rc };
                return self.status.clone();
            }
            let stage_err = self
                .shared
                .borrow()
                .stages
                .iter()
                .find_map(|s| s.error.clone());
            if let Some(err) = stage_err {
                let err = wrap_nested_callpipe_error(&self.shared.borrow(), err);
                abort_and_drain(&mut self.cos, &self.shared);
                self.status = SessionStatus::Failed(err.to_string());
                return self.status.clone();
            }
            if self.shared.borrow().aborted {
                abort_and_drain(&mut self.cos, &self.shared);
                self.status = SessionStatus::Stopped {
                    rc: self.aggregate_rc(),
                };
                return self.status.clone();
            }

            if self.debug_should_pause() {
                self.park_debug();
                return self.status.clone();
            }

            let next = pick_next(&self.shared.borrow());
            if let Some(i) = next {
                self.idle_passes = 0;
                if let Err(e) = resume_stage(i, &mut self.cos, &self.shared) {
                    abort_and_drain(&mut self.cos, &self.shared);
                    self.status = SessionStatus::Failed(e.to_string());
                    return self.status.clone();
                }
                if let Err(e) = absorb_pending(&mut self.cos, &self.shared) {
                    abort_and_drain(&mut self.cos, &self.shared);
                    self.status = SessionStatus::Failed(e.to_string());
                    return self.status.clone();
                }
                if self.debug_should_pause() {
                    self.park_debug();
                    return self.status.clone();
                }
            } else {
                if self.has_debug_hold() {
                    self.park_debug();
                    return self.status.clone();
                }
                self.idle_passes += 1;
                let mut progressed = false;
                let readers = {
                    let g = self.shared.borrow();
                    g.stages
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| {
                            if s.ended || s.waiting_debug {
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
                    if resume_stage(i, &mut self.cos, &self.shared).is_err() {
                        abort_and_drain(&mut self.cos, &self.shared);
                        self.status = SessionStatus::Failed("resume failed".into());
                        return self.status.clone();
                    }
                    progressed = true;
                    if self.debug_should_pause() {
                        let _ = absorb_pending(&mut self.cos, &self.shared);
                        self.park_debug();
                        return self.status.clone();
                    }
                }
                let writers = {
                    let g = self.shared.borrow();
                    g.stages
                        .iter()
                        .enumerate()
                        .filter_map(|(i, s)| {
                            if s.ended || s.waiting_debug {
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
                    let _ = resume_stage(i, &mut self.cos, &self.shared);
                    progressed = true;
                    if self.debug_should_pause() {
                        let _ = absorb_pending(&mut self.cos, &self.shared);
                        self.park_debug();
                        return self.status.clone();
                    }
                }
                for i in 0..self.shared.borrow().stages.len() {
                    if self.shared.borrow().stages[i].waiting_commit.is_some()
                        && !self.shared.borrow().stages[i].waiting_debug
                    {
                        let _ = resume_stage(i, &mut self.cos, &self.shared);
                        progressed = true;
                    }
                }
                let helpers: Vec<usize> = {
                    let g = self.shared.borrow();
                    g.stages
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| !s.ended && !s.waiting_debug && s.waiting_helper)
                        .map(|(i, _)| i)
                        .collect()
                };
                for i in helpers {
                    let _ = resume_stage(i, &mut self.cos, &self.shared);
                    if !self.shared.borrow().stages[i].waiting_helper {
                        progressed = true;
                    }
                }
                if !progressed {
                    let deadline = nearest_sleep_deadline(&self.shared.borrow());
                    if let Some(deadline) = deadline {
                        if self.debug_should_pause() {
                            self.park_debug();
                            return self.status.clone();
                        }
                        let now = Instant::now();
                        if deadline > now {
                            std::thread::sleep((deadline - now).min(SLEEP_SLICE));
                        }
                        self.idle_passes = 0;
                        continue;
                    }
                    if self
                        .shared
                        .borrow()
                        .stages
                        .iter()
                        .any(|s| !s.ended && s.waiting_helper)
                    {
                        if self.debug_should_pause() {
                            self.park_debug();
                            return self.status.clone();
                        }
                        std::thread::sleep(SLEEP_SLICE);
                        self.idle_passes = 0;
                        continue;
                    }
                    if self.idle_passes > 64 {
                        let (friendly, raw, origin) = {
                            let g = self.shared.borrow();
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
                        abort_and_drain(&mut self.cos, &self.shared);
                        let err = PipeError::msg(friendly);
                        let err = match origin {
                            Some(o) => err.with_origin(o),
                            None => err,
                        };
                        self.status = SessionStatus::Failed(err.to_string());
                        return self.status.clone();
                    }
                } else {
                    self.idle_passes = 0;
                    if self.debug_should_pause() {
                        self.park_debug();
                        return self.status.clone();
                    }
                }
            }
        }
    }

    fn has_debug_hold(&self) -> bool {
        let g = self.shared.borrow();
        if g.debug.is_none() {
            return false;
        }
        g.stages.iter().any(|s| !s.ended && s.waiting_debug)
            || g.debug.as_ref().is_some_and(|d| d.borrow().holding)
    }
}

impl Drop for DebugSession {
    fn drop(&mut self) {
        if !self.kicked {
            return;
        }
        let live = self.shared.borrow().stages.iter().any(|s| !s.ended)
            || self.cos.iter().any(|c| matches!(c, CoState::Running(_)));
        if live {
            abort_and_drain(&mut self.cos, &self.shared);
        }
    }
}

/// Coroutine dispatcher: resumes ready stages with drain (consumer) bias.
pub struct Dispatcher;

impl Dispatcher {
    pub fn run(
        stages: Vec<StageSpec>,
        connections: Vec<ConnectionSpec>,
        trace: TraceConfig,
    ) -> PipeResult<i32> {
        DebugSession::from_parts(stages, connections, trace, None).run_to_completion()
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
                "{}[{}]: read={:?} read_any={:?} write={:?} commit={:?} yield={} until={:?} callpipe={} helper={}",
                s.name,
                i,
                s.waiting_read,
                s.waiting_read_any,
                s.waiting_write,
                s.waiting_commit,
                s.waiting_yield,
                s.waiting_until,
                s.waiting_callpipe.is_some(),
                s.waiting_helper
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn stall_wait_text(s: &StageState) -> Option<String> {
    if s.waiting_debug {
        return Some("waiting for debug hold".into());
    }
    if s.waiting_callpipe.is_some() {
        return Some("waiting for CALLPIPE".into());
    }
    if s.waiting_helper {
        return Some("waiting for helper".into());
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

/// Index of the non-ended sleeper whose `waiting_until` is earliest and
/// already due. `None` if every sleeper is still in the future.
fn earliest_expired_sleep(shared: &Shared) -> Option<usize> {
    let now = Instant::now();
    shared
        .stages
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.ended)
        .filter_map(|(i, s)| s.waiting_until.map(|t| (i, t)))
        .filter(|(_, t)| *t <= now)
        .min_by_key(|(_, t)| *t)
        .map(|(i, _)| i)
}

/// Prefer runnable consumers (waiting read with data/eof available), then
/// writers whose slot is free, then commit waiters that are satisfied, then
/// wall-clock waiters whose deadline has arrived (DELAY; BEAT may also be
/// waiting_read so an arriving record wins via step 1), then any non-ended
/// stage that is not clearly blocked forever, then yielders.
fn debug_waiters_runnable(shared: &Shared) -> bool {
    match &shared.debug {
        None => true,
        Some(d) => d.borrow().waiters_runnable(),
    }
}

fn pick_next(shared: &Shared) -> Option<usize> {
    // 0. Resume debug holds when the controller has released them.
    if debug_waiters_runnable(shared) {
        for (i, s) in shared.stages.iter().enumerate() {
            if !s.ended && s.waiting_debug {
                return Some(i);
            }
        }
    }
    // 1. Readers that can make progress (drain bias).
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended || s.waiting_debug {
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
        if s.ended || s.waiting_debug {
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
        if s.ended || s.waiting_debug {
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
    //
    // When a coarse host sleep expires several waiters at once, resume the
    // earliest deadline first. Stage-index order would let a later producer
    // EOF before BEAT writes a marker (the macOS CI `got []` failures).
    if shared.external_stop {
        for (i, s) in shared.stages.iter().enumerate() {
            if s.ended || s.waiting_debug {
                continue;
            }
            if s.waiting_until.is_some() && !still_sleeping(s, true) {
                return Some(i);
            }
        }
    } else if let Some(i) = earliest_expired_sleep(shared) {
        if !shared.stages[i].waiting_debug {
            return Some(i);
        }
    }
    // 4b. CALLPIPE waiters whose nested stages have all ended (or the
    // network aborted). Must not fall through to step 5 as "ready".
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended || s.waiting_debug {
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
        if s.ended || s.waiting_debug {
            continue;
        }
        if s.waiting_callpipe.is_some() {
            continue;
        }
        if s.waiting_helper {
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
        if !s.ended && !s.waiting_debug && s.waiting_yield {
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
                        let mut g = shared_cl.borrow_mut();
                        g.trace_stage(stage_id, "start");
                        g.emit_debug(stage_id, DebugEventKind::StageStart);
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
            let rc = g.stages[index].return_code;
            g.emit_debug(StageId(index), DebugEventKind::StageEnd { rc });
            g.stages[index].ended = true;
            g.stages[index].waiting_read = None;
            g.stages[index].waiting_read_any.clear();
            g.stages[index].waiting_write = None;
            g.stages[index].waiting_commit = None;
            g.stages[index].waiting_yield = false;
            g.stages[index].waiting_until = None;
            g.stages[index].waiting_callpipe = None;
            g.stages[index].waiting_helper = false;
            g.stages[index].waiting_debug = false;
            g.mark_writer_done(StageId(index));
            g.sever_remaining_inputs(StageId(index));
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
    parent
        .shared
        .borrow_mut()
        .emit_debug(parent.stage_id, DebugEventKind::CallPipeEnter);

    let mut builder = PipelineBuilder::new().with_trace(nest_trace);
    build(&mut builder);

    let result = run_callpipe_loaned(parent, builder, connectors);

    parent
        .shared
        .borrow()
        .trace_stage(parent.stage_id, "CALLPIPE exit");
    parent
        .shared
        .borrow_mut()
        .emit_debug(parent.stage_id, DebugEventKind::CallPipeExit);
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

/// Add a nested pipeline to the running set (CMS ADDPIPE).
///
/// Nested stages are spliced into the parent's dispatcher and join the
/// caller's commit group. Connectors insert-between so the caller keeps
/// its streams. The caller does not wait and loans are not restored.
pub(crate) fn run_addpipe(
    parent: &mut StageCtx,
    build: impl FnOnce(&mut PipelineBuilder),
    connectors: AddPipeConnectors,
) -> PipeResult<()> {
    let nest_trace = parent.shared.borrow().trace.clone();
    parent
        .shared
        .borrow()
        .trace_stage(parent.stage_id, "ADDPIPE enter");
    parent
        .shared
        .borrow_mut()
        .emit_debug(parent.stage_id, DebugEventKind::AddPipeEnter);

    let mut builder = PipelineBuilder::new().with_trace(nest_trace);
    build(&mut builder);

    let result = run_addpipe_inserted(parent, builder, connectors);

    parent
        .shared
        .borrow()
        .trace_stage(parent.stage_id, "ADDPIPE exit");
    parent
        .shared
        .borrow_mut()
        .emit_debug(parent.stage_id, DebugEventKind::AddPipeExit);
    result
}

fn run_addpipe_inserted(
    parent: &mut StageCtx,
    builder: PipelineBuilder,
    connectors: AddPipeConnectors,
) -> PipeResult<()> {
    if parent.shared.borrow().aborted {
        return Err(PipeError::Aborted);
    }

    let group = parent.shared.borrow().stages[parent.stage_id.0].commit_group;
    let children = spawn_graph(&parent.shared, builder, group);
    apply_insert_between(
        &parent.shared,
        parent.stage_id,
        &connectors.attachments,
        &children,
    );

    if !children.is_empty() {
        let g = parent.shared.borrow();
        g.trace_stage(
            parent.stage_id,
            format!(
                "ADDPIPE spawn ({} nested stages, {} connectors, commit group {group})",
                children.len(),
                connectors.attachments.len()
            ),
        );
    }
    let _ = connectors.processing;
    Ok(())
}

fn output_taken(g: &Shared, stage: StageId, stream: StreamId) -> bool {
    g.connections
        .iter()
        .any(|c| c.writer == stage && c.writer_stream == stream && !c.severed)
}

fn input_taken(g: &Shared, stage: StageId, stream: StreamId) -> bool {
    g.connections
        .iter()
        .any(|c| c.reader == stage && c.reader_stream == stream && !c.severed)
}

fn take_reader_slot(g: &mut Shared, reader: StageId, stream: StreamId) -> Option<Record> {
    // Only a peeked slot stays with the caller (PIPCMD leaves the
    // command in the slot until the command ends). An unread in-flight
    // record stays on the retargeted connection so the nested first
    // stage sees it.
    for c in &mut g.connections {
        if c.reader == reader && c.reader_stream == stream && !c.severed && c.reader_peeked {
            c.reader_peeked = false;
            return c.slot.take();
        }
    }
    None
}

fn apply_insert_between(
    shared: &Rc<RefCell<Shared>>,
    parent: StageId,
    attachments: &[AddPipeAttachment],
    children: &[StageId],
) {
    // Input connectors first so `*: | filter | *:` keeps the caller reading
    // and leaves the original output in place when the output half conflicts.
    let mut input_first: Vec<&AddPipeAttachment> = Vec::new();
    let mut output_rest: Vec<&AddPipeAttachment> = Vec::new();
    for a in attachments {
        match a.side {
            LoanSide::Input => input_first.push(a),
            LoanSide::Output => output_rest.push(a),
        }
    }
    for a in input_first.into_iter().chain(output_rest) {
        let Some(&nested) = children.get(a.nested_stage) else {
            continue;
        };
        let Some(&extra) = children.get(a.extra_stage) else {
            continue;
        };
        let mut g = shared.borrow_mut();
        match a.side {
            LoanSide::Input => {
                if output_taken(&g, extra, a.extra_stream) {
                    continue;
                }
                // Keep a peeked command on the caller. PIPCMD leaves
                // the command in the slot until the command ends;
                // moving that slot onto the nested first stage would
                // re-issue ADDPIPE forever. An unread record stays
                // on the retargeted connection for the nested first
                // stage.
                let slot = take_reader_slot(&mut g, parent, a.caller_stream);
                g.retarget_reader(parent, a.caller_stream, nested, a.nested_stream);
                let mut added = Connection::new(extra, a.extra_stream, parent, a.caller_stream);
                added.reader_peeked = slot.is_some();
                added.slot = slot;
                g.connections.push(added);
            }
            LoanSide::Output => {
                if input_taken(&g, extra, a.extra_stream)
                    || output_taken(&g, nested, a.nested_stream)
                {
                    continue;
                }
                g.retarget_writer(parent, a.caller_stream, nested, a.nested_stream);
                g.connect_runtime(parent, a.caller_stream, extra, a.extra_stream);
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
    fn expired_sleep_wakes_the_earliest_deadline_first() {
        // Two parked DELAY-style waits. The later-added stage has the
        // earlier deadline. Resume that one first, not stage-index order.
        use std::time::{Duration, Instant};

        let order = Rc::new(RefCell::new(Vec::new()));
        let late_log = Rc::clone(&order);
        let early_log = Rc::clone(&order);
        let mut b = PipelineBuilder::new();
        b.add_stage(
            "late",
            Box::new(move |ctx| {
                let _ = ctx.wait_until(Instant::now() + Duration::from_millis(40));
                late_log.borrow_mut().push("late");
            }),
        );
        b.add_stage(
            "early",
            Box::new(move |ctx| {
                let _ = ctx.wait_until(Instant::now() + Duration::from_millis(5));
                early_log.borrow_mut().push("early");
            }),
        );
        b.run().unwrap();
        assert_eq!(
            *order.borrow(),
            vec!["early", "late"],
            "earliest deadline must run first; got {:?}",
            order.borrow()
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

    fn input_insert() -> AddPipeConnectors {
        AddPipeConnectors {
            attachments: vec![AddPipeAttachment {
                side: LoanSide::Input,
                caller_stream: StreamId::PRIMARY,
                nested_stage: 0,
                nested_stream: StreamId::PRIMARY,
                extra_stage: 0,
                extra_stream: StreamId::PRIMARY,
            }],
            processing: None,
        }
    }

    fn output_insert() -> AddPipeConnectors {
        AddPipeConnectors {
            attachments: vec![AddPipeAttachment {
                side: LoanSide::Output,
                caller_stream: StreamId::PRIMARY,
                nested_stage: 0,
                nested_stream: StreamId::PRIMARY,
                extra_stage: 0,
                extra_stream: StreamId::PRIMARY,
            }],
            processing: None,
        }
    }

    fn both_inserts() -> AddPipeConnectors {
        AddPipeConnectors {
            attachments: vec![
                AddPipeAttachment {
                    side: LoanSide::Input,
                    caller_stream: StreamId::PRIMARY,
                    nested_stage: 0,
                    nested_stream: StreamId::PRIMARY,
                    extra_stage: 0,
                    extra_stream: StreamId::PRIMARY,
                },
                AddPipeAttachment {
                    side: LoanSide::Output,
                    caller_stream: StreamId::PRIMARY,
                    nested_stage: 0,
                    nested_stream: StreamId::PRIMARY,
                    extra_stage: 0,
                    extra_stream: StreamId::PRIMARY,
                },
            ],
            processing: None,
        }
    }

    fn upper_copier() -> StageFn {
        Box::new(|ctx| {
            while let Ok(Some(r)) = ctx.read() {
                let up = r.to_string_lossy().to_ascii_uppercase();
                if ctx.write(Record::from(up)).is_err() {
                    break;
                }
            }
        })
    }

    #[test]
    fn addpipe_input_insert_keeps_caller_reading() {
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
                ctx.addpipe(
                    |pb| {
                        let _ = pb.add_stage("upper", upper_copier());
                    },
                    input_insert(),
                )
                .unwrap();
                assert!(
                    ctx.input_connected(StreamId::PRIMARY),
                    "ADDPIPE input insert must keep the caller connected"
                );
                while let Ok(Some(r)) = ctx.read() {
                    if ctx.write(r).is_err() {
                        break;
                    }
                }
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
        assert_eq!(*collected.borrow(), vec!["HELLO".to_string()]);
    }

    #[test]
    fn addpipe_output_insert_keeps_caller_writing() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.addpipe(
                    |pb| {
                        let _ = pb.add_stage("upper", upper_copier());
                    },
                    output_insert(),
                )
                .unwrap();
                assert!(
                    ctx.output_connected(StreamId::PRIMARY),
                    "ADDPIPE output insert must keep the caller connected"
                );
                let _ = ctx.write(Record::from("hello"));
                let _ = ctx.write(Record::from("after"));
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
            vec!["HELLO".to_string(), "AFTER".to_string()]
        );
    }

    #[test]
    fn addpipe_both_connectors_prefers_input_insert() {
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
                ctx.addpipe(
                    |pb| {
                        let _ = pb.add_stage("upper", upper_copier());
                    },
                    both_inserts(),
                )
                .unwrap();
                assert!(ctx.input_connected(StreamId::PRIMARY));
                assert!(ctx.output_connected(StreamId::PRIMARY));
                while let Ok(Some(r)) = ctx.read() {
                    if ctx.write(r).is_err() {
                        break;
                    }
                }
                let _ = ctx.write(Record::from("after"));
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
            vec!["HELLO".to_string(), "after".to_string()],
            "input insert uppercases what the caller reads; later OUTPUT is not filtered"
        );
    }

    #[test]
    fn addpipe_named_output_insert_uses_bound_stream() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.bind_stream_name("FOO", StreamId::SECONDARY).unwrap();
                ctx.addpipe(
                    |pb| {
                        let _ = pb.add_stage("upper", upper_copier());
                    },
                    AddPipeConnectors {
                        attachments: vec![AddPipeAttachment {
                            side: LoanSide::Output,
                            caller_stream: StreamId::SECONDARY,
                            nested_stage: 0,
                            nested_stream: StreamId::PRIMARY,
                            extra_stage: 0,
                            extra_stream: StreamId::PRIMARY,
                        }],
                        processing: None,
                    },
                )
                .unwrap();
                ctx.select_output(StreamId::SECONDARY);
                let _ = ctx.write(Record::from("hello"));
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
        b.connect(caller, StreamId::SECONDARY, sink, StreamId::PRIMARY);
        b.run().unwrap();
        assert_eq!(*collected.borrow(), vec!["HELLO".to_string()]);
    }

    #[test]
    fn addpipe_does_not_wait_so_later_write_still_runs() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("in"));
            }),
        );
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.addpipe(
                    |pb| {
                        let _ = pb.add_stage("copy", copier());
                    },
                    input_insert(),
                )
                .unwrap();
                let _ = ctx.write(Record::from("later"));
                while let Ok(Some(r)) = ctx.read() {
                    if ctx.write(r).is_err() {
                        break;
                    }
                }
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
        let got = collected.borrow().clone();
        assert!(
            got.contains(&"later".to_string()),
            "ADDPIPE must return immediately so a later write runs: {got:?}"
        );
        assert!(
            got.contains(&"in".to_string()),
            "input insert must still deliver the upstream record: {got:?}"
        );
    }

    #[test]
    fn ending_stage_severs_unread_input() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                for rec in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
                    if ctx.write(Record::from(rec)).is_err() {
                        break;
                    }
                }
            }),
        );
        let early = b.add_stage(
            "early",
            Box::new(|ctx| {
                let _ = ctx.write(Record::from("x"));
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
        b.connect_primary(src, early);
        b.connect_primary(early, sink);
        b.run().unwrap();
        assert_eq!(*collected.borrow(), vec!["x".to_string()]);
    }

    #[test]
    fn addpipe_caller_end_severs_nested_write_toward_input() {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let out = Rc::clone(&collected);
        let mut b = PipelineBuilder::new();
        let src = b.add_stage(
            "src",
            Box::new(|ctx| {
                for rec in ["a", "b", "c", "d", "e"] {
                    if ctx.write(Record::from(rec)).is_err() {
                        break;
                    }
                }
            }),
        );
        let caller = b.add_stage(
            "caller",
            Box::new(|ctx| {
                ctx.addpipe(
                    |pb| {
                        let _ = pb.add_stage("copy", copier());
                    },
                    input_insert(),
                )
                .unwrap();
                if let Ok(Some(r)) = ctx.read() {
                    let _ = ctx.write(r);
                }
                let _ = ctx.write(Record::from("still"));
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
            vec!["a".to_string(), "still".to_string()]
        );
    }
}
