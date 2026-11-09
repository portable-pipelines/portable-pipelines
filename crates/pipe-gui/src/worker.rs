//! DebugSession lives on this thread. The GUI sends commands and reads snapshots.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{
    self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError, TrySendError,
};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use pipe_core::{
    with_pipeline_message_sink, DebugConfig, DebugEvent, DebugRemote, DebugSession, LiveStage,
    SessionStatus, SlotSnapshot, StageId, StageIoCounts, StageRecordLog, StepOutcome,
};
use pipe_parser::parse_pipeline;
use pipe_stages::{debug_session, with_console_host, ConsoleHost, StageRegistry};

const EVENT_CAP: usize = 128;

/// How long Drop/on_exit wait for the worker after Stop. A blocking host
/// file syscall already in flight is not interrupted; this bound keeps the
/// UI thread from joining forever.
pub const SHUTDOWN_JOIN: Duration = Duration::from_secs(8);

#[derive(Debug, Clone)]
pub enum WorkerCmd {
    Prepare {
        spec: String,
        cwd: PathBuf,
    },
    Run,
    Pause,
    /// `None` prefers a waiting reader; `Some` tries that stage first, then others.
    Step {
        stage: Option<StageId>,
    },
    Stop,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub status: SessionStatus,
    pub events: Vec<DebugEvent>,
    pub slots: Vec<SlotSnapshot>,
    pub live: Vec<LiveStage>,
    pub last_step: Option<String>,
    pub spec_fingerprint: String,
    pub io_counts: Vec<(StageId, StageIoCounts)>,
    pub record_logs: Vec<(StageId, StageRecordLog)>,
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Snapshot(Snapshot),
    Console(String),
    Message(String),
    Error(String),
    Info(String),
}

#[derive(Clone, Copy)]
enum Activity {
    Idle,
    Run { announced: bool },
    Step { preferred: Option<StageId> },
}

pub struct SessionWorker {
    pub cmd_tx: Sender<WorkerCmd>,
    pub ev_rx: Receiver<WorkerEvent>,
    remote: Arc<Mutex<Option<DebugRemote>>>,
    finished: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SessionWorker {
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (ev_tx, ev_rx) = mpsc::sync_channel(EVENT_CAP);
        let remote = Arc::new(Mutex::new(None));
        let finished = Arc::new(AtomicBool::new(false));
        let remote_for_thread = Arc::clone(&remote);
        let finished_for_thread = Arc::clone(&finished);
        let thread = thread::Builder::new()
            .name("pipe-debug-session".into())
            .spawn(move || worker_main(cmd_rx, ev_tx, remote_for_thread, finished_for_thread))
            .expect("debug session thread");
        Self {
            cmd_tx,
            ev_rx,
            remote,
            finished,
            thread: Some(thread),
        }
    }

    pub fn request_pause(&self) {
        if let Ok(g) = self.remote.lock() {
            if let Some(r) = g.as_ref() {
                r.request_pause();
            }
        }
    }

    pub fn request_stop(&self) {
        if let Ok(g) = self.remote.lock() {
            if let Some(r) = g.as_ref() {
                r.request_stop();
            }
        }
        let _ = self.cmd_tx.send(WorkerCmd::Stop);
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    /// Request abort (not run-to-completion) and wait up to `budget`.
    ///
    /// Returns true if the thread exited. Does not detach: a still-running
    /// worker keeps its JoinHandle so Drop can join or the GUI process can
    /// exit on a last-resort timeout.
    pub fn shutdown(&mut self, budget: Duration) -> bool {
        self.request_stop();
        let _ = self.cmd_tx.send(WorkerCmd::Shutdown);
        let start = std::time::Instant::now();
        while !self.finished.load(Ordering::SeqCst) && start.elapsed() < budget {
            thread::sleep(Duration::from_millis(10));
        }
        if !self.finished.load(Ordering::SeqCst) {
            return false;
        }
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
        true
    }

    pub fn try_recv(&self) -> Vec<WorkerEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.ev_rx.try_recv() {
            out.push(ev);
            if out.len() >= EVENT_CAP {
                break;
            }
        }
        out
    }

    pub fn send(&self, cmd: WorkerCmd) {
        let _ = self.cmd_tx.send(cmd);
    }
}

