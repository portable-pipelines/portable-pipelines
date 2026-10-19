use std::cell::RefCell;
use std::rc::Rc;

use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult, Yielder};

use crate::error::{PipeError, PipeResult};
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
    pub ended: bool,
    pub error: Option<PipeError>,
    /// Live only while this stage's coroutine body is on the stack.
    pub yielder: *const corosensei::Yielder<(), WaitReason>,
}

pub(crate) struct Shared {
    pub stages: Vec<StageState>,
    pub connections: Vec<Connection>,
    pub aborted: bool,
    /// Global minimum commit currently allowed to proceed (simplified negotiation).
    #[allow(dead_code)]
    pub network_commit: i32,
    pub trace: TraceConfig,
}

impl Shared {
    pub(crate) fn stage_name(&self, id: StageId) -> &str {
        &self.stages[id.0].name
    }

    pub(crate) fn trace_stage(&self, id: StageId, detail: impl AsRef<str>) {
        if !self.trace.is_enabled() {
            return;
        }
        self.trace
            .emit(format!("stage[{}] {} {}", id.0, self.stage_name(id), detail.as_ref()));
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
        let id = StageId(self.stages.len());
        self.stages.push(StageSpec {
            name: name.into(),
            entry,
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
    ) -> PipeResult<()> {
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
                .map(|s| StageState {
                    name: s.name.clone(),
                    commit_level: crate::COMMIT_INITIAL,
                    waiting_read: None,
                    waiting_read_any: Vec::new(),
                    waiting_write: None,
                    waiting_commit: None,
                    waiting_yield: false,
                    ended: false,
                    error: None,
                    yielder: std::ptr::null(),
                })
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
            network_commit: crate::COMMIT_INITIAL, // reserved for richer commit lattices
            trace,
        }));

        let mut cos: Vec<CoState> = stages
            .into_iter()
            .map(|s| CoState::NotStarted(s.entry))
            .collect();

        // Kick every stage once so producers can start.
        for i in 0..cos.len() {
            resume_stage(i, &mut cos, &shared)?;
        }

        // Main scheduling loop — drain-biased.
        let mut idle_passes = 0;
        loop {
            if shared.borrow().stages.iter().all(|s| s.ended) {
                break;
            }
            // Clone the error first so the RefCell borrow ends before borrow_mut
            // (if-let would otherwise keep the temporary Ref alive across the body).
            let stage_err = shared
                .borrow()
                .stages
                .iter()
                .find_map(|s| s.error.clone());
            if let Some(err) = stage_err {
                abort_and_drain(&mut cos, &shared);
                return Err(err);
            }

            let next = pick_next(&shared.borrow());
            if let Some(i) = next {
                idle_passes = 0;
                resume_stage(i, &mut cos, &shared)?;
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
                    if idle_passes > 64 {
                        // Deadlock / stall — wake everyone so suspended
                        // coroutines observe abort and exit before we drop
                        // them (corosensei cannot drop a parked stack).
                        let detail = {
                            let g = shared.borrow();
                            g.stages
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| !s.ended)
                                .map(|(i, s)| {
                                    format!(
                                        "{}[{}]: read={:?} read_any={:?} write={:?} commit={:?} yield={}",
                                        s.name,
                                        i,
                                        s.waiting_read,
                                        s.waiting_read_any,
                                        s.waiting_write,
                                        s.waiting_commit,
                                        s.waiting_yield
                                    )
                                })
                                .collect::<Vec<_>>()
                        };
                        abort_and_drain(&mut cos, &shared);
                        return Err(PipeError::msg(format!(
                            "pipeline stalled (possible deadlock): {}",
                            detail.join("; ")
                        )));
                    }
                } else {
                    idle_passes = 0;
                }
            }
        }

        if shared.borrow().trace.is_enabled() {
            shared.borrow().trace.emit("dispatcher end");
        }
        Ok(())
    }
}

/// Mark the network aborted and resume every live stage until they exit so
/// parked coroutines are not dropped (corosensei cannot unwind a parked stack).
fn abort_and_drain(cos: &mut [CoState], shared: &Rc<RefCell<Shared>>) {
    shared.borrow_mut().aborted = true;
    for _ in 0..64 {
        if shared.borrow().stages.iter().all(|s| s.ended) {
            return;
        }
        for i in 0..cos.len() {
            if !shared.borrow().stages[i].ended {
                let _ = resume_stage(i, cos, shared);
            }
        }
    }
}

/// Prefer runnable consumers (waiting read with data/eof available), then
/// writers whose slot is free, then commit waiters that are satisfied, then
/// any non-ended stage that is not clearly blocked forever, then yielders.
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
    // 4. Stages not yet started / not waiting (ready).
    for (i, s) in shared.stages.iter().enumerate() {
        if s.ended {
            continue;
        }
        if s.waiting_read.is_none()
            && s.waiting_read_any.is_empty()
            && s.waiting_write.is_none()
            && s.waiting_commit.is_none()
            && !s.waiting_yield
        {
            return Some(i);
        }
    }
    // 5. Yield waiters: resume only after every other currently-ready stage
    // has had a chance (ELASTIC listening-mode termination, 1.1.11 p.335).
    for (i, s) in shared.stages.iter().enumerate() {
        if !s.ended && s.waiting_yield {
            return Some(i);
        }
    }
    None
}

fn resume_stage(
    index: usize,
    cos: &mut [CoState],
    shared: &Rc<RefCell<Shared>>,
) -> PipeResult<()> {
    if shared.borrow().stages[index].ended {
        return Ok(());
    }

    let state = std::mem::replace(&mut cos[index], CoState::Done);
    match state {
        CoState::Done => Ok(()),
        CoState::NotStarted(entry) => {
            let shared_cl = Rc::clone(shared);
            let stage_id = StageId(index);
            let mut coroutine =
                Coroutine::with_stack(DefaultStack::default(), move |yielder: &Yielder<(), WaitReason>, ()| {
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
                        g.stages[stage_id.0].error =
                            Some(PipeError::stage(format!("{}: {msg}", g.stages[stage_id.0].name)));
                        g.aborted = true;
                        g.trace_stage(stage_id, format!("error {msg}"));
                    }
                });
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
                !self.connections.iter().any(|c| {
                    c.writer == StageId(*i) && c.writer_stream == StreamId::PRIMARY
                })
            })
            .map(StageId)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            Box::new(|ctx| {
                while let Ok(Some(_)) = ctx.read() {}
            }),
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
            Box::new(|ctx| {
                while let Ok(Some(_)) = ctx.read() {}
            }),
        );
        b.connect_primary(a, c);
        let err = b.run().expect_err("stage fail should surface as Err");
        assert!(
            err.to_string().contains("deliberate stage failure"),
            "err={err}"
        );
    }
}
