//! Bounded record previews. No execution. Structure comes from the bytes
//! themselves (JSON objects) or is treated as a single text cell — never guessed CSV.

use pipe_core::{
    DebugEvent, DebugEventKind, RecordPreview, SlotSnapshot, StageId, StageIoCounts,
    StageRecordLog, StreamId, DEFAULT_RECORD_LOG_CAPACITY,
};

const TABLE_ROWS: usize = 24;
const TABLE_COLS: usize = 12;
const CELL_CHARS: usize = 48;
/// Visible rows in a compact nested sheet before the grid scrolls.
pub const HOVER_SHEET_ROWS: usize = 8;
/// Visible rows in the Records pane sheet before the grid scrolls.
pub const PANE_SHEET_ROWS: usize = 10;
pub const RECORD_HISTORY_CAP: usize = DEFAULT_RECORD_LOG_CAPACITY;
/// One-line hint on the graph peek. Record grids live in the Records pane.
pub const CLICK_RECORDS_HINT: &str = "Click to see records below.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordFocus {
    Stage(StageId),
    /// One connection. Outgoing history is the writer's writes on `writer_stream` only.
    Edge {
        writer: StageId,
        writer_stream: u32,
        reader: StageId,
        reader_stream: u32,
    },
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusCounts {
    pub received: usize,
    pub sent: usize,
    pub discarded: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingKind {
    Waiting,
    Empty,
    WriterFinished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingItem {
    pub kind: WaitingKind,
    pub writer: StageId,
    pub reader: StageId,
    pub writer_stream: u32,
    pub reader_stream: u32,
    pub record: Option<RecordPreview>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedKind {
    CameIn,
    WentOut,
    Discarded,
    InputEnded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedItem {
    pub kind: ObservedKind,
    pub stream: u32,
    pub record: Option<RecordPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MiniCell {
    Text(String),
    Nested { summary: String, json: String },
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiniKind {
    /// Unstructured bytes shown as one column of rows. No field names.
    Text,
    /// JSON object or array of objects — keys are the real fields.
    Structured,
    Binary,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniTable {
    pub kind: MiniKind,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<MiniCell>>,
    pub truncated: bool,
}

/// Display names for the graph peek and the Records pane. Copy only; does not change scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusLabels {
    pub title: String,
    pub stage: String,
    pub writer: String,
    pub reader: String,
    /// Topology verb for the focused stage (`SORT`, `BUFFER`). Empty on edges.
    pub verb: String,
    pub output_count: u32,
    pub input_count: u32,
    pub names: Vec<(StageId, String)>,
}

impl Default for FocusLabels {
    fn default() -> Self {
        Self {
            title: "connection".into(),
            stage: String::new(),
            writer: String::new(),
            reader: String::new(),
            verb: String::new(),
            output_count: 1,
            input_count: 1,
            names: Vec::new(),
        }
    }
}

impl FocusLabels {
    pub fn edge(writer: &str, reader: &str) -> Self {
        Self {
            title: edge_title(writer, reader),
            stage: writer.into(),
            writer: writer.into(),
            reader: reader.into(),
            verb: String::new(),
            output_count: 1,
            input_count: 0,
            names: Vec::new(),
        }
    }

    pub fn name(&self, id: StageId) -> &str {
        self.names
            .iter()
            .find(|(s, _)| *s == id)
            .map(|(_, n)| n.as_str())
            .unwrap_or("stage")
    }
}

pub fn edge_title(writer: &str, reader: &str) -> String {
    format!("{writer} -> {reader}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingSectionCopy {
    pub heading: String,
    pub tooltip: String,
    pub kind: WaitingKind,
    pub record: Option<RecordPreview>,
    pub finished_note: Option<String>,
    /// `Some(true)` belongs with Sent (this stage wrote the record).
    /// `Some(false)` belongs with Received (this stage has not read it yet).
    /// `None` is stage-level (idle, or mixed empty slots).
    pub on_output: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRowCopy {
    pub caption: Option<String>,
    pub tooltip: String,
    pub still_waiting: bool,
    pub kind: ObservedKind,
    pub record: Option<RecordPreview>,
    pub stream: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipCopy {
    pub kind: ChipKind,
    pub text: String,
    pub tooltip: String,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Waiting,
    Received,
    Sent,
    Discarded,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoldingCopy {
    pub count: usize,
    pub line: String,
    pub tooltip: String,
}

pub struct PreviewCopy {
    pub summary: String,
    pub chips: Vec<ChipCopy>,
    pub holding: Option<HoldingCopy>,
    pub waiting: Vec<WaitingSectionCopy>,
    pub no_connection: bool,
    pub recent_heading: String,
    pub recent_empty: Option<String>,
    pub recent: Vec<RecentRowCopy>,
    pub received_heading: Option<String>,
    pub sent_heading: Option<String>,
    pub discarded_heading: Option<String>,
    pub received_table: MiniTable,
    pub sent_table: MiniTable,
    pub discarded_table: MiniTable,
    pub received_first_seq: usize,
    pub sent_first_seq: usize,
    pub discarded_first_seq: usize,
    pub no_more_input: bool,
    pub sent_still_waiting: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordHistory {
    pub received: Vec<RecordPreview>,
    pub sent: Vec<RecordPreview>,
    pub discarded: Vec<RecordPreview>,
    pub received_total: usize,
    pub sent_total: usize,
    pub discarded_total: usize,
    pub no_more_input: bool,
}

/// In-badge digits after the right-pointing flow arrow, with a leading space so it reads `→ 7`.
pub fn in_badge_label(n: usize) -> String {
    format!(" {n}")
}

/// Out-badge digits before the outgoing arrow, with a trailing space so it reads `6 →`.
pub fn out_badge_label(n: usize) -> String {
    format!("{n} ")
}

pub fn first_seq(kept: usize, total: usize) -> usize {
    total.saturating_sub(kept).saturating_add(1)
}

pub fn dir_heading(word: &str, kept: usize, total: usize) -> Option<String> {
    if kept == 0 {
        None
    } else if kept < total {
        Some(format!("{word} (showing last {kept} of {total})"))
    } else {
        Some(word.to_string())
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct StageBadges {
    /// Records this stage has read. 0 on sources with no input — no in badge.
    pub received: usize,
    pub sent: usize,
    pub discarded: usize,
    pub waiting_in: bool,
    /// Known internal bag size for SORT/BUFFER. `None` if not a bufferer or unknown.
    pub holding: Option<usize>,
    /// True when the live stage has ended and will do no more work.
    pub ended: bool,
    pub in_tip: String,
    pub out_tip: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageOverviewRow {
    pub name: String,
    pub sent: usize,
    pub discarded: usize,
    pub waiting_in: bool,
    pub holding: Option<usize>,
    pub holding_line: Option<String>,
    pub ended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingWhere {
    pub line: String,
    pub tooltip: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOverview {
    pub has_session: bool,
    pub status: String,
    pub steps: usize,
    pub stages: Vec<StageOverviewRow>,
    pub waiting: Vec<WaitingWhere>,
}

pub fn run_overview(
    snap: Option<&crate::worker::Snapshot>,
    names: &[(StageId, String, String)],
    steps: usize,
) -> RunOverview {
    let Some(snap) = snap else {
        return RunOverview {
            has_session: false,
            status: "No session yet".into(),
            steps: 0,
            stages: Vec::new(),
            waiting: Vec::new(),
        };
    };
    let status = match &snap.status {
        pipe_core::SessionStatus::Idle => "Idle".into(),
        pipe_core::SessionStatus::Running => "Running".into(),
        pipe_core::SessionStatus::Paused => "Paused".into(),
        pipe_core::SessionStatus::Ended { rc } => format!("Ended (code {rc})"),
        pipe_core::SessionStatus::Stopped { rc } => format!("Stopped (code {rc})"),
        pipe_core::SessionStatus::Failed(m) => format!("Failed: {m}"),
    };
    let mut stages = Vec::new();
    for (id, name, verb) in names {
        let c = counts_for_stage(&snap.events, *id, &snap.io_counts);
        let waiting_in = snap
            .slots
            .iter()
            .any(|s| s.reader == *id && s.record.is_some());
        let holding = holding_count(verb, c);
        let ended = snap.live.iter().any(|l| l.id == *id && l.ended);
        stages.push(StageOverviewRow {
            name: name.clone(),
            sent: c.sent,
            discarded: c.discarded,
            waiting_in,
            holding,
            holding_line: holding.map(|n| holding_line(verb, n)),
            ended,
        });
    }
    let mut waiting = Vec::new();
    for slot in &snap.slots {
        if slot.record.is_none() {
            continue;
        }
        let writer = names
            .iter()
            .find(|(id, _, _)| *id == slot.writer)
            .map(|(_, n, _)| n.as_str())
            .unwrap_or("stage");
        let reader = names
            .iter()
            .find(|(id, _, _)| *id == slot.reader)
            .map(|(_, n, _)| n.as_str())
            .unwrap_or("stage");
        waiting.push(WaitingWhere {
            line: format!("Waiting for {reader} to read"),
            tooltip: format!("Waiting for {reader} to read (from {writer})"),
        });
    }
    RunOverview {
        has_session: true,
        status,
        steps,
        stages,
        waiting,
    }
}

pub fn stage_badges(
    events: &[DebugEvent],
    slots: &[SlotSnapshot],
    id: StageId,
    verb: &str,
    ended: bool,
) -> StageBadges {
    stage_badges_named(events, slots, id, verb, ended, &[], &[])
}

pub fn stage_badges_on(
    snap: &crate::worker::Snapshot,
    id: StageId,
    verb: &str,
    ended: bool,
) -> StageBadges {
    stage_badges_named(
        &snap.events,
        &snap.slots,
        id,
        verb,
        ended,
        &[],
        &snap.io_counts,
    )
}

pub fn stage_badges_named(
    events: &[DebugEvent],
    slots: &[SlotSnapshot],
    id: StageId,
    verb: &str,
    ended: bool,
    names: &[(StageId, String)],
    tallies: &[(StageId, StageIoCounts)],
) -> StageBadges {
    let c = counts_for_stage(events, id, tallies);
    StageBadges {
        received: c.received,
        sent: c.sent,
        discarded: c.discarded,
        waiting_in: slots.iter().any(|s| s.reader == id && s.record.is_some()),
        holding: holding_count(verb, c),
        ended,
        in_tip: in_count_tooltip(c.received, events, slots, id, names),
        out_tip: out_count_tooltip(c.sent),
    }
}

pub fn in_count_tooltip(
    received: usize,
    events: &[DebugEvent],
    slots: &[SlotSnapshot],
    id: StageId,
    names: &[(StageId, String)],
) -> String {
    if received == 0 {
        return String::new();
    }
    let mut line = format!("Read {}", records_phrase(received, "in"));
    let parts = received_by_writer(events, slots, id, names);
    let sum: usize = parts.iter().map(|(_, n)| *n).sum();
    if parts.len() > 1 && sum == received {
        for (name, n) in parts {
            line.push('\n');
            line.push_str(&format!("{n} from {name}"));
        }
    }
    line
}

pub fn out_count_tooltip(sent: usize) -> String {
    if sent == 0 {
        String::new()
    } else {
        format!("Sent {}", records_phrase(sent, "out"))
    }
}

fn received_by_writer(
    events: &[DebugEvent],
    slots: &[SlotSnapshot],
    id: StageId,
    names: &[(StageId, String)],
) -> Vec<(String, usize)> {
    let mut by_stream: Vec<(u32, usize)> = Vec::new();
    for e in events.iter().filter(|e| e.stage == id) {
        let DebugEventKind::ReadTaken { stream, .. } = &e.kind else {
            continue;
        };
        if let Some(row) = by_stream.iter_mut().find(|(s, _)| *s == stream.0) {
            row.1 += 1;
        } else {
            by_stream.push((stream.0, 1));
        }
    }
    by_stream
        .into_iter()
        .map(|(stream, n)| {
            let writer = slots
                .iter()
                .find(|s| s.reader == id && s.reader_stream.0 == stream)
                .and_then(|s| names.iter().find(|(wid, _)| *wid == s.writer))
                .map(|(_, name)| name.as_str())
                .unwrap_or("input");
            (writer.to_string(), n)
        })
        .collect()
}

/// Stages that keep a bag of records until EOF, then emit them 1-for-1.
/// `received - sent - discarded` is that bag. INSTORE / JSON COLLECT / LOOKUP
/// are not on this list: they transform, so the subtraction is not a count.
pub fn buffering_verb(verb: &str) -> Option<&'static str> {
    match verb.trim().to_ascii_uppercase().as_str() {
        "SORT" => Some("SORT"),
        "BUFFER" => Some("BUFFER"),
        _ => None,
    }
}

/// Honest held count, or `None` when the stage is not a known bufferer or
/// the arithmetic cannot be a bag size (`sent + discarded > received`).
pub fn holding_count(verb: &str, counts: FocusCounts) -> Option<usize> {
    let _ = buffering_verb(verb)?;
    let out = counts.sent.saturating_add(counts.discarded);
    if counts.received < out {
        return None;
    }
    let held = counts.received - out;
    if held == 0 {
        None
    } else {
        Some(held)
    }
}

pub fn holding_line(verb: &str, n: usize) -> String {
    let name = buffering_verb(verb).unwrap_or("This stage");
    let records = if n == 1 {
        "1 record".to_string()
    } else {
        format!("{n} records")
    };
    format!("Buffering {records} ({name} waits for all input before sending)")
}

fn holding_copy(
    focus: RecordFocus,
    labels: &FocusLabels,
    counts: FocusCounts,
) -> Option<HoldingCopy> {
    if !matches!(focus, RecordFocus::Stage(_)) {
        return None;
    }
    let verb = if buffering_verb(&labels.verb).is_some() {
        labels.verb.as_str()
    } else {
        labels.stage.as_str()
    };
    let count = holding_count(verb, counts)?;
    let line = holding_line(verb, count);
    Some(HoldingCopy {
        count,
        tooltip: line.clone(),
        line,
    })
}

pub fn edge_has_waiting(
    slots: &[SlotSnapshot],
    writer: StageId,
    writer_stream: u32,
    reader: StageId,
    reader_stream: u32,
) -> bool {
    waiting_items(
        slots,
        RecordFocus::Edge {
            writer,
            writer_stream,
            reader,
            reader_stream,
        },
    )
    .iter()
    .any(|w| w.kind == WaitingKind::Waiting)
}

pub fn records_phrase(n: usize, verb: &str) -> String {
    match n {
        1 => format!("1 record {verb}"),
        _ => format!("{n} records {verb}"),
    }
}

pub fn count_summary(c: FocusCounts, is_edge: bool) -> String {
    let discarded = match c.discarded {
        1 => "1 discarded".to_string(),
        n => format!("{n} discarded"),
    };
    if is_edge {
        format!("{} · {discarded}", records_phrase(c.sent, "sent"))
    } else {
        format!(
            "{} · {} · {discarded}",
            records_phrase(c.received, "received"),
            records_phrase(c.sent, "sent")
        )
    }
}

/// Status word on the graph peek. Short, and never “debugger”.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeekStatus {
    Finished,
    Waiting,
    Idle,
    Buffering,
}

impl PeekStatus {
    pub fn word(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::Waiting => "waiting",
            Self::Idle => "idle",
            Self::Buffering => "buffering",
        }
    }
}

pub fn peek_status(ended: bool, waiting_in: bool, holding: bool) -> PeekStatus {
    if ended {
        PeekStatus::Finished
    } else if waiting_in {
        PeekStatus::Waiting
    } else if holding {
        PeekStatus::Buffering
    } else {
        PeekStatus::Idle
    }
}

/// Short tooltip copy for a hovered stage or connection. No record grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverPeek {
    pub title: String,
    pub clause: String,
    pub status: Option<PeekStatus>,
    pub counts: String,
    pub hint: &'static str,
    /// Runtime note for this stage (missing `$field`, …).
    pub warning: Option<String>,
}

pub fn stage_hover_peek(
    name: &str,
    clause: &str,
    ended: bool,
    waiting_in: bool,
    holding: bool,
    received: usize,
    sent: usize,
    discarded: usize,
) -> HoverPeek {
    let mut counts = format!("Read {received}, wrote {sent}");
    if discarded > 0 {
        counts.push_str(&format!(", discarded {discarded}"));
    }
    HoverPeek {
        title: name.to_string(),
        clause: clause.to_string(),
        status: Some(peek_status(ended, waiting_in, holding)),
        counts,
        hint: CLICK_RECORDS_HINT,
        warning: None,
    }
}

pub fn edge_hover_peek(writer: &str, reader: &str, writer_stream: u32, sent: usize) -> HoverPeek {
    let mut title = format!("{writer} to {reader}");
    match writer_stream {
        0 => {}
        1 => title.push_str(" (secondary)"),
        n => title.push_str(&format!(" (stream {n})")),
    }
    let counts = match sent {
        0 => "No records crossed yet.".to_string(),
        1 => "1 record crossed.".to_string(),
        n => format!("{n} records crossed."),
    };
    HoverPeek {
        title,
        clause: String::new(),
        status: None,
        counts,
        hint: CLICK_RECORDS_HINT,
        warning: None,
    }
}

pub fn edge_sent_count(snap: &crate::worker::Snapshot, focus: RecordFocus) -> usize {
    let RecordFocus::Edge {
        writer,
        writer_stream,
        ..
    } = focus
    else {
        return 0;
    };
    if let Some(log) = snap.record_logs.iter().find(|(s, _)| *s == writer) {
        if let Some((_, n)) = log
            .1
            .sent_stream_totals
            .iter()
            .find(|(s, _)| *s == writer_stream)
        {
            return *n;
        }
    }
    counts_for_focus(&snap.events, focus).sent
}

pub fn output_stream_label(stream: u32) -> String {
    match stream {
        0 => "main output".into(),
        1 => "second output".into(),
        n => format!("output stream {n}"),
    }
}

pub fn input_stream_label(stream: u32) -> String {
    match stream {
        0 => "main input".into(),
        1 => "second input".into(),
        n => format!("input stream {n}"),
    }
}

fn writer_display(labels: &FocusLabels) -> &str {
    if labels.writer.is_empty() {
        "this stage"
    } else {
        &labels.writer
    }
}

fn stage_display(labels: &FocusLabels) -> &str {
    if !labels.stage.is_empty() {
        &labels.stage
    } else if !labels.writer.is_empty() {
        &labels.writer
    } else {
        "this stage"
    }
}

fn dest_name(focus: RecordFocus, item: Option<&WaitingItem>, labels: &FocusLabels) -> String {
    match focus {
        RecordFocus::Edge { .. } => {
            if labels.reader.is_empty() {
                "the next stage".into()
            } else {
                labels.reader.clone()
            }
        }
        RecordFocus::Stage(_) => item
            .map(|w| labels.name(w.reader).to_string())
            .unwrap_or_else(|| stage_display(labels).to_string()),
    }
}

fn waiting_heading(focus: RecordFocus, item: Option<&WaitingItem>, labels: &FocusLabels) -> String {
    format!("Waiting for {} to read", dest_name(focus, item, labels))
}

/// Output waiting is the writer's unread offer (Sent). Input waiting is
/// the reader's unread offer (Received). Connections are Sent-only.
fn waiting_on_output(focus: RecordFocus, item: &WaitingItem) -> bool {
    match focus {
        RecordFocus::Edge { .. } => true,
        RecordFocus::Stage(id) => item.writer == id,
    }
}

#[allow(dead_code)]
fn still_waiting(waiting: &[WaitingItem], obs: &ObservedItem) -> bool {
    if obs.kind != ObservedKind::WentOut {
        return false;
    }
    let Some(rec) = &obs.record else {
        return false;
    };
    waiting.iter().any(|w| {
        w.kind == WaitingKind::Waiting
            && w.writer_stream == obs.stream
            && w.record
                .as_ref()
                .is_some_and(|r| r.len == rec.len && r.bytes == rec.bytes)
    })
}

#[allow(dead_code)]
fn recent_caption(obs: &ObservedItem, focus: RecordFocus, labels: &FocusLabels) -> Option<String> {
    match (focus, obs.kind) {
        (RecordFocus::Edge { .. }, ObservedKind::WentOut) => {
            if labels.output_count > 1 {
                Some(format!("Sent on {}", output_stream_label(obs.stream)))
            } else {
                None
            }
        }
        (RecordFocus::Edge { .. }, ObservedKind::Discarded) => {
            if labels.output_count > 1 {
                Some(format!(
                    "Not sent (discarded) · {}",
                    output_stream_label(obs.stream)
                ))
            } else {
                Some("Not sent (discarded)".into())
            }
        }
        (RecordFocus::Stage(_), ObservedKind::CameIn) => {
            if labels.input_count > 1 {
                Some(format!("Received on {}", input_stream_label(obs.stream)))
            } else {
                Some("Received".into())
            }
        }
        (RecordFocus::Stage(_), ObservedKind::WentOut) => {
            if labels.output_count > 1 {
                Some(format!("Sent on {}", output_stream_label(obs.stream)))
            } else {
                Some("Sent".into())
            }
        }
        (RecordFocus::Stage(_), ObservedKind::Discarded) => Some("Not sent (discarded)".into()),
        (_, ObservedKind::InputEnded) => Some("No more input".into()),
        _ => None,
    }
}

fn waiting_section(
    item: &WaitingItem,
    focus: RecordFocus,
    labels: &FocusLabels,
) -> WaitingSectionCopy {
    let src = match focus {
        RecordFocus::Edge { .. } => writer_display(labels).to_string(),
        RecordFocus::Stage(_) => labels.name(item.writer).to_string(),
    };
    let heading = waiting_heading(focus, Some(item), labels);
    WaitingSectionCopy {
        tooltip: heading.clone(),
        heading,
        kind: item.kind,
        record: item.record.clone(),
        finished_note: match item.kind {
            WaitingKind::WriterFinished => Some(format!("{src} has finished.")),
            _ => None,
        },
        on_output: Some(waiting_on_output(focus, item)),
    }
}

#[allow(dead_code)]
fn recent_tooltip(obs: &ObservedItem, focus: RecordFocus, labels: &FocusLabels) -> String {
    match (focus, obs.kind) {
        (RecordFocus::Edge { .. }, ObservedKind::WentOut) => {
            format!(
                "Sent from {} to {}",
                writer_display(labels),
                dest_name(focus, None, labels)
            )
        }
        (RecordFocus::Edge { .. }, ObservedKind::Discarded) => {
            format!(
                "{} discarded this instead of sending it",
                writer_display(labels)
            )
        }
        (RecordFocus::Stage(_), ObservedKind::CameIn) => {
            format!("{} read this", stage_display(labels))
        }
        (RecordFocus::Stage(_), ObservedKind::WentOut) => {
            format!("{} sent this on", stage_display(labels))
        }
        (RecordFocus::Stage(_), ObservedKind::Discarded) => {
            format!(
                "{} discarded this instead of sending it",
                stage_display(labels)
            )
        }
        (_, ObservedKind::InputEnded) => "No more input on this connection".into(),
        _ => String::new(),
    }
}

fn summary_chips(
    counts: FocusCounts,
    _waiting: &[WaitingItem],
    focus: RecordFocus,
    labels: &FocusLabels,
) -> Vec<ChipCopy> {
    let is_edge = matches!(focus, RecordFocus::Edge { .. });
    let sent_tip = if is_edge {
        format!(
            "{} sent from {} to {}",
            records_phrase(counts.sent, "sent"),
            writer_display(labels),
            dest_name(focus, None, labels)
        )
    } else {
        format!(
            "{} from {}",
            records_phrase(counts.sent, "sent"),
            stage_display(labels)
        )
    };
    let mut chips = Vec::new();
    if !is_edge {
        chips.push(ChipCopy {
            kind: ChipKind::Received,
            text: format!("{} received", counts.received),
            tooltip: format!(
                "{} at {}",
                records_phrase(counts.received, "received"),
                stage_display(labels)
            ),
            count: counts.received,
        });
    }
    chips.push(ChipCopy {
        kind: ChipKind::Sent,
        text: format!("{} sent", counts.sent),
        tooltip: sent_tip,
        count: counts.sent,
    });
    if counts.discarded > 0 {
        chips.push(ChipCopy {
            kind: ChipKind::Discarded,
            text: format!("{} discarded", counts.discarded),
            tooltip: format!(
                "{} discarded at {}",
                counts.discarded,
                if is_edge {
                    writer_display(labels)
                } else {
                    stage_display(labels)
                }
            ),
            count: counts.discarded,
        });
    }
    chips
}

/// Plain-language copy for the Records pane.
pub fn preview_copy(
    counts: FocusCounts,
    waiting: &[WaitingItem],
    history: &RecordHistory,
    focus: RecordFocus,
    labels: &FocusLabels,
) -> PreviewCopy {
    let is_edge = matches!(focus, RecordFocus::Edge { .. });
    let waiting_sections = if waiting.is_empty() {
        Vec::new()
    } else if waiting.iter().all(|w| w.record.is_none()) {
        let finished = waiting
            .iter()
            .find(|w| w.kind == WaitingKind::WriterFinished);
        let dest = dest_name(focus, waiting.first(), labels);
        let kind = finished.map(|w| w.kind).unwrap_or(WaitingKind::Empty);
        let src_finished = finished.map(|w| {
            if is_edge {
                writer_display(labels).to_string()
            } else {
                labels.name(w.writer).to_string()
            }
        });
        let tooltip = if let Some(src) = &src_finished {
            format!("{src} has finished.")
        } else {
            format!("Idle, waiting for input ({dest})")
        };
        let sides: Vec<bool> = waiting
            .iter()
            .map(|w| waiting_on_output(focus, w))
            .collect();
        let on_output = if sides.iter().all(|b| *b) {
            Some(true)
        } else if sides.iter().all(|b| !*b) {
            Some(false)
        } else {
            None
        };
        vec![WaitingSectionCopy {
            tooltip,
            heading: String::new(),
            kind,
            record: None,
            // The inspector paints finished once; this field stays for tests.
            finished_note: src_finished.map(|src| format!("{src} has finished.")),
            on_output,
        }]
    } else {
        waiting
            .iter()
            .filter(|w| w.record.is_some())
            .map(|w| waiting_section(w, focus, labels))
            .collect()
    };
    let recv_total = history.received_total.max(counts.received);
    let sent_total = history.sent_total.max(counts.sent);
    let disc_total = history.discarded_total.max(counts.discarded);
    let received_heading = if is_edge {
        None
    } else {
        dir_heading("Received", history.received.len(), recv_total)
    };
    let sent_word = match focus {
        RecordFocus::Edge { writer_stream, .. } if labels.output_count > 1 => {
            format!("Sent on {}", output_stream_label(writer_stream))
        }
        _ => "Sent".into(),
    };
    let sent_heading = dir_heading(&sent_word, history.sent.len(), sent_total);
    let discarded_word = match focus {
        RecordFocus::Edge { writer_stream, .. } if labels.output_count > 1 => {
            format!(
                "Not sent (discarded) · {}",
                output_stream_label(writer_stream)
            )
        }
        _ => "Discarded".into(),
    };
    let discarded_heading = dir_heading(&discarded_word, history.discarded.len(), disc_total);
    let has_rows =
        !history.received.is_empty() || !history.sent.is_empty() || !history.discarded.is_empty();
    let recent_empty = if has_rows {
        None
    } else {
        Some(if is_edge && counts.sent == 0 && history.sent_total == 0 {
            "Nothing sent yet.".into()
        } else {
            "Nothing kept yet.".into()
        })
    };
    let sent_still_waiting = last_sent_still_waiting(waiting, &history.sent);
    PreviewCopy {
        summary: count_summary(counts, is_edge),
        chips: summary_chips(counts, waiting, focus, labels),
        holding: holding_copy(focus, labels, counts),
        waiting: waiting_sections,
        no_connection: waiting.is_empty(),
        recent_heading: String::new(),
        recent_empty,
        recent: Vec::new(),
        received_heading,
        sent_heading,
        discarded_heading,
        received_table: records_table(&history.received),
        sent_table: records_table(&history.sent),
        discarded_table: records_table(&history.discarded),
        received_first_seq: first_seq(history.received.len(), recv_total),
        sent_first_seq: first_seq(history.sent.len(), sent_total),
        discarded_first_seq: first_seq(history.discarded.len(), disc_total),
        no_more_input: history.no_more_input,
        sent_still_waiting,
    }
}

/// Visible Records-pane blocks in data-flow order: Received, then Sent.
/// Waiting/status lines are listed with the section they describe.
pub fn records_pane_blocks(copy: &PreviewCopy) -> Vec<&'static str> {
    let mut out = Vec::new();
    if copy.holding.is_some() {
        out.push("holding");
    }
    if copy.waiting.iter().any(|w| w.on_output.is_none()) {
        out.push("stage-waiting");
    }
    let recv_wait = copy.waiting.iter().any(|w| w.on_output == Some(false));
    if recv_wait {
        out.push("received-waiting");
    }
    if copy.no_more_input {
        out.push("no-more-input");
    }
    if copy.received_heading.is_some() {
        out.push("received");
    }
    let sent_wait = copy.waiting.iter().any(|w| w.on_output == Some(true));
    if sent_wait {
        out.push("sent-waiting");
    }
    if copy.sent_heading.is_some() {
        out.push("sent");
    }
    if copy.discarded_heading.is_some() {
        out.push("discarded");
    }
    out
}

fn last_sent_still_waiting(waiting: &[WaitingItem], sent: &[RecordPreview]) -> bool {
    let Some(last) = sent.last() else {
        return false;
    };
    waiting.iter().any(|w| {
        w.kind == WaitingKind::Waiting
            && w.record
                .as_ref()
                .is_some_and(|r| r.len == last.len && r.bytes == last.bytes)
    })
}

pub fn counts_for_stage(
    events: &[DebugEvent],
    id: StageId,
    tallies: &[(StageId, StageIoCounts)],
) -> FocusCounts {
    if let Some(t) = tallies.iter().find(|(s, _)| *s == id).map(|(_, c)| *c) {
        return FocusCounts {
            received: t.received,
            sent: t.sent,
            discarded: t.discarded,
        };
    }
    counts_for_focus(events, RecordFocus::Stage(id))
}

pub fn counts_for_focus(events: &[DebugEvent], focus: RecordFocus) -> FocusCounts {
    match focus {
        RecordFocus::Stage(id) => {
            let mut c = FocusCounts::default();
            for e in events.iter().filter(|e| e.stage == id) {
                match e.kind {
                    DebugEventKind::ReadTaken { .. } => c.received += 1,
                    DebugEventKind::WriteOffered { .. } => c.sent += 1,
                    DebugEventKind::WriteDiscard { .. } => c.discarded += 1,
                    _ => {}
                }
            }
            c
        }
        RecordFocus::Edge {
            writer,
            writer_stream,
            ..
        } => {
            let mut c = FocusCounts::default();
            for e in events.iter().filter(|e| e.stage == writer) {
                match &e.kind {
                    DebugEventKind::WriteOffered { stream, .. } if stream.0 == writer_stream => {
                        c.sent += 1;
                    }
                    DebugEventKind::WriteDiscard { stream, .. } if stream.0 == writer_stream => {
                        c.discarded += 1;
                    }
                    _ => {}
                }
            }
            c
        }
    }
}

pub fn waiting_items(slots: &[SlotSnapshot], focus: RecordFocus) -> Vec<WaitingItem> {
    match focus {
        RecordFocus::Stage(id) => slots
            .iter()
            .filter(|s| s.writer == id || s.reader == id)
            .map(waiting_from_slot)
            .collect(),
        RecordFocus::Edge {
            writer,
            writer_stream,
            reader,
            reader_stream,
        } => slots
            .iter()
            .filter(|s| {
                s.writer == writer
                    && s.reader == reader
                    && s.writer_stream.0 == writer_stream
                    && s.reader_stream.0 == reader_stream
            })
            .map(waiting_from_slot)
            .collect(),
    }
}

fn waiting_from_slot(slot: &SlotSnapshot) -> WaitingItem {
    let kind = if slot.record.is_some() {
        WaitingKind::Waiting
    } else if slot.severed || slot.writer_done {
        WaitingKind::WriterFinished
    } else {
        WaitingKind::Empty
    };
    WaitingItem {
        kind,
        writer: slot.writer,
        reader: slot.reader,
        writer_stream: slot.writer_stream.0,
        reader_stream: slot.reader_stream.0,
        record: slot.record.clone(),
    }
}

pub fn history_from_events(events: &[DebugEvent], focus: RecordFocus) -> RecordHistory {
    history_for(events, focus, &[])
}

pub fn history_for(
    events: &[DebugEvent],
    focus: RecordFocus,
    logs: &[(StageId, StageRecordLog)],
) -> RecordHistory {
    match focus {
        RecordFocus::Stage(id) => {
            if let Some(log) = logs.iter().find(|(s, _)| *s == id).map(|(_, l)| l) {
                return RecordHistory {
                    received: log.received.clone(),
                    sent: log.sent.iter().map(|r| r.record.clone()).collect(),
                    discarded: log.discarded.iter().map(|r| r.record.clone()).collect(),
                    received_total: log.received_total,
                    sent_total: log.sent_total,
                    discarded_total: log.discarded_total,
                    no_more_input: log.input_ended
                        || events.iter().any(|e| {
                            e.stage == id && matches!(e.kind, DebugEventKind::ReadEof { .. })
                        }),
                };
            }
            collect_history_from_events(events, focus)
        }
        RecordFocus::Edge {
            writer,
            writer_stream,
            ..
        } => {
            if let Some(log) = logs.iter().find(|(s, _)| *s == writer).map(|(_, l)| l) {
                let sent = log
                    .sent_by_stream
                    .iter()
                    .find(|(s, _)| *s == writer_stream)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                let discarded = log
                    .discarded_by_stream
                    .iter()
                    .find(|(s, _)| *s == writer_stream)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                let sent_total = log
                    .sent_stream_totals
                    .iter()
                    .find(|(s, _)| *s == writer_stream)
                    .map(|(_, n)| *n)
                    .unwrap_or(sent.len());
                let discarded_total = log
                    .discarded_stream_totals
                    .iter()
                    .find(|(s, _)| *s == writer_stream)
                    .map(|(_, n)| *n)
                    .unwrap_or(discarded.len());
                return RecordHistory {
                    received: Vec::new(),
                    sent,
                    discarded,
                    received_total: 0,
                    sent_total,
                    discarded_total,
                    no_more_input: false,
                };
            }
            collect_history_from_events(events, focus)
        }
    }
}

fn collect_history_from_events(events: &[DebugEvent], focus: RecordFocus) -> RecordHistory {
    let mut received = Vec::new();
    let mut sent = Vec::new();
    let mut discarded = Vec::new();
    let mut no_more_input = false;
    match focus {
        RecordFocus::Stage(id) => {
            for e in events.iter().filter(|e| e.stage == id) {
                match &e.kind {
                    DebugEventKind::ReadTaken { record, .. } => received.push(record.clone()),
                    DebugEventKind::WriteOffered { record, .. } => sent.push(record.clone()),
                    DebugEventKind::WriteDiscard { record, .. } => discarded.push(record.clone()),
                    DebugEventKind::ReadEof { .. } => no_more_input = true,
                    _ => {}
                }
            }
        }
        RecordFocus::Edge {
            writer,
            writer_stream,
            ..
        } => {
            for e in events.iter().filter(|e| e.stage == writer) {
                match &e.kind {
                    DebugEventKind::WriteOffered { stream, record }
                        if stream.0 == writer_stream =>
                    {
                        sent.push(record.clone());
                    }
                    DebugEventKind::WriteDiscard { stream, record }
                        if stream.0 == writer_stream =>
                    {
                        discarded.push(record.clone());
                    }
                    _ => {}
                }
            }
        }
    }
    let cap = RECORD_HISTORY_CAP;
    let received_total = received.len();
    let sent_total = sent.len();
    let discarded_total = discarded.len();
    RecordHistory {
        received: last_n(received, cap),
        sent: last_n(sent, cap),
        discarded: last_n(discarded, cap),
        received_total,
        sent_total,
        discarded_total,
        no_more_input,
    }
}

fn last_n<T>(mut v: Vec<T>, n: usize) -> Vec<T> {
    if v.len() > n {
        v.split_off(v.len() - n)
    } else {
        v
    }
}

/// One row per retained record. JSON objects share a union header; mixed or
/// non-object payloads stay in a single `value` column.
pub fn records_table(records: &[RecordPreview]) -> MiniTable {
    if records.is_empty() {
        return MiniTable {
            kind: MiniKind::Empty,
            headers: Vec::new(),
            rows: Vec::new(),
            truncated: false,
        };
    }
    let mut objects: Vec<Option<serde_json::Map<String, serde_json::Value>>> =
        Vec::with_capacity(records.len());
    let mut all_objects = true;
    let mut truncated = false;
    for rec in records {
        truncated |= rec.len > rec.bytes.len();
        let parsed = std::str::from_utf8(&rec.bytes)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
        match parsed {
            Some(serde_json::Value::Object(map)) => objects.push(Some(map)),
            _ => {
                all_objects = false;
                objects.push(None);
            }
        }
    }
    if all_objects {
        let mut headers = Vec::new();
        for map in objects.iter().flatten() {
            for k in map.keys() {
                if !headers.iter().any(|e| e == k) {
                    headers.push(k.clone());
                }
            }
        }
        if !headers.is_empty() && headers.len() <= TABLE_COLS {
            let rows = objects
                .iter()
                .map(|map| {
                    let map = map.as_ref().unwrap();
                    headers
                        .iter()
                        .map(|k| cell_from_json(map.get(k).unwrap_or(&serde_json::Value::Null)))
                        .collect()
                })
                .collect();
            return MiniTable {
                kind: MiniKind::Structured,
                headers,
                rows,
                truncated,
            };
        }
    }
    MiniTable {
        kind: MiniKind::Text,
        headers: Vec::new(),
        rows: records
            .iter()
            .map(|rec| {
                let cell = match std::str::from_utf8(&rec.bytes) {
                    Ok(s) => MiniCell::Text(s.to_string()),
                    Err(_) => MiniCell::Text(hex_preview(&rec.bytes)),
                };
                vec![cell]
            })
            .collect(),
        truncated,
    }
}

/// Recent observations in the scope of `focus`. Edge scope is writer *outgoing*
/// on that stream only — never the writer's input, never the reader's input.
pub fn recent_items(events: &[DebugEvent], focus: RecordFocus, n: usize) -> Vec<ObservedItem> {
    let mut out: Vec<ObservedItem> = match focus {
        RecordFocus::Stage(id) => events
            .iter()
            .filter(|e| e.stage == id)
            .filter_map(observed_from_event)
            .collect(),
        RecordFocus::Edge {
            writer,
            writer_stream,
            ..
        } => events
            .iter()
            .filter(|e| e.stage == writer)
            .filter_map(|e| match &e.kind {
                DebugEventKind::WriteOffered { record, stream } if stream.0 == writer_stream => {
                    Some(ObservedItem {
                        kind: ObservedKind::WentOut,
                        stream: stream.0,
                        record: Some(record.clone()),
                    })
                }
                DebugEventKind::WriteDiscard { record, stream } if stream.0 == writer_stream => {
                    Some(ObservedItem {
                        kind: ObservedKind::Discarded,
                        stream: stream.0,
                        record: Some(record.clone()),
                    })
                }
                _ => None,
            })
            .collect(),
    };
    if out.len() > n {
        out = out.split_off(out.len() - n);
    }
    out
}

fn observed_from_event(e: &DebugEvent) -> Option<ObservedItem> {
    match &e.kind {
        DebugEventKind::ReadTaken { record, stream } => Some(ObservedItem {
            kind: ObservedKind::CameIn,
            stream: stream.0,
            record: Some(record.clone()),
        }),
        DebugEventKind::WriteOffered { record, stream } => Some(ObservedItem {
            kind: ObservedKind::WentOut,
            stream: stream.0,
            record: Some(record.clone()),
        }),
        DebugEventKind::WriteDiscard { record, stream } => Some(ObservedItem {
            kind: ObservedKind::Discarded,
            stream: stream.0,
            record: Some(record.clone()),
        }),
        DebugEventKind::ReadEof { stream } => Some(ObservedItem {
            kind: ObservedKind::InputEnded,
            stream: stream.0,
            record: None,
        }),
        _ => None,
    }
}

pub fn mini_table(rec: &RecordPreview) -> MiniTable {
    let truncated = rec.len > rec.bytes.len();
    if rec.len == 0 && rec.bytes.is_empty() {
        return MiniTable {
            kind: MiniKind::Empty,
            headers: Vec::new(),
            rows: Vec::new(),
            truncated,
        };
    }
    match std::str::from_utf8(&rec.bytes) {
        Ok(s) => text_or_structured(s, truncated),
        Err(_) => MiniTable {
            kind: MiniKind::Binary,
            headers: Vec::new(),
            rows: vec![vec![MiniCell::Text(hex_preview(&rec.bytes))]],
            truncated,
        },
    }
}

fn text_or_structured(s: &str, truncated: bool) -> MiniTable {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        if let Some(table) = structured_table(&v, truncated) {
            return table;
        }
    }
    MiniTable {
        kind: MiniKind::Text,
        headers: Vec::new(),
        rows: vec![vec![MiniCell::Text(s.to_string())]],
        truncated,
    }
}

/// Mini-table for a JSON value when it is an object or array of objects.
pub fn json_mini_table(v: &serde_json::Value) -> Option<MiniTable> {
    structured_table(v, false)
}

fn structured_table(v: &serde_json::Value, truncated: bool) -> Option<MiniTable> {
    match v {
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                return Some(MiniTable {
                    kind: MiniKind::Structured,
                    headers: Vec::new(),
                    rows: Vec::new(),
                    truncated,
                });
            }
            let headers: Vec<String> = map.keys().cloned().collect();
            if headers.len() > TABLE_COLS {
                return None;
            }
            Some(MiniTable {
                kind: MiniKind::Structured,
                headers: headers.clone(),
                rows: vec![headers
                    .iter()
                    .map(|k| cell_from_json(map.get(k).unwrap_or(&serde_json::Value::Null)))
                    .collect()],
                truncated,
            })
        }
        serde_json::Value::Array(arr) if !arr.is_empty() && arr.iter().all(|x| x.is_object()) => {
            let mut headers = Vec::new();
            for item in arr.iter().take(TABLE_ROWS) {
                if let Some(map) = item.as_object() {
                    for k in map.keys() {
                        if !headers.iter().any(|e| e == k) {
                            headers.push(k.clone());
                        }
                    }
                }
            }
            if headers.is_empty() || headers.len() > TABLE_COLS {
                return None;
            }
            let rows = arr
                .iter()
                .take(TABLE_ROWS)
                .map(|item| {
                    let map = item.as_object().unwrap();
                    headers
                        .iter()
                        .map(|k| cell_from_json(map.get(k).unwrap_or(&serde_json::Value::Null)))
                        .collect()
                })
                .collect();
            Some(MiniTable {
                kind: MiniKind::Structured,
                headers,
                rows,
                truncated: truncated || arr.len() > TABLE_ROWS,
            })
        }
        _ => None,
    }
}

fn cell_from_json(v: &serde_json::Value) -> MiniCell {
    match v {
        serde_json::Value::Null => MiniCell::Empty,
        serde_json::Value::String(s) => MiniCell::Text(s.clone()),
        serde_json::Value::Number(n) => MiniCell::Text(n.to_string()),
        serde_json::Value::Bool(b) => MiniCell::Text(b.to_string()),
        nested @ (serde_json::Value::Object(_) | serde_json::Value::Array(_)) => MiniCell::Nested {
            summary: clip(&compact_json(nested)),
            json: nested.to_string(),
        },
    }
}

fn compact_json(v: &serde_json::Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| v.to_string())
}

fn clip(s: &str) -> String {
    let mut it = s.chars();
    let taken: String = it.by_ref().take(CELL_CHARS).collect();
    if it.next().is_some() {
        format!("{taken}…")
    } else {
        taken
    }
}

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(32)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn rec(s: &str) -> RecordPreview {
    RecordPreview {
        len: s.len(),
        bytes: s.as_bytes().to_vec(),
    }
}

pub fn stream(n: u32) -> StreamId {
    StreamId(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_core::DebugEvent;

    fn ev(stage: usize, kind: DebugEventKind) -> DebugEvent {
        DebugEvent {
            stage: StageId(stage),
            name: "t".into(),
            kind,
        }
    }

    fn slot(writer: usize, reader: usize, record: Option<&str>, writer_done: bool) -> SlotSnapshot {
        SlotSnapshot {
            writer: StageId(writer),
            writer_stream: StreamId(0),
            reader: StageId(reader),
            reader_stream: StreamId(0),
            record: record.map(rec),
            peeked: false,
            severed: false,
            writer_done,
        }
    }

    #[test]
    fn plain_apple_is_one_column_not_csv() {
        let t = mini_table(&rec("apple"));
        assert_eq!(t.kind, MiniKind::Text);
        assert!(
            t.headers.is_empty(),
            "no invented field names: {:?}",
            t.headers
        );
        assert_eq!(crate::sheet::sheet_headers(&t), vec!["value".to_string()]);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0], vec![MiniCell::Text("apple".into())]);
    }

    #[test]
    fn raw_csv_header_is_not_split_into_columns() {
        let line = "vessel_id,name,type,berth_id,owner_id";
        let t = mini_table(&rec(line));
        assert_eq!(t.kind, MiniKind::Text);
        assert!(t.headers.is_empty());
        assert_eq!(crate::sheet::sheet_headers(&t), vec!["value".to_string()]);
        assert_eq!(t.rows[0].len(), 1);
        match &t.rows[0][0] {
            MiniCell::Text(s) => {
                assert_eq!(s, line);
                assert!(s.contains("vessel_id,name"), "{s}");
            }
            other => panic!("expected one text cell, got {other:?}"),
        }
    }

    #[test]
    fn json_object_is_field_columns() {
        let t = mini_table(&rec(r#"{"vessel_id":"V1","name":"Kestrel","type":"sail"}"#));
        assert_eq!(t.kind, MiniKind::Structured);
        assert_eq!(t.headers, vec!["vessel_id", "name", "type"]);
        assert_eq!(t.rows.len(), 1);
        assert_eq!(
            t.rows[0],
            vec![
                MiniCell::Text("V1".into()),
                MiniCell::Text("Kestrel".into()),
                MiniCell::Text("sail".into()),
            ]
        );
    }

    #[test]
    fn json_object_array_is_multirow_grid() {
        let t = mini_table(&rec(r#"[{"a":1},{"a":2,"b":"x"}]"#));
        assert_eq!(t.kind, MiniKind::Structured);
        assert!(t.headers.contains(&"a".to_string()));
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn nested_object_cell_is_inspectable_not_flattened() {
        let t = mini_table(&rec(r#"{"id":"V1","owner":{"name":"Ann"}}"#));
        assert_eq!(t.kind, MiniKind::Structured);
        assert!(matches!(t.rows[0][1], MiniCell::Nested { .. }));
        match &t.rows[0][1] {
            MiniCell::Nested { json, .. } => assert!(json.contains("Ann"), "{json}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(crate::sheet::sheet_headers(&t)[0], "id");
    }

    #[test]
    fn long_plain_text_keeps_full_value() {
        let s = "abcdefghijklmnopqrstuvwxyz-0123456789-ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let t = mini_table(&rec(s));
        assert_eq!(t.kind, MiniKind::Text);
        match &t.rows[0][0] {
            MiniCell::Text(got) => assert_eq!(got, s),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn json_object_array_scrolls_past_visible_rows() {
        let rows: Vec<String> = (0..20)
            .map(|i| format!(r#"{{"n":{i},"name":"row-{i}"}}"#))
            .collect();
        let body = format!("[{}]", rows.join(","));
        let t = mini_table(&rec(&body));
        assert_eq!(t.kind, MiniKind::Structured);
        assert_eq!(t.rows.len(), 20);
        assert!(!t.truncated);
        assert!(crate::sheet::sheet_headers(&t).contains(&"name".to_string()));
    }

    #[test]
    fn edge_recent_is_writer_outgoing_only() {
        let events = vec![
            ev(
                1,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("vessel_id,name,type,berth_id,owner_id"),
                },
            ),
            ev(
                1,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec(r#"{"vessel_id":"V1"}"#),
                },
            ),
            ev(
                2,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec(r#"{"vessel_id":"V1"}"#),
                },
            ),
        ];
        let focus = RecordFocus::Edge {
            writer: StageId(1),
            writer_stream: 0,
            reader: StageId(2),
            reader_stream: 0,
        };
        let recent = recent_items(&events, focus, 8);
        assert_eq!(
            recent.len(),
            1,
            "must not include CSV input or LOOKUP input: {recent:?}"
        );
        assert_eq!(recent[0].kind, ObservedKind::WentOut);
        let bytes = recent[0].record.as_ref().unwrap();
        assert!(std::str::from_utf8(&bytes.bytes).unwrap().contains("V1"));
        let c = counts_for_focus(&events, focus);
        assert_eq!(c.received, 0);
        assert_eq!(c.sent, 1);
    }

    #[test]
    fn csv_pre_output_edge_claims_no_outgoing() {
        let events = vec![ev(
            1,
            DebugEventKind::ReadTaken {
                stream: stream(0),
                record: rec("vessel_id,name"),
            },
        )];
        let slots = vec![slot(1, 2, None, false)];
        let focus = RecordFocus::Edge {
            writer: StageId(1),
            writer_stream: 0,
            reader: StageId(2),
            reader_stream: 0,
        };
        assert!(recent_items(&events, focus, 8).is_empty());
        assert_eq!(counts_for_focus(&events, focus).sent, 0);
        let wait = waiting_items(&slots, focus);
        assert_eq!(wait.len(), 1);
        assert_eq!(wait[0].kind, WaitingKind::Empty);
        assert!(wait[0].record.is_none());
    }

    #[test]
    fn first_literal_edge_waiting_is_apple() {
        let events = vec![ev(
            0,
            DebugEventKind::WriteOffered {
                stream: stream(0),
                record: rec("apple"),
            },
        )];
        let slots = vec![slot(0, 1, Some("apple"), false)];
        let focus = RecordFocus::Edge {
            writer: StageId(0),
            writer_stream: 0,
            reader: StageId(1),
            reader_stream: 0,
        };
        let wait = waiting_items(&slots, focus);
        assert_eq!(wait[0].kind, WaitingKind::Waiting);
        let t = mini_table(wait[0].record.as_ref().unwrap());
        assert_eq!(t.rows[0][0], MiniCell::Text("apple".into()));
        let recent = recent_items(&events, focus, 8);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].kind, ObservedKind::WentOut);
        assert_eq!(counts_for_focus(&events, focus).sent, 1);
        assert_eq!(counts_for_focus(&events, focus).received, 0);
    }

    #[test]
    fn stage_focus_keeps_both_directions() {
        let events = vec![
            ev(
                3,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
            ev(
                3,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
        ];
        let focus = RecordFocus::Stage(StageId(3));
        let recent = recent_items(&events, focus, 8);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].kind, ObservedKind::CameIn);
        assert_eq!(recent[1].kind, ObservedKind::WentOut);
        let c = counts_for_focus(&events, focus);
        assert_eq!(c.received, 1);
        assert_eq!(c.sent, 1);
    }

    #[test]
    fn empty_and_binary_states() {
        let empty = mini_table(&RecordPreview {
            len: 0,
            bytes: vec![],
        });
        assert_eq!(empty.kind, MiniKind::Empty);
        let bin = mini_table(&RecordPreview {
            len: 2,
            bytes: vec![0xff, 0xfe],
        });
        assert_eq!(bin.kind, MiniKind::Binary);
        let trunc = mini_table(&RecordPreview {
            len: 100,
            bytes: b"short".to_vec(),
        });
        assert!(trunc.truncated);
    }

    fn blob(copy: &PreviewCopy) -> String {
        let mut s = copy.summary.clone();
        s.push('\n');
        if let Some(h) = &copy.holding {
            s.push_str(&h.line);
            s.push('\n');
        }
        let push_waiting = |s: &mut String, want: Option<bool>| {
            for w in &copy.waiting {
                if w.on_output != want {
                    continue;
                }
                s.push('\n');
                s.push_str(&w.heading);
            }
        };
        push_waiting(&mut s, None);
        push_waiting(&mut s, Some(false));
        if let Some(h) = &copy.received_heading {
            s.push_str(h);
            s.push('\n');
        }
        if copy.no_more_input {
            s.push_str("No more input\n");
        }
        push_waiting(&mut s, Some(true));
        if let Some(h) = &copy.sent_heading {
            s.push_str(h);
            s.push('\n');
        }
        if let Some(h) = &copy.discarded_heading {
            s.push_str(h);
            s.push('\n');
        }
        if let Some(e) = &copy.recent_empty {
            s.push('\n');
            s.push_str(e);
        }
        s
    }

    fn copy_of(
        events: &[DebugEvent],
        waiting: &[WaitingItem],
        focus: RecordFocus,
        labels: &FocusLabels,
    ) -> PreviewCopy {
        preview_copy(
            counts_for_focus(events, focus),
            waiting,
            &history_from_events(events, focus),
            focus,
            labels,
        )
    }

    fn assert_no_jargon(s: &str) {
        for bad in [
            "stream 0",
            "bounded",
            "ReadTaken",
            "WriteOffered",
            "Waiting now",
            "Recently observed",
            "went out",
            "came in",
            "on this connection",
        ] {
            assert!(
                !s.to_ascii_lowercase().contains(&bad.to_ascii_lowercase()),
                "jargon {bad:?} in {s:?}"
            );
        }
    }

    #[test]
    fn edge_copy_names_destination_and_tags_still_waiting() {
        let focus = RecordFocus::Edge {
            writer: StageId(1),
            writer_stream: 0,
            reader: StageId(2),
            reader_stream: 0,
        };
        let events = vec![ev(
            1,
            DebugEventKind::WriteOffered {
                stream: stream(0),
                record: rec("apple"),
            },
        )];
        let slots = vec![slot(1, 2, Some("apple"), false)];
        let wait = waiting_items(&slots, focus);
        let labels = FocusLabels::edge("LITERAL", "LITERAL");
        let copy = copy_of(&events, &wait, focus, &labels);
        assert_eq!(copy.summary, "1 record sent · 0 discarded");
        assert_eq!(copy.waiting[0].heading, "Waiting for LITERAL to read");
        assert_eq!(copy.waiting[0].on_output, Some(true));
        assert!(copy.waiting[0].record.is_some());
        assert_eq!(records_pane_blocks(&copy), ["sent-waiting", "sent"]);
        assert_eq!(copy.sent_heading.as_deref(), Some("Sent"));
        assert_eq!(copy.sent_table.rows.len(), 1);
        assert!(copy.sent_still_waiting);
        assert_no_jargon(&blob(&copy));
        assert_eq!(labels.title, "LITERAL -> LITERAL");
    }

    #[test]
    fn csv_pre_output_copy_is_nothing_sent() {
        let focus = RecordFocus::Edge {
            writer: StageId(1),
            writer_stream: 0,
            reader: StageId(2),
            reader_stream: 0,
        };
        let events = vec![ev(
            1,
            DebugEventKind::ReadTaken {
                stream: stream(0),
                record: rec("vessel_id,name"),
            },
        )];
        let slots = vec![slot(1, 2, None, false)];
        let wait = waiting_items(&slots, focus);
        let labels = FocusLabels::edge("CSV", "V: LOOKUP");
        let copy = copy_of(&events, &wait, focus, &labels);
        assert_eq!(copy.summary, "0 records sent · 0 discarded");
        assert!(copy.waiting[0].heading.is_empty());
        assert_eq!(copy.waiting[0].kind, WaitingKind::Empty);
        assert!(copy.waiting[0]
            .tooltip
            .contains("Idle, waiting for input (V: LOOKUP)"));
        assert_eq!(copy.recent_empty.as_deref(), Some("Nothing sent yet."));
        assert_eq!(labels.title, "CSV -> V: LOOKUP");
        assert_no_jargon(&blob(&copy));
    }

    #[test]
    fn multi_output_copy_names_human_streams() {
        let focus = RecordFocus::Edge {
            writer: StageId(0),
            writer_stream: 1,
            reader: StageId(2),
            reader_stream: 0,
        };
        let events = vec![ev(
            0,
            DebugEventKind::WriteOffered {
                stream: stream(1),
                record: rec("apple"),
            },
        )];
        let mut s = slot(0, 2, Some("apple"), false);
        s.writer_stream = StreamId(1);
        let wait = waiting_items(&[s], focus);
        let mut labels = FocusLabels::edge("LOCATE", "CONSOLE");
        labels.output_count = 2;
        let copy = copy_of(&events, &wait, focus, &labels);
        assert_eq!(copy.sent_heading.as_deref(), Some("Sent on second output"));
        assert_eq!(copy.sent_table.rows.len(), 1);
        assert!(copy.sent_still_waiting);
        assert_no_jargon(&blob(&copy));
        assert!(!blob(&copy).contains("output stream"));
    }

    #[test]
    fn stage_copy_uses_received_and_sent() {
        let focus = RecordFocus::Stage(StageId(3));
        let events = vec![
            ev(
                3,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
            ev(
                3,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
        ];
        let slots = vec![slot(3, 4, Some("apricot"), false)];
        let wait = waiting_items(&slots, focus);
        let labels = FocusLabels {
            title: "LOCATE".into(),
            stage: "LOCATE".into(),
            writer: "LOCATE".into(),
            reader: String::new(),
            verb: "LOCATE".into(),
            output_count: 1,
            input_count: 1,
            names: vec![
                (StageId(3), "LOCATE".into()),
                (StageId(4), "CONSOLE".into()),
            ],
        };
        let copy = copy_of(&events, &wait, focus, &labels);
        assert_eq!(
            copy.summary,
            "1 record received · 1 record sent · 0 discarded"
        );
        assert_eq!(copy.waiting[0].heading, "Waiting for CONSOLE to read");
        assert_eq!(copy.waiting[0].on_output, Some(true));
        assert_eq!(copy.received_heading.as_deref(), Some("Received"));
        assert_eq!(copy.sent_heading.as_deref(), Some("Sent"));
        assert_eq!(copy.received_table.rows.len(), 1);
        assert_eq!(copy.sent_table.rows.len(), 1);
        assert!(copy.sent_still_waiting);
        assert_eq!(copy.chips[0].kind, ChipKind::Received);
        assert_eq!(copy.chips[1].kind, ChipKind::Sent);
        assert_eq!(
            records_pane_blocks(&copy),
            ["received", "sent-waiting", "sent"]
        );
        assert_no_jargon(&blob(&copy));
    }

    #[test]
    fn records_pane_is_received_then_sent_with_waiting_attached() {
        let focus = RecordFocus::Stage(StageId(3));
        let events = vec![
            ev(
                3,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
            ev(
                3,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
        ];
        let slots = vec![
            slot(2, 3, Some("incoming"), false),
            slot(3, 4, Some("apricot"), false),
        ];
        let wait = waiting_items(&slots, focus);
        let labels = FocusLabels {
            title: "LOCATE".into(),
            stage: "LOCATE".into(),
            writer: "LOCATE".into(),
            reader: String::new(),
            verb: "LOCATE".into(),
            output_count: 1,
            input_count: 1,
            names: vec![
                (StageId(2), "LITERAL".into()),
                (StageId(3), "LOCATE".into()),
                (StageId(4), "CONSOLE".into()),
            ],
        };
        let copy = copy_of(&events, &wait, focus, &labels);
        assert_eq!(copy.waiting.len(), 2);
        let input = copy
            .waiting
            .iter()
            .find(|w| w.on_output == Some(false))
            .expect("input waiting");
        let output = copy
            .waiting
            .iter()
            .find(|w| w.on_output == Some(true))
            .expect("output waiting");
        assert_eq!(input.heading, "Waiting for LOCATE to read");
        assert_eq!(output.heading, "Waiting for CONSOLE to read");
        assert_eq!(
            records_pane_blocks(&copy),
            ["received-waiting", "received", "sent-waiting", "sent"]
        );
        let blob = blob(&copy);
        let recv_at = blob.find("Waiting for LOCATE to read").expect("input wait");
        let recv_h = blob.find("Received").expect("Received");
        let sent_at = blob
            .find("Waiting for CONSOLE to read")
            .expect("output wait");
        let sent_h = blob.find("Sent").expect("Sent");
        assert!(
            recv_at < recv_h && recv_h < sent_at && sent_at < sent_h,
            "data-flow order in {blob:?}"
        );
        assert_no_jargon(&blob);
    }

    #[test]
    fn lookup_input_waiting_is_one_line_even_when_received_is_zero() {
        let focus = RecordFocus::Stage(StageId(2));
        let events = vec![ev(
            1,
            DebugEventKind::WriteOffered {
                stream: stream(0),
                record: rec(r#"{"vessel_id":"V1","name":"Kestrel"}"#),
            },
        )];
        let slots = vec![slot(
            1,
            2,
            Some(r#"{"vessel_id":"V1","name":"Kestrel"}"#),
            false,
        )];
        let wait = waiting_items(&slots, focus);
        let labels = FocusLabels {
            title: "V: LOOKUP".into(),
            stage: "V: LOOKUP".into(),
            writer: "V: LOOKUP".into(),
            reader: String::new(),
            verb: "LOOKUP".into(),
            output_count: 1,
            input_count: 1,
            names: vec![(StageId(1), "CSV".into()), (StageId(2), "V: LOOKUP".into())],
        };
        let copy = copy_of(&events, &wait, focus, &labels);
        assert_eq!(counts_for_focus(&events, focus).received, 0);
        assert_eq!(copy.waiting.len(), 1, "one waiting line, not heading+role");
        assert_eq!(copy.waiting[0].heading, "Waiting for V: LOOKUP to read");
        assert_eq!(copy.waiting[0].on_output, Some(false));
        assert!(copy.waiting[0].record.is_some());
        assert_eq!(records_pane_blocks(&copy), ["received-waiting"]);
        assert_eq!(copy.recent_empty.as_deref(), Some("Nothing kept yet."));
        let blob = blob(&copy);
        assert_eq!(blob.matches("Waiting for").count(), 1);
        assert!(!blob.contains("input:"));
        assert_no_jargon(&blob);
        let badges = stage_badges(&events, &slots, StageId(2), "LOOKUP", false);
        assert!(badges.waiting_in);
        assert_eq!(badges.sent, 0);
        assert_eq!(badges.holding, None);
        assert!(edge_has_waiting(&slots, StageId(1), 0, StageId(2), 0));
        assert!(!edge_has_waiting(&slots, StageId(2), 0, StageId(3), 0));
    }

    #[test]
    fn overview_names_waiting_stage_and_counts() {
        let snap = crate::worker::Snapshot {
            status: pipe_core::SessionStatus::Paused,
            events: vec![ev(
                1,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("x"),
                },
            )],
            slots: vec![slot(1, 2, Some("x"), false)],
            live: Vec::new(),
            last_step: None,
            spec_fingerprint: String::new(),
            io_counts: Vec::new(),
            record_logs: Vec::new(),
        };
        let names = vec![
            (StageId(1), "CSV".into(), "CSV".into()),
            (StageId(2), "V: LOOKUP".into(), "LOOKUP".into()),
        ];
        let o = run_overview(Some(&snap), &names, 3);
        assert_eq!(o.status, "Paused");
        assert_eq!(o.steps, 3);
        assert_eq!(o.waiting[0].line, "Waiting for V: LOOKUP to read");
        assert!(o.waiting[0].tooltip.contains("CSV"));
        let lookup = o.stages.iter().find(|s| s.name == "V: LOOKUP").unwrap();
        assert!(
            lookup.waiting_in,
            "unread record is ⏳ waiting, not a receiver check"
        );
        assert!(!lookup.ended);
        assert_eq!(lookup.sent, 0);
        assert_eq!(lookup.holding, None);
        let csv = o.stages.iter().find(|s| s.name == "CSV").unwrap();
        assert_eq!(
            csv.sent, 1,
            "writer → sent plus waiting covers handled vs unread"
        );
        assert!(!csv.ended);
        assert!(!csv.waiting_in);
        let idle = run_overview(None, &names, 0);
        assert!(!idle.has_session);
    }

    #[test]
    fn ended_stage_is_done_not_idle() {
        use pipe_core::LiveStage;
        let live = |id: usize, name: &str, ended: bool| LiveStage {
            id: StageId(id),
            name: name.into(),
            ended,
            return_code: 0,
            waiting_debug: false,
            waiting_callpipe: false,
            waiting_helper: false,
            commit_group: 0,
        };
        let snap = crate::worker::Snapshot {
            status: pipe_core::SessionStatus::Ended { rc: 0 },
            events: vec![ev(
                0,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("apple"),
                },
            )],
            slots: vec![slot(0, 1, None, true)],
            live: vec![live(0, "LITERAL", true), live(1, "HOLE", true)],
            last_step: None,
            spec_fingerprint: String::new(),
            io_counts: Vec::new(),
            record_logs: Vec::new(),
        };
        let names = vec![
            (StageId(0), "LITERAL".into(), "LITERAL".into()),
            (StageId(1), "HOLE".into(), "HOLE".into()),
        ];
        let o = run_overview(Some(&snap), &names, 2);
        assert!(o.stages.iter().all(|s| s.ended), "{:?}", o.stages);
        assert!(!o.stages.iter().any(|s| s.waiting_in));
        let badges = stage_badges(&snap.events, &snap.slots, StageId(0), "LITERAL", true);
        assert!(badges.ended);
        assert!(!badges.waiting_in);
        assert_eq!(
            crate::symbols::stage_run_mark(badges.ended, badges.waiting_in),
            crate::symbols::RecordMark::Done
        );
        let live_hole = stage_badges(&snap.events, &snap.slots, StageId(1), "HOLE", false);
        assert_eq!(
            crate::symbols::stage_run_mark(live_hole.ended, live_hole.waiting_in),
            crate::symbols::RecordMark::Idle
        );
    }

    #[test]
    fn writer_finished_copy_is_done_not_idle() {
        let focus = RecordFocus::Edge {
            writer: StageId(0),
            writer_stream: 0,
            reader: StageId(1),
            reader_stream: 0,
        };
        let wait = waiting_items(&[slot(0, 1, None, true)], focus);
        assert_eq!(wait[0].kind, WaitingKind::WriterFinished);
        let labels = FocusLabels::edge("LITERAL", "HOLE");
        let copy = preview_copy(
            FocusCounts::default(),
            &wait,
            &RecordHistory::default(),
            focus,
            &labels,
        );
        assert_eq!(copy.waiting[0].kind, WaitingKind::WriterFinished);
        assert!(
            copy.waiting[0].tooltip.contains("finished"),
            "{}",
            copy.waiting[0].tooltip
        );
        assert!(
            !copy.waiting[0].tooltip.contains("Idle"),
            "{}",
            copy.waiting[0].tooltip
        );
        assert_eq!(
            crate::symbols::mark_for_waiting(copy.waiting[0].kind),
            crate::symbols::RecordMark::Done
        );
    }

    #[test]
    fn sort_holding_is_received_minus_sent() {
        let counts = FocusCounts {
            received: 5,
            sent: 1,
            discarded: 0,
        };
        assert_eq!(holding_count("SORT", counts), Some(4));
        assert_eq!(
            holding_line("SORT", 4),
            "Buffering 4 records (SORT waits for all input before sending)"
        );
        assert_eq!(holding_count("BUFFER", counts), Some(4));
        assert_eq!(holding_count("LOCATE", counts), None);
        assert_eq!(holding_count("LOOKUP", counts), None);
        assert_eq!(holding_count("INSTORE", counts), None);
        assert_eq!(
            holding_count(
                "SORT",
                FocusCounts {
                    received: 2,
                    sent: 5,
                    discarded: 0
                }
            ),
            None
        );
        assert_eq!(
            holding_count(
                "SORT",
                FocusCounts {
                    received: 3,
                    sent: 3,
                    discarded: 0
                }
            ),
            None
        );

        let focus = RecordFocus::Stage(StageId(2));
        let mut events = Vec::new();
        for i in 0..5 {
            events.push(ev(
                2,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec(&format!("w{i}")),
                },
            ));
        }
        events.push(ev(
            2,
            DebugEventKind::WriteOffered {
                stream: stream(0),
                record: rec("W1"),
            },
        ));
        let labels = FocusLabels {
            title: "SORT".into(),
            stage: "SORT".into(),
            writer: "SORT".into(),
            reader: String::new(),
            verb: "SORT".into(),
            output_count: 1,
            input_count: 1,
            names: vec![
                (StageId(2), "SORT".into()),
                (StageId(3), "C: COLLATE".into()),
            ],
        };
        let copy = preview_copy(
            counts_for_focus(&events, focus),
            &[],
            &history_from_events(&events, focus),
            focus,
            &labels,
        );
        let h = copy.holding.as_ref().expect("SORT holding");
        assert_eq!(h.count, 4);
        assert_eq!(
            h.line,
            "Buffering 4 records (SORT waits for all input before sending)"
        );
        assert!(blob(&copy).contains("Buffering 4 records"));
        assert_eq!(
            stage_badges(&events, &[], StageId(2), "SORT", false).holding,
            Some(4)
        );
        let names = vec![
            (StageId(1), "CSV".into(), "CSV".into()),
            (StageId(2), "SORT".into(), "SORT".into()),
        ];
        let snap = crate::worker::Snapshot {
            status: pipe_core::SessionStatus::Paused,
            events,
            slots: Vec::new(),
            live: Vec::new(),
            last_step: None,
            spec_fingerprint: String::new(),
            io_counts: Vec::new(),
            record_logs: Vec::new(),
        };
        let o = run_overview(Some(&snap), &names, 6);
        let sort = o.stages.iter().find(|s| s.name == "SORT").unwrap();
        assert_eq!(sort.holding, Some(4));
        assert_eq!(
            sort.holding_line.as_deref(),
            Some("Buffering 4 records (SORT waits for all input before sending)")
        );
        assert_eq!(
            o.stages.iter().find(|s| s.name == "CSV").unwrap().holding,
            None
        );

        let edge = RecordFocus::Edge {
            writer: StageId(1),
            writer_stream: 0,
            reader: StageId(2),
            reader_stream: 0,
        };
        let edge_copy = preview_copy(
            FocusCounts {
                received: 0,
                sent: 5,
                discarded: 0,
            },
            &[],
            &RecordHistory::default(),
            edge,
            &FocusLabels::edge("CSV", "SORT"),
        );
        assert!(edge_copy.holding.is_none());
    }

    #[test]
    fn locate_badges_are_three_in_two_out() {
        let events = vec![
            ev(
                3,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("apple"),
                },
            ),
            ev(
                3,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("apple"),
                },
            ),
            ev(
                3,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("banana"),
                },
            ),
            ev(
                3,
                DebugEventKind::WriteDiscard {
                    stream: stream(0),
                    record: rec("banana"),
                },
            ),
            ev(
                3,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
            ev(
                3,
                DebugEventKind::WriteOffered {
                    stream: stream(0),
                    record: rec("apricot"),
                },
            ),
        ];
        let b = stage_badges(&events, &[], StageId(3), "LOCATE", false);
        assert_eq!(b.received, 3);
        assert_eq!(b.sent, 2);
        assert_eq!(b.discarded, 1);
        assert_eq!(b.in_tip, "Read 3 records in");
        assert_eq!(b.out_tip, "Sent 2 records out");
        let hist = history_from_events(&events, RecordFocus::Stage(StageId(3)));
        let copy = preview_copy(
            counts_for_focus(&events, RecordFocus::Stage(StageId(3))),
            &[],
            &hist,
            RecordFocus::Stage(StageId(3)),
            &FocusLabels {
                title: "LOCATE".into(),
                stage: "LOCATE".into(),
                writer: "LOCATE".into(),
                reader: String::new(),
                verb: "LOCATE".into(),
                output_count: 1,
                input_count: 1,
                names: vec![(StageId(3), "LOCATE".into())],
            },
        );
        assert_eq!(copy.received_table.rows.len(), 3);
        assert_eq!(copy.sent_table.rows.len(), 2);
        assert_eq!(copy.discarded_table.rows.len(), 1);
        assert_eq!(in_badge_label(7), " 7");
        assert_eq!(out_badge_label(6), "6 ");
        let src = stage_badges(&events, &[], StageId(0), "LITERAL", false);
        assert_eq!(src.received, 0);
        assert!(
            src.in_tip.is_empty(),
            "source with no reads has no in badge"
        );
    }

    #[test]
    fn in_tooltip_names_each_input_stage() {
        let events = vec![
            ev(
                2,
                DebugEventKind::ReadTaken {
                    stream: stream(0),
                    record: rec("detail"),
                },
            ),
            ev(
                2,
                DebugEventKind::ReadTaken {
                    stream: stream(1),
                    record: rec("master"),
                },
            ),
            ev(
                2,
                DebugEventKind::ReadTaken {
                    stream: stream(1),
                    record: rec("master2"),
                },
            ),
        ];
        let slots = vec![
            SlotSnapshot {
                writer: StageId(0),
                writer_stream: StreamId(0),
                reader: StageId(2),
                reader_stream: StreamId(0),
                record: None,
                peeked: false,
                severed: false,
                writer_done: true,
            },
            SlotSnapshot {
                writer: StageId(1),
                writer_stream: StreamId(0),
                reader: StageId(2),
                reader_stream: StreamId(1),
                record: None,
                peeked: false,
                severed: false,
                writer_done: true,
            },
        ];
        let names = [
            (StageId(0), "CSV".into()),
            (StageId(1), "owners".into()),
            (StageId(2), "V: LOOKUP".into()),
        ];
        let tip = in_count_tooltip(3, &events, &slots, StageId(2), &names);
        assert!(tip.starts_with("Read 3 records in"), "{tip}");
        assert!(tip.contains("1 from CSV"), "{tip}");
        assert!(tip.contains("2 from owners"), "{tip}");
    }

    #[test]
    fn records_table_stacks_one_row_per_record() {
        let text = records_table(&[rec("a"), rec("b"), rec("c")]);
        assert_eq!(text.kind, MiniKind::Text);
        assert_eq!(text.rows.len(), 3);
        assert_eq!(text.rows[0][0], MiniCell::Text("a".into()));
        assert_eq!(text.rows[2][0], MiniCell::Text("c".into()));

        let objs = records_table(&[
            rec(r#"{"berth_id":"B1","dock":"North"}"#),
            rec(r#"{"berth_id":"B6","dock":"Guest","slip":"G1"}"#),
        ]);
        assert_eq!(objs.kind, MiniKind::Structured);
        assert_eq!(objs.rows.len(), 2);
        assert!(objs.headers.iter().any(|h| h == "berth_id"));
        assert_eq!(dir_heading("Received", 3, 3).as_deref(), Some("Received"));
        assert_eq!(
            dir_heading("Sent", 1000, 2500).as_deref(),
            Some("Sent (showing last 1000 of 2500)")
        );
        assert_eq!(first_seq(1000, 2500), 1501);
    }

    #[test]
    fn stage_peek_is_name_clause_status_and_counts() {
        let peek = stage_hover_peek(
            "<",
            "< examples/marina/vessels.jsonl",
            true,
            false,
            false,
            5,
            5,
            0,
        );
        assert_eq!(peek.title, "<");
        assert_eq!(peek.clause, "< examples/marina/vessels.jsonl");
        assert_eq!(peek.status, Some(PeekStatus::Finished));
        assert_eq!(peek.status.unwrap().word(), "finished");
        assert_eq!(peek.counts, "Read 5, wrote 5");
        assert_eq!(peek.hint, "Click to see records below.");
        assert!(!peek.counts.to_lowercase().contains("lane"));
        assert!(!peek.title.to_lowercase().contains("debugger"));
    }

    #[test]
    fn stage_peek_status_words_are_short() {
        assert_eq!(peek_status(false, false, false).word(), "idle");
        assert_eq!(peek_status(false, true, false).word(), "waiting");
        assert_eq!(peek_status(false, false, true).word(), "buffering");
        assert_eq!(peek_status(true, true, true).word(), "finished");
        let locate = stage_hover_peek("LOCATE", "locate /ap/", true, false, false, 3, 2, 1);
        assert_eq!(locate.counts, "Read 3, wrote 2, discarded 1");
    }

    #[test]
    fn connection_peek_names_the_stages_and_how_many_crossed() {
        let idle = edge_hover_peek("LITERAL", "LOCATE", 0, 0);
        assert_eq!(idle.title, "LITERAL to LOCATE");
        assert!(idle.clause.is_empty());
        assert_eq!(idle.status, None);
        assert_eq!(idle.counts, "No records crossed yet.");
        assert_eq!(idle.hint, CLICK_RECORDS_HINT);

        let one = edge_hover_peek("LOCATE", "HOLE", 1, 1);
        assert_eq!(one.title, "LOCATE to HOLE (secondary)");
        assert_eq!(one.counts, "1 record crossed.");

        let many = edge_hover_peek("FANOUT", "CSV", 2, 6);
        assert_eq!(many.title, "FANOUT to CSV (stream 2)");
        assert_eq!(many.counts, "6 records crossed.");
        assert!(!many.title.contains("lane"));
        assert!(!many.title.contains("->"));
    }

    #[test]
    fn show_records_mode_picks_peek_or_graph_grids() {
        use crate::settings::ShowRecords;
        assert!(!ShowRecords::Pane.grids_on_graph());
        assert!(ShowRecords::Graph.grids_on_graph());
        assert!(ShowRecords::Pane.pane_shows_grids());
        assert!(!ShowRecords::Graph.pane_shows_grids());
        let peek = stage_hover_peek("LOCATE", "locate /ap/", true, false, false, 3, 2, 1);
        assert_eq!(peek.hint, CLICK_RECORDS_HINT);
        assert_eq!(peek.counts, "Read 3, wrote 2, discarded 1");
        assert!(!peek.counts.to_lowercase().contains("received"));
        assert!(!peek.counts.to_lowercase().contains("lane"));
        // Graph mode draws the same compact headings the Records pane uses.
        assert_eq!(dir_heading("Received", 3, 3).as_deref(), Some("Received"));
        assert_eq!(dir_heading("Sent", 2, 2).as_deref(), Some("Sent"));
        assert_eq!(dir_heading("Discarded", 1, 1).as_deref(), Some("Discarded"));
    }
}
