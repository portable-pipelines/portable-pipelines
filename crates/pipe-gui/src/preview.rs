//! Bounded record previews. No execution. Structure comes from the bytes
//! themselves (JSON objects) or is treated as a single text cell — never guessed CSV.

use pipe_core::{DebugEvent, DebugEventKind, RecordPreview, SlotSnapshot, StageId, StreamId};

const TABLE_ROWS: usize = 16;
const TABLE_COLS: usize = 12;
const CELL_CHARS: usize = 48;
const HOVER_RECENT: usize = 3;
const PANE_RECENT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub fn hover_recent_limit() -> usize {
    HOVER_RECENT
}

pub fn pane_recent_limit() -> usize {
    PANE_RECENT
}

/// Display names for hover and the Records pane. Copy only; does not change scope.
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
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageBadges {
    pub sent: usize,
    pub discarded: usize,
    pub waiting_in: bool,
    /// Known internal bag size for SORT/BUFFER. `None` if not a bufferer or unknown.
    pub holding: Option<usize>,
    /// True when the live stage has ended and will do no more work.
    pub ended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageOverviewRow {
    pub name: String,
    pub sent: usize,
    pub discarded: usize,
    pub passed: usize,
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
        let c = counts_for_focus(&snap.events, RecordFocus::Stage(*id));
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
            passed: c.received,
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
    let c = counts_for_focus(events, RecordFocus::Stage(id));
    StageBadges {
        sent: c.sent,
        discarded: c.discarded,
        waiting_in: slots.iter().any(|s| s.reader == id && s.record.is_some()),
        holding: holding_count(verb, c),
        ended,
    }
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
    format!("Holding {records} ({name} waits for all input before sending)")
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
    }
}

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
    let mut chips = vec![ChipCopy {
        kind: ChipKind::Sent,
        text: format!("{} sent", counts.sent),
        tooltip: sent_tip,
        count: counts.sent,
    }];
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