impl Drop for SessionWorker {
    fn drop(&mut self) {
        if self.thread.is_none() {
            return;
        }
        if self.shutdown(SHUTDOWN_JOIN) {
            return;
        }
        // Thread still live after Stop. Do not detach: it may still be in a
        // host write. Join so those side effects finish. The GUI binary's
        // on_exit exits the process before this if the budget already elapsed.
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

struct FinishOnDrop(Arc<AtomicBool>);

impl Drop for FinishOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

struct GuiConsole {
    tx: SyncSender<WorkerEvent>,
}

impl ConsoleHost for GuiConsole {
    fn write_record(&self, record: &pipe_core::Record) {
        let text = format_record(record.as_bytes());
        send_ev(&self.tx, WorkerEvent::Console(text));
    }

    fn read_record(&self) -> Option<pipe_core::Record> {
        send_ev(
            &self.tx,
            WorkerEvent::Info(
                "CONSOLE stdin is not available in the desktop window. Input is treated as end of file so the window does not wait on a hidden terminal. Use LITERAL or a file for input.".into(),
            ),
        );
        None
    }
}

fn send_ev(tx: &SyncSender<WorkerEvent>, ev: WorkerEvent) {
    match tx.try_send(ev) {
        Ok(()) | Err(TrySendError::Disconnected(_)) => {}
        Err(TrySendError::Full(_)) => {}
    }
}

fn format_record(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => format!("<{} bytes, not UTF-8>", bytes.len()),
    }
}

fn worker_main(
    cmd_rx: Receiver<WorkerCmd>,
    ev_tx: SyncSender<WorkerEvent>,
    remote_slot: Arc<Mutex<Option<DebugRemote>>>,
    finished: Arc<AtomicBool>,
) {
    let _done = FinishOnDrop(finished);
    pipe_rexx::install_hook();
    pipe_python::install_hook();
    let host: Arc<dyn ConsoleHost> = Arc::new(GuiConsole { tx: ev_tx.clone() });
    with_console_host(host, || {
        let mut session: Option<DebugSession> = None;
        let mut remote: Option<DebugRemote> = None;
        let mut fingerprint = String::new();
        let mut prev_cwd = std::env::current_dir().ok();
        let mut activity = Activity::Idle;
        let mut pending_step: Option<Option<StageId>> = None;
        loop {
            let cmd = match activity {
                Activity::Idle => match cmd_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                Activity::Run { .. } | Activity::Step { .. } => match cmd_rx.try_recv() {
                    Ok(c) => Some(c),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => break,
                },
            };
            if let Some(cmd) = cmd {
                match cmd {
                    WorkerCmd::Shutdown => {
                        if let Some(s) = session.as_mut() {
                            let _ = s.stop();
                        }
                        break;
                    }
                    WorkerCmd::Prepare { spec, cwd } => {
                        if let Some(s) = session.as_mut() {
                            let _ = s.stop();
                        }
                        session = None;
                        remote = None;
                        activity = Activity::Idle;
                        pending_step = None;
                        fingerprint = spec.clone();
                        if let Err(e) = std::env::set_current_dir(&cwd) {
                            send_ev(
                                &ev_tx,
                                WorkerEvent::Error(format!(
                                    "working directory {}: {e}",
                                    cwd.display()
                                )),
                            );
                            continue;
                        }
                        match parse_pipeline(&spec) {
                            Ok(parsed) => match debug_session(
                                &parsed,
                                &StageRegistry::default(),
                                DebugConfig::bounded(256, 4096),
                            ) {
                                Ok(s) => {
                                    remote = Some(s.remote());
                                    if let Ok(mut slot) = remote_slot.lock() {
                                        *slot = remote.clone();
                                    }
                                    emit_snapshot(&ev_tx, &s, None, &fingerprint);
                                    session = Some(s);
                                    if pending_step.is_none() {
                                        send_ev(
                                            &ev_tx,
                                            WorkerEvent::Info(
                                                "Session ready. Idle: nothing has run yet.".into(),
                                            ),
                                        );
                                    }
                                }
                                Err(e) => send_ev(&ev_tx, WorkerEvent::Error(e.to_string())),
                            },
                            Err(e) => send_ev(&ev_tx, WorkerEvent::Error(format!("parse: {e}"))),
                        }
                    }
                    WorkerCmd::Run => {
                        if session.is_some() {
                            activity = Activity::Run { announced: false };
                            send_ev(&ev_tx, WorkerEvent::Info("running...".into()));
                        }
                    }
                    WorkerCmd::Pause => {
                        if let Some(r) = &remote {
                            r.request_pause();
                        }
                    }
                    WorkerCmd::Stop => {
                        if let Some(r) = &remote {
                            r.request_stop();
                        }
                        if let Some(s) = session.as_mut() {
                            let _ = s.stop();
                            emit_snapshot(&ev_tx, s, None, &fingerprint);
                        }
                        activity = Activity::Idle;
                    }
                    WorkerCmd::Step { stage } => {
                        if session.is_some() {
                            activity = Activity::Step { preferred: stage };
                        } else {
                            pending_step = Some(stage);
                        }
                    }
                }
            }

            if matches!(activity, Activity::Idle) {
                if let Some(preferred) = pending_step.take() {
                    if session.is_some() {
                        activity = Activity::Step { preferred };
                    } else {
                        pending_step = Some(preferred);
                    }
                }
            }
            match activity {
                Activity::Idle => {}
                Activity::Run { announced } => {
                    if let Some(s) = session.as_mut() {
                        let sink = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                        let status = with_pipeline_message_sink(sink.clone(), || s.poll());
                        drain_messages(&ev_tx, &sink);
                        if status.is_terminal() || matches!(status, SessionStatus::Paused) {
                            activity = Activity::Idle;
                            emit_snapshot(&ev_tx, s, None, &fingerprint);
                            send_ev(
                                &ev_tx,
                                WorkerEvent::Info(format!("run finished: {status:?}")),
                            );
                        } else if !announced {
                            emit_snapshot(&ev_tx, s, None, &fingerprint);
                            activity = Activity::Run { announced: true };
                        }
                    } else {
                        activity = Activity::Idle;
                    }
                }
                Activity::Step { preferred } => {
                    if let Some(s) = session.as_mut() {
                        let sink = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                        let (ran, out) = with_pipeline_message_sink(sink.clone(), || {
                            s.step_one_record(preferred)
                        });
                        drain_messages(&ev_tx, &sink);
                        let note = describe_step(&out, ran, s);
                        emit_snapshot(&ev_tx, s, Some(note), &fingerprint);
                        activity = Activity::Idle;
                    } else {
                        activity = Activity::Idle;
                    }
                }
            }
        }
        if let Some(dir) = prev_cwd.take() {
            let _ = std::env::set_current_dir(dir);
        }
        if let Ok(mut slot) = remote_slot.lock() {
            *slot = None;
        }
        let _ = session;
    });
}

fn drain_messages(
    tx: &SyncSender<WorkerEvent>,
    sink: &std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) {
    for line in sink.borrow_mut().drain(..) {
        send_ev(tx, WorkerEvent::Message(line));
    }
}

fn emit_snapshot(
    tx: &SyncSender<WorkerEvent>,
    session: &DebugSession,
    last_step: Option<String>,
    fingerprint: &str,
) {
    send_ev(
        tx,
        WorkerEvent::Snapshot(Snapshot {
            status: session.status(),
            events: session.events(),
            slots: session.slots(),
            live: session.live_stages(),
            last_step,
            spec_fingerprint: fingerprint.to_string(),
            io_counts: session.io_counts(),
            record_logs: session.record_logs(),
        }),
    );
}

fn describe_step(outcome: &StepOutcome, stage: Option<StageId>, session: &DebugSession) -> String {
    let name = stage
        .and_then(|id| {
            session
                .live_stages()
                .into_iter()
                .find(|l| l.id == id)
                .map(|l| l.name)
        })
        .unwrap_or_else(|| "stage".into());
    match outcome {
        StepOutcome::Cycle(c) => {
            let kind = match c.kind {
                pipe_core::CycleKind::ReadThenQuiet => "read a record",
                pipe_core::CycleKind::BufferedInput => "read a record and is buffering it",
                pipe_core::CycleKind::EmittedOutput => "wrote a record",
                pipe_core::CycleKind::ProducedOutput => "produced a record",
                pipe_core::CycleKind::InputClosed => "input ended",
            };
            format!("{name}: {kind}")
        }
        StepOutcome::BlockedOnCallPipe { .. } => {
            format!("Waiting for {name} (CALLPIPE nested stages)")
        }
        StepOutcome::BlockedOnHelper => {
            format!("Waiting for {name} (Python or REXX helper)")
        }
        StepOutcome::Ended { rc } => format!("{name}: this stage ended (rc {rc})."),
        StepOutcome::PipelineEnded { rc } => format!("Pipeline ended (rc {rc})."),
        StepOutcome::Stopped { rc } => format!("Stopped (rc {rc})."),
        StepOutcome::Failed(m) => format!("Failed: {m}"),
        StepOutcome::NoProgress { reason } => reason.clone(),
    }
}
