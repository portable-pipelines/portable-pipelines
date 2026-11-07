//! DebugSession lives on this thread. The GUI sends commands and reads snapshots.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use pipe_core::{
    with_pipeline_message_sink, DebugConfig, DebugEvent, DebugRemote, DebugSession, LiveStage,
    SessionStatus, SlotSnapshot, StageId, StepOutcome,
};
use pipe_parser::parse_pipeline;
use pipe_stages::{debug_session, with_console_host, ConsoleHost, StageRegistry};

const EVENT_CAP: usize = 128;

#[derive(Debug, Clone)]
pub enum WorkerCmd {
    Prepare { spec: String, cwd: PathBuf },
    Run,
    Pause,
    Step { stage: StageId },
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
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Snapshot(Snapshot),
    Console(String),
    Message(String),
    Error(String),
    Info(String),
}

pub struct SessionWorker {
    pub cmd_tx: Sender<WorkerCmd>,
    pub ev_rx: Receiver<WorkerEvent>,
    remote: Arc<Mutex<Option<DebugRemote>>>,
    thread: Option<JoinHandle<()>>,
}

impl SessionWorker {
    pub fn start() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (ev_tx, ev_rx) = mpsc::sync_channel(EVENT_CAP);
        let remote = Arc::new(Mutex::new(None));
        let remote_for_thread = Arc::clone(&remote);
        let thread = thread::Builder::new()
            .name("pipe-debug-session".into())
            .spawn(move || worker_main(cmd_rx, ev_tx, remote_for_thread))
            .expect("debug session thread");
        Self {
            cmd_tx,
            ev_rx,
            remote,
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
        self.request_stop();
        let _ = self.cmd_tx.send(WorkerCmd::Shutdown);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
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
) {
    pipe_rexx::install_hook();
    pipe_python::install_hook();
    let host: Arc<dyn ConsoleHost> = Arc::new(GuiConsole { tx: ev_tx.clone() });
    with_console_host(host, || {
        let mut session: Option<DebugSession> = None;
        let mut remote: Option<DebugRemote> = None;
        let mut fingerprint = String::new();
        let mut prev_cwd = std::env::current_dir().ok();
        loop {
            let cmd = match cmd_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
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
                                    send_ev(
                                        &ev_tx,
                                        WorkerEvent::Info(
                                            "Session ready. Idle: nothing has run yet.".into(),
                                        ),
                                    );
                                }
                                Err(e) => send_ev(&ev_tx, WorkerEvent::Error(e.to_string())),
                            },
                            Err(e) => send_ev(&ev_tx, WorkerEvent::Error(format!("parse: {e}"))),
                        }
                    }
                    WorkerCmd::Run => {
                        if let Some(s) = session.as_mut() {
                            let sink = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                            send_ev(&ev_tx, WorkerEvent::Info("running...".into()));
                            let status = with_pipeline_message_sink(sink.clone(), || s.run());
                            drain_messages(&ev_tx, &sink);
                            emit_snapshot(&ev_tx, s, None, &fingerprint);
                            send_ev(
                                &ev_tx,
                                WorkerEvent::Info(format!("run finished: {status:?}")),
                            );
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
                    }
                    WorkerCmd::Step { stage } => {
                        if let Some(s) = session.as_mut() {
                            let outcome = s.step_stage(stage);
                            let note = describe_step(&outcome);
                            emit_snapshot(&ev_tx, s, Some(note), &fingerprint);
                        }
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
        }),
    );
}

fn describe_step(outcome: &StepOutcome) -> String {
    match outcome {
        StepOutcome::Cycle(c) => format!(
            "cycle {:?} reads={} writes={} discarded={} short={} eof={}",
            c.kind, c.reads_taken, c.writes_offered, c.writes_discarded, c.saw_short, c.saw_eof
        ),
        StepOutcome::BlockedOnCallPipe { children } => format!(
            "CALLPIPE in flight. Nested stages {:?}. Step those ids; this is not one record.",
            children.iter().map(|s| s.0).collect::<Vec<_>>()
        ),
        StepOutcome::BlockedOnHelper => {
            "waiting on a REXX/Python helper frame. Not a record cycle.".into()
        }
        StepOutcome::Ended { rc } => format!("stage ended rc={rc}"),
        StepOutcome::PipelineEnded { rc } => format!("pipeline ended rc={rc}"),
        StepOutcome::Stopped { rc } => format!("stopped rc={rc}"),
        StepOutcome::Failed(m) => format!("failed: {m}"),
        StepOutcome::NoProgress { reason } => reason.clone(),
    }
}