/// Plain-language copy for hover and the Records pane. Same wording in both places.
pub fn preview_copy(
    counts: FocusCounts,
    waiting: &[WaitingItem],
    recent: &[ObservedItem],
    focus: RecordFocus,
    labels: &FocusLabels,
    recent_limit: usize,
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
        vec![WaitingSectionCopy {
            tooltip,
            heading: String::new(),
            kind,
            record: None,
            finished_note: src_finished.map(|src| format!("{src} has finished.")),
        }]
    } else {
        waiting
            .iter()
            .filter(|w| w.record.is_some())
            .map(|w| waiting_section(w, focus, labels))
            .collect()
    };
    let recent_heading = if is_edge {
        format!(
            "Recently sent from {} (last {recent_limit} kept)",
            writer_display(labels)
        )
    } else {
        format!(
            "Recently seen at {} (last {recent_limit} kept)",
            stage_display(labels)
        )
    };
    let recent_empty = if recent.is_empty() {
        Some(if is_edge && counts.sent == 0 {
            "Nothing sent yet.".into()
        } else {
            "Nothing kept yet.".into()
        })
    } else {
        None
    };
    PreviewCopy {
        summary: count_summary(counts, is_edge),
        chips: summary_chips(counts, waiting, focus, labels),
        holding: holding_copy(focus, labels, counts),
        waiting: waiting_sections,
        no_connection: waiting.is_empty(),
        recent_heading,
        recent_empty,
        recent: recent
            .iter()
            .map(|o| RecentRowCopy {
                caption: recent_caption(o, focus, labels),
                tooltip: recent_tooltip(o, focus, labels),
                still_waiting: still_waiting(waiting, o),
                kind: o.kind,
                record: o.record.clone(),
                stream: o.stream,
            })
            .collect(),
    }
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
        rows: vec![vec![MiniCell::Text(clip(s))]],
        truncated,
    }
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
        serde_json::Value::String(s) => MiniCell::Text(clip(s)),
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
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0], vec![MiniCell::Text("apple".into())]);
    }

    #[test]
    fn raw_csv_header_is_not_split_into_columns() {
        let line = "vessel_id,name,type,berth_id,owner_id";
        let t = mini_table(&rec(line));
        assert_eq!(t.kind, MiniKind::Text);
        assert!(t.headers.is_empty());
        assert_eq!(t.rows[0].len(), 1);
        match &t.rows[0][0] {
            MiniCell::Text(s) => assert!(s.contains("vessel_id,name"), "{s}"),
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
        s.push_str(&copy.recent_heading);
        for w in &copy.waiting {
            s.push('\n');
            s.push_str(&w.heading);
            if let Some(n) = &w.finished_note {
                s.push('\n');
                s.push_str(n);
            }
        }
        if let Some(e) = &copy.recent_empty {
            s.push('\n');
            s.push_str(e);
        }
        for r in &copy.recent {
            if let Some(c) = &r.caption {
                s.push('\n');
                s.push_str(c);
            }
        }
        s
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
        let recent = recent_items(&events, focus, 3);
        let labels = FocusLabels::edge("LITERAL", "LITERAL");
        let copy = preview_copy(
            counts_for_focus(&events, focus),
            &wait,
            &recent,
            focus,
            &labels,
            3,
        );
        assert_eq!(copy.summary, "1 record sent · 0 discarded");
        assert_eq!(copy.waiting[0].heading, "Waiting for LITERAL to read");
        assert!(copy.waiting[0].record.is_some());
        assert_eq!(
            copy.recent_heading,
            "Recently sent from LITERAL (last 3 kept)"
        );
        assert_eq!(copy.recent[0].caption, None);
        assert!(copy.recent[0].still_waiting);
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
        let recent = recent_items(&events, focus, 3);
        let labels = FocusLabels::edge("CSV", "V: LOOKUP");
        let copy = preview_copy(
            counts_for_focus(&events, focus),
            &wait,
            &recent,
            focus,
            &labels,
            3,
        );
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
        let recent = recent_items(&events, focus, 3);
        let mut labels = FocusLabels::edge("LOCATE", "CONSOLE");
        labels.output_count = 2;
        let copy = preview_copy(
            counts_for_focus(&events, focus),
            &wait,
            &recent,
            focus,
            &labels,
            3,
        );
        assert_eq!(
            copy.recent[0].caption.as_deref(),
            Some("Sent on second output")
        );
        assert!(copy.recent[0].still_waiting);
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
        let recent = recent_items(&events, focus, 8);
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
        let copy = preview_copy(
            counts_for_focus(&events, focus),
            &wait,
            &recent,
            focus,
            &labels,
            8,
        );
        assert_eq!(
            copy.summary,
            "1 record received · 1 record sent · 0 discarded"
        );
        assert_eq!(copy.waiting[0].heading, "Waiting for CONSOLE to read");
        assert_eq!(copy.recent_heading, "Recently seen at LOCATE (last 8 kept)");
        assert_eq!(copy.recent[0].caption.as_deref(), Some("Received"));
        assert_eq!(copy.recent[1].caption.as_deref(), Some("Sent"));
        assert!(copy.recent[1].still_waiting);
        assert!(!copy.recent[0].still_waiting);
        assert_no_jargon(&blob(&copy));
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
        let recent = recent_items(&events, focus, 3);
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
        let copy = preview_copy(
            counts_for_focus(&events, focus),
            &wait,
            &recent,
            focus,
            &labels,
            3,
        );
        assert_eq!(counts_for_focus(&events, focus).received, 0);
        assert_eq!(copy.waiting.len(), 1, "one waiting line, not heading+role");
        assert_eq!(copy.waiting[0].heading, "Waiting for V: LOOKUP to read");
        assert!(copy.waiting[0].record.is_some());
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
        assert!(lookup.waiting_in);
        assert!(!lookup.ended);
        assert_eq!(lookup.sent, 0);
        assert_eq!(lookup.holding, None);
        let csv = o.stages.iter().find(|s| s.name == "CSV").unwrap();
        assert_eq!(csv.sent, 1);
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
        let copy = preview_copy(FocusCounts::default(), &wait, &[], focus, &labels, 3);
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
            "Holding 4 records (SORT waits for all input before sending)"
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
            &[],
            focus,
            &labels,
            3,
        );
        let h = copy.holding.as_ref().expect("SORT holding");
        assert_eq!(h.count, 4);
        assert_eq!(
            h.line,
            "Holding 4 records (SORT waits for all input before sending)"
        );
        assert!(blob(&copy).contains("Holding 4 records"));
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
        };
        let o = run_overview(Some(&snap), &names, 6);
        let sort = o.stages.iter().find(|s| s.name == "SORT").unwrap();
        assert_eq!(sort.holding, Some(4));
        assert_eq!(
            sort.holding_line.as_deref(),
            Some("Holding 4 records (SORT waits for all input before sending)")
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
            &[],
            edge,
            &FocusLabels::edge("CSV", "SORT"),
            3,
        );
        assert!(edge_copy.holding.is_none());
    }
}
