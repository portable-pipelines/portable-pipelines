//! Headless debug events and selected-stage record stepping.
//!
//! A debug session records dispatcher I/O. It does not start coroutines until
//! [`crate::DebugSession::run`] or [`crate::DebugSession::step_stage`]. Pause and
//! step take effect at `StageCtx` read/write/short boundaries. A stage that
//! computes without calling those services (a large SORT, a host file write
//! loop body, CONSOLE stdin) will not pause until the next boundary. DELAY
//! sleeps in the dispatcher in 1ms slices; a pause request is observed between
//! slices.

use std::collections::{HashSet, VecDeque};

use crate::record::{Record, StreamId};
use crate::StageId;

/// How many debug events a session keeps by default.
pub const DEFAULT_EVENT_CAPACITY: usize = 256;

/// How many payload bytes each event retains by default.
pub const DEFAULT_RECORD_BYTES: usize = 4096;

/// Configuration for a [`crate::DebugSession`].
#[derive(Debug, Clone)]
pub struct DebugConfig {
    /// Ring-buffer size for [`crate::DebugSession::events`]. Oldest events drop.
    pub event_capacity: usize,
    /// Maximum payload bytes stored on each record-bearing event.
    pub max_record_bytes: usize,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            event_capacity: DEFAULT_EVENT_CAPACITY,
            max_record_bytes: DEFAULT_RECORD_BYTES,
        }
    }
}

impl DebugConfig {
    pub fn bounded(event_capacity: usize, max_record_bytes: usize) -> Self {
        Self {
            event_capacity: event_capacity.max(1),
            max_record_bytes,
        }
    }
}

/// Bytes copied into a debug event. This is not a live slot alias.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordPreview {
    pub len: usize,
    pub bytes: Vec<u8>,
}

impl RecordPreview {
    pub fn from_record(record: &Record, max_bytes: usize) -> Self {
        let data = record.as_bytes();
        let take = max_bytes.min(data.len());
        Self {
            len: data.len(),
            bytes: data[..take].to_vec(),
        }
    }
}

/// One dispatcher observation. Causal claims beyond this list are not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugEvent {
    pub stage: StageId,
    pub name: String,
    pub kind: DebugEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugEventKind {
    StageStart,
    StageEnd {
        rc: i32,
    },
    ReadTaken {
        stream: StreamId,
        record: RecordPreview,
    },
    ReadWait {
        stream: StreamId,
    },
    ReadEof {
        stream: StreamId,
    },
    ReadSevered {
        stream: StreamId,
    },
    WriteOffered {
        stream: StreamId,
        record: RecordPreview,
    },
    WriteWait {
        stream: StreamId,
    },
    WriteDiscard {
        stream: StreamId,
        record: RecordPreview,
    },
    WriteSevered {
        stream: StreamId,
    },
    WriteDeliver {
        stream: StreamId,
    },
    ShortInput {
        stream: StreamId,
    },
    ShortOutput {
        stream: StreamId,
    },
    Short {
        input: StreamId,
        output: StreamId,
    },
    CallPipeEnter,
    CallPipeExit,
    AddPipeEnter,
    AddPipeExit,
    HelperWait,
}

impl DebugEventKind {
    pub fn is_wait(&self) -> bool {
        matches!(
            self,
            Self::ReadWait { .. }
                | Self::WriteWait { .. }
                | Self::WriteDeliver { .. }
                | Self::HelperWait
        )
    }
}

/// Observable record-cycle classification from events on one selected stage.
///
/// This is not "the stage consumed one input and emitted one output." Writes
/// listed on [`RecordCycle`] happened after the opening read (or with no read
/// for a producer) and before the closing wait. For FANIN, COLLATE, REXX,
/// Python, and COMMAND that may be one of several inputs, or several outputs,
/// without a proven inner mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleKind {
    /// READ taken, then the stage waited on READ again or closed input. Zero or
    /// more WRITEs happened in between (filters, FANOUT, SPLIT, TAKE).
    ReadThenQuiet,
    /// READ taken and the next wait/EOF happened with no WRITE. SORT/BUFFER ingest.
    BufferedInput,
    /// After this stage has already seen input EOF, one WRITE was offered.
    EmittedOutput,
    /// One WRITE offered with no READ taken in this cycle (LITERAL, `<` files).
    ProducedOutput,
    /// READ EOF or SEVERED with no record taken in this cycle.
    InputClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordCycle {
    pub kind: CycleKind,
    pub reads_taken: usize,
    pub writes_offered: usize,
    pub writes_discarded: usize,
    pub saw_short: bool,
    pub saw_eof: bool,
    pub saw_severed: bool,
}

