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

/// How CALLPIPE connectors attach nested pipeline ends to the caller stage.
#[derive(Debug, Clone, Default)]
pub struct CallPipeConnectors {
    /// Nested first-stage input comes from caller's selected input (consume from parent).
    pub feed_from_caller_input: bool,
    /// Nested last-stage output is written to caller's selected output.
    pub drain_to_caller_output: bool,
    /// Caller's input stream used when feeding nested pipeline.
    pub caller_input: StreamId,
    /// Caller's output stream used when draining nested pipeline.
    pub caller_output: StreamId,
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
            self.connections[idx].writer_done = true;
        }
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
            if c.writer == stage {
                c.writer_done = true;
            }
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
        self.connections.push(Connection {
            writer,
            writer_stream,
            reader,
            reader_stream,
            slot: None,
            severed: false,
            writer_done: false,
        });
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

    pub(crate) fn commit_satisfied(&self, _stage: StageId, level: i32) -> bool {
        for s in &self.stages {
            if s.ended {
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
                .map(|c| Connection {
                    writer: c.writer,
                    writer_stream: c.writer_stream,
                    reader: c.reader,
                    reader_stream: c.reader_stream,
                    slot: None,
                    severed: false,
                    writer_done: false,
                })
                .collect(),
            aborted: false,
            external_stop: false,
            network_commit: crate::COMMIT_INITIAL, // reserved for richer commit lattices
            trace,
            pending_spawns: Vec::new(),
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
                "{}[{}]: read={:?} read_any={:?} write={:?} commit={:?} yield={} until={:?}",
                s.name,
                i,
                s.waiting_read,
                s.waiting_read_any,
                s.waiting_write,
                s.waiting_commit,
                s.waiting_yield,
                s.waiting_until
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn stall_wait_text(s: &StageState) -> Option<String> {
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
    // 5. Stages not yet started / not waiting (ready).
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
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
            g.mark_writer_done(StageId(index));
            cos[index] = CoState::Done;
            Ok(())
        }
    }
}

/// Run a nested pipeline for CALLPIPE from within a parent stage.
pub(crate) fn run_callpipe(
    parent: &mut StageCtx,
    build: impl FnOnce(&mut PipelineBuilder),
    connectors: CallPipeConnectors,
) -> PipeResult<()> {
    // Strategy: materialize records from parent input (if requested), run a
    // nested dispatcher with synthetic feeder/drainer stages, then write
    // nested output records to the parent output.
    //
    // For streaming CALLPIPE fidelity we wire connector stages that call back
    // into the parent StageCtx. Because we are already inside a coroutine,
    // nested stages that read/write parent streams use dedicated bridge stages
    // running in the nested dispatcher that pull/push via shared buffers
    // filled cooperatively.
    //
    // Practical v1 approach (still record-oriented and connector-correct):
    // - If feed_from_caller_input: parent reads all remaining input into a buffer
    //   first ONLY when connectors request a buffering callpipe. For streaming,
    //   we install bridge stages.

    let nest_trace = parent.shared.borrow().trace.clone();
    parent
        .shared
        .borrow()
        .trace_stage(parent.stage_id, "CALLPIPE enter");

    let mut builder = PipelineBuilder::new().with_trace(nest_trace.clone());
    build(&mut builder);

    // Inject connector stages when requested.
    let result = if connectors.feed_from_caller_input || connectors.drain_to_caller_output {
        run_callpipe_bridged(parent, builder, connectors)
    } else {
        builder.run()
    };

    parent
        .shared
        .borrow()
        .trace_stage(parent.stage_id, "CALLPIPE exit");
    result
}

fn run_callpipe_bridged(
    parent: &mut StageCtx,
    user: PipelineBuilder,
    connectors: CallPipeConnectors,
) -> PipeResult<()> {
    // Buffering CALLPIPE: parent input is read fully into the nested feeder;
    // nested output is collected then written to the parent. Correct connectors,
    // simpler than true streaming bridges for v1.

    let input_records = if connectors.feed_from_caller_input {
        parent.select_input(connectors.caller_input);
        parent.read_all()?
    } else {
        Vec::new()
    };

    let outputs: Rc<RefCell<Vec<Record>>> = Rc::new(RefCell::new(Vec::new()));
    let inputs: Rc<RefCell<Vec<Record>>> = Rc::new(RefCell::new(input_records));

    let nest_trace = parent.shared.borrow().trace.clone();
    let (user_stages, user_conns) = user.into_parts();
    let mut wrapped = PipelineBuilder::new().with_trace(nest_trace);

    let feed_id = if connectors.feed_from_caller_input {
        let inputs_cl = Rc::clone(&inputs);
        Some(wrapped.add_stage(
            "*feed",
            Box::new(move |ctx: &mut StageCtx| {
                let recs = std::mem::take(&mut *inputs_cl.borrow_mut());
                for r in recs {
                    if ctx.write(r).is_err() {
                        break;
                    }
                }
            }),
        ))
    } else {
        None
    };

    let user_start = wrapped.stage_count();
    let user_count = user_stages.len();
    wrapped.append_stages(user_stages, user_conns);

    if let Some(feed) = feed_id {
        if user_count > 0 {
            wrapped.connect(
                feed,
                StreamId::PRIMARY,
                StageId(user_start),
                StreamId::PRIMARY,
            );
        }
    }

    if connectors.drain_to_caller_output && user_count > 0 {
        let outputs_cl = Rc::clone(&outputs);
        let drain = wrapped.add_stage(
            "*drain",
            Box::new(move |ctx: &mut StageCtx| {
                while let Ok(Some(r)) = ctx.read() {
                    outputs_cl.borrow_mut().push(r);
                }
            }),
        );
        // Connect user stages that have no primary output edge to the drain.
        let sinks = wrapped.stages_without_primary_output(user_start, user_start + user_count);
        for s in sinks {
            wrapped.connect(s, StreamId::PRIMARY, drain, StreamId::PRIMARY);
        }
    }

    wrapped.run()?;

    if connectors.drain_to_caller_output {
        parent.select_output(connectors.caller_output);
        for r in outputs.borrow_mut().drain(..) {
            parent.write(r)?;
        }
    }
    Ok(())
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

    pub(crate) fn stage_count(&self) -> usize {
        self.stages.len()
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
}