/// One live connection as the inspector should show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotSnapshot {
    pub writer: StageId,
    pub writer_stream: StreamId,
    pub reader: StageId,
    pub reader_stream: StreamId,
    pub record: Option<RecordPreview>,
    pub peeked: bool,
    pub severed: bool,
    pub writer_done: bool,
}

/// One live stage, including CALLPIPE children spliced into the same dispatcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveStage {
    pub id: StageId,
    pub name: String,
    pub ended: bool,
    pub return_code: i32,
    pub waiting_debug: bool,
    pub waiting_callpipe: bool,
    pub waiting_helper: bool,
    pub commit_group: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    /// Coroutines have not been started.
    Idle,
    Running,
    Paused,
    Ended {
        rc: i32,
    },
    Stopped {
        rc: i32,
    },
    Failed(String),
}

impl SessionStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Ended { .. } | Self::Stopped { .. } | Self::Failed(_)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Cycle(RecordCycle),
    /// Selected stage issued CALLPIPE. Nested stages are live `StageId`s;
    /// step those. This outcome does not run the nested graph as one record.
    BlockedOnCallPipe {
        children: Vec<StageId>,
    },
    /// Selected stage is waiting on a helper-process frame (REXX/Python).
    BlockedOnHelper,
    Ended {
        rc: i32,
    },
    PipelineEnded {
        rc: i32,
    },
    Stopped {
        rc: i32,
    },
    Failed(String),
    /// Selected stage is not runnable without claiming a false record cycle.
    NoProgress {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DebugPolicy {
    /// Record events; do not force a hold.
    Run,
    /// Hold at the next non-wait I/O boundary.
    Pause,
    /// Hold when the selected-stage step machine completes a cycle.
    Step,
}

pub(crate) struct DebugCtl {
    pub policy: DebugPolicy,
    pub events: VecDeque<DebugEvent>,
    pub cap: usize,
    pub max_record_bytes: usize,
    pub step: Option<StepMachine>,
    pub step_complete: bool,
    pub boundary_since_resume: bool,
    /// When true, `waiting_debug` stages stay parked.
    pub holding: bool,
    /// Stages that have observed READ EOF. Used so a later step can classify emit.
    eof_stages: HashSet<usize>,
}

impl DebugCtl {
    pub fn new(config: DebugConfig) -> Self {
        Self {
            policy: DebugPolicy::Pause,
            events: VecDeque::new(),
            cap: config.event_capacity.max(1),
            max_record_bytes: config.max_record_bytes,
            step: None,
            step_complete: false,
            boundary_since_resume: false,
            holding: true,
            eof_stages: HashSet::new(),
        }
    }

    pub fn preview(&self, record: &Record) -> RecordPreview {
        RecordPreview::from_record(record, self.max_record_bytes)
    }

    pub fn push(&mut self, event: DebugEvent) {
        if self.events.len() >= self.cap {
            self.events.pop_front();
        }
        if matches!(event.kind, DebugEventKind::ReadEof { .. }) {
            self.eof_stages.insert(event.stage.0);
        }
        if !event.kind.is_wait() {
            self.boundary_since_resume = true;
        }
        if let Some(step) = self.step.as_mut() {
            if step.on_event(&event) {
                self.step_complete = true;
            }
        }
        self.events.push_back(event);
    }

    pub fn begin_step(&mut self, selected: StageId) {
        self.policy = DebugPolicy::Step;
        self.holding = false;
        let seen_eof = self.eof_stages.contains(&selected.0);
        self.step = Some(StepMachine::new(selected, seen_eof));
        self.step_complete = false;
        self.boundary_since_resume = false;
    }

    pub fn begin_run(&mut self) {
        self.policy = DebugPolicy::Run;
        self.holding = false;
        self.step = None;
        self.step_complete = false;
        self.boundary_since_resume = false;
    }

    pub fn request_pause(&mut self) {
        self.policy = DebugPolicy::Pause;
    }

    pub fn park(&mut self) {
        self.holding = true;
    }

    pub fn waiters_runnable(&self) -> bool {
        !self.holding
    }

    pub fn should_hold_now(&self, kind: &DebugEventKind) -> bool {
        if kind.is_wait()
            || matches!(
                kind,
                DebugEventKind::CallPipeEnter
                    | DebugEventKind::CallPipeExit
                    | DebugEventKind::AddPipeEnter
                    | DebugEventKind::AddPipeExit
                    | DebugEventKind::StageStart
            )
        {
            return false;
        }
        match self.policy {
            DebugPolicy::Run => false,
            DebugPolicy::Pause => true,
            DebugPolicy::Step => self.step_complete,
        }
    }
}

/// Selected-stage stepper. Only events on `selected` change the cycle.
pub(crate) struct StepMachine {
    selected: StageId,
    seen_eof: bool,
    reads_taken: usize,
    writes_offered: usize,
    writes_discarded: usize,
    saw_short: bool,
    saw_eof: bool,
    saw_severed: bool,
    complete: Option<RecordCycle>,
    blocked_callpipe: bool,
    blocked_helper: bool,
}

impl StepMachine {
    fn new(selected: StageId, seen_eof: bool) -> Self {
        Self {
            selected,
            seen_eof,
            reads_taken: 0,
            writes_offered: 0,
            writes_discarded: 0,
            saw_short: false,
            saw_eof: false,
            saw_severed: false,
            complete: None,
            blocked_callpipe: false,
            blocked_helper: false,
        }
    }

    fn cycle(&self, kind: CycleKind) -> RecordCycle {
        RecordCycle {
            kind,
            reads_taken: self.reads_taken,
            writes_offered: self.writes_offered,
            writes_discarded: self.writes_discarded,
            saw_short: self.saw_short,
            saw_eof: self.saw_eof,
            saw_severed: self.saw_severed,
        }
    }

    fn finish(&mut self, kind: CycleKind) -> bool {
        if self.complete.is_none() {
            self.complete = Some(self.cycle(kind));
        }
        true
    }

    /// Returns true once this step should hold.
    fn on_event(&mut self, event: &DebugEvent) -> bool {
        if event.stage != self.selected {
            return false;
        }
        if self.complete.is_some() {
            return true;
        }
        match &event.kind {
            DebugEventKind::CallPipeEnter => {
                self.blocked_callpipe = true;
                true
            }
            DebugEventKind::HelperWait => {
                self.blocked_helper = true;
                true
            }
            DebugEventKind::ReadTaken { .. } => {
                self.reads_taken += 1;
                false
            }
            DebugEventKind::WriteOffered { .. } => {
                self.writes_offered += 1;
                if self.reads_taken == 0 {
                    let kind = if self.seen_eof {
                        CycleKind::EmittedOutput
                    } else {
                        CycleKind::ProducedOutput
                    };
                    self.finish(kind)
                } else {
                    false
                }
            }
            DebugEventKind::WriteDiscard { .. } => {
                self.writes_discarded += 1;
                false
            }
            DebugEventKind::ReadWait { .. } => {
                if self.reads_taken > 0 {
                    let kind = if self.writes_offered == 0 && self.writes_discarded == 0 {
                        CycleKind::BufferedInput
                    } else {
                        CycleKind::ReadThenQuiet
                    };
                    self.finish(kind)
                } else {
                    false
                }
            }
            DebugEventKind::ReadEof { .. } => {
                self.saw_eof = true;
                self.seen_eof = true;
                if self.reads_taken > 0 {
                    let kind = if self.writes_offered == 0 && self.writes_discarded == 0 {
                        CycleKind::BufferedInput
                    } else {
                        CycleKind::ReadThenQuiet
                    };
                    self.finish(kind)
                } else if self.writes_offered == 0 {
                    self.finish(CycleKind::InputClosed)
                } else {
                    false
                }
            }
            DebugEventKind::ReadSevered { .. } | DebugEventKind::WriteSevered { .. } => {
                self.saw_severed = true;
                if self.reads_taken > 0 {
                    self.finish(CycleKind::ReadThenQuiet)
                } else {
                    self.finish(CycleKind::InputClosed)
                }
            }
            DebugEventKind::ShortInput { .. }
            | DebugEventKind::ShortOutput { .. }
            | DebugEventKind::Short { .. } => {
                self.saw_short = true;
                false
            }
            DebugEventKind::StageEnd { .. } => {
                if self.reads_taken > 0 {
                    let kind = if self.writes_offered == 0 && self.writes_discarded == 0 {
                        CycleKind::BufferedInput
                    } else {
                        CycleKind::ReadThenQuiet
                    };
                    self.finish(kind)
                } else if self.writes_offered > 0 {
                    let kind = if self.seen_eof {
                        CycleKind::EmittedOutput
                    } else {
                        CycleKind::ProducedOutput
                    };
                    self.finish(kind)
                } else {
                    self.finish(CycleKind::InputClosed)
                }
            }
            DebugEventKind::WriteWait { .. }
            | DebugEventKind::WriteDeliver { .. }
            | DebugEventKind::StageStart
            | DebugEventKind::CallPipeExit
            | DebugEventKind::AddPipeEnter
            | DebugEventKind::AddPipeExit => false,
        }
    }

    pub fn outcome(&self, ended: bool, rc: i32, children: &[StageId]) -> Option<StepOutcome> {
        if self.blocked_callpipe {
            return Some(StepOutcome::BlockedOnCallPipe {
                children: children.to_vec(),
            });
        }
        if self.blocked_helper {
            return Some(StepOutcome::BlockedOnHelper);
        }
        if let Some(cycle) = &self.complete {
            if ended && cycle.reads_taken == 0 && cycle.writes_offered == 0 {
                return Some(StepOutcome::Ended { rc });
            }
            return Some(StepOutcome::Cycle(cycle.clone()));
        }
        if ended {
            return Some(StepOutcome::Ended { rc });
        }
        None
    }
}

#[cfg(test)]
mod step_machine_tests {
    use super::*;

    fn ev(kind: DebugEventKind) -> DebugEvent {
        DebugEvent {
            stage: StageId(1),
            name: "LOCATE".into(),
            kind,
        }
    }

    fn rec() -> RecordPreview {
        RecordPreview {
            len: 1,
            bytes: vec![b'a'],
        }
    }

    #[test]
    fn locate_hit_then_read_wait() {
        let mut m = StepMachine::new(StageId(1), false);
        let s = StreamId::PRIMARY;
        assert!(!m.on_event(&ev(DebugEventKind::ReadTaken {
            stream: s,
            record: rec()
        })));
        assert!(!m.on_event(&ev(DebugEventKind::WriteOffered {
            stream: s,
            record: rec()
        })));
        assert!(m.on_event(&ev(DebugEventKind::ReadWait { stream: s })));
        let StepOutcome::Cycle(c) = m.outcome(false, 0, &[]).unwrap() else {
            panic!("expected cycle");
        };
        assert_eq!(c.kind, CycleKind::ReadThenQuiet);
        assert_eq!(c.writes_offered, 1);
    }

    #[test]
    fn sort_ingest_is_buffered() {
        let mut m = StepMachine::new(StageId(1), false);
        let s = StreamId::PRIMARY;
        assert!(!m.on_event(&ev(DebugEventKind::ReadTaken {
            stream: s,
            record: rec()
        })));
        assert!(m.on_event(&ev(DebugEventKind::ReadWait { stream: s })));
        let StepOutcome::Cycle(c) = m.outcome(false, 0, &[]).unwrap() else {
            panic!("expected cycle");
        };
        assert_eq!(c.kind, CycleKind::BufferedInput);
        assert_eq!(c.writes_offered, 0);
    }

    #[test]
    fn locate_miss_is_discard_then_quiet() {
        let mut m = StepMachine::new(StageId(1), false);
        let s = StreamId::PRIMARY;
        let sec = StreamId::SECONDARY;
        assert!(!m.on_event(&ev(DebugEventKind::ReadTaken {
            stream: s,
            record: rec()
        })));
        assert!(!m.on_event(&ev(DebugEventKind::WriteDiscard {
            stream: sec,
            record: rec()
        })));
        assert!(m.on_event(&ev(DebugEventKind::ReadWait { stream: s })));
        let StepOutcome::Cycle(c) = m.outcome(false, 0, &[]).unwrap() else {
            panic!("expected cycle");
        };
        assert_eq!(c.kind, CycleKind::ReadThenQuiet);
        assert_eq!(c.writes_offered, 0);
        assert_eq!(c.writes_discarded, 1);
    }

    #[test]
    fn sort_eof_then_emit() {
        let mut m = StepMachine::new(StageId(1), false);
        let s = StreamId::PRIMARY;
        m.on_event(&ev(DebugEventKind::ReadTaken {
            stream: s,
            record: rec(),
        }));
        assert!(m.on_event(&ev(DebugEventKind::ReadEof { stream: s })));
        let StepOutcome::Cycle(c) = m.outcome(false, 0, &[]).unwrap() else {
            panic!("expected ingest eof cycle");
        };
        assert_eq!(c.kind, CycleKind::BufferedInput);
        assert!(c.saw_eof);

        let mut m = StepMachine::new(StageId(1), true);
        assert!(m.on_event(&ev(DebugEventKind::WriteOffered {
            stream: s,
            record: rec()
        })));
        let StepOutcome::Cycle(c) = m.outcome(false, 0, &[]).unwrap() else {
            panic!("expected emit");
        };
        assert_eq!(c.kind, CycleKind::EmittedOutput);
    }
}
