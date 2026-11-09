//! Plain-language inspector. Counts come from recorded debug events.

use eframe::egui::{self, RichText, Ui};
use pipe_core::{DebugEvent, DebugEventKind, LiveStage, RecordPreview, SlotSnapshot, StageId};

use crate::preview::{
    counts_for_focus, mini_table, pane_recent_limit, recent_items, waiting_items, MiniCell,
    MiniKind, MiniTable, ObservedItem, ObservedKind, RecordFocus, WaitingItem, WaitingKind,
};
use crate::theme;
use crate::worker::Snapshot;

const HISTORY: usize = 24;
const TABLE_ROWS: usize = 16;

#[derive(Default, Clone, Copy)]
pub struct StageCounts {
    pub received: usize,
    pub sent: usize,
    pub discarded: usize,
}

pub fn counts_for(events: &[DebugEvent], id: StageId) -> StageCounts {
    let mut c = StageCounts::default();
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

pub fn live_status_line(snap: &Snapshot) -> String {
    match &snap.status {
        pipe_core::SessionStatus::Idle => "Idle — nothing has run yet".into(),
        pipe_core::SessionStatus::Running => "Running".into(),
        pipe_core::SessionStatus::Paused => "Paused".into(),
        pipe_core::SessionStatus::Ended { rc } => format!("Ended  rc {rc}"),
        pipe_core::SessionStatus::Stopped { rc } => format!("Stopped  rc {rc}"),
        pipe_core::SessionStatus::Failed(m) => format!("Failed: {m}"),
    }
}

/// Compact record-centric pane. Details stay behind a disclosure.
pub fn show_compact(
    ui: &mut Ui,
    snap: Option<&Snapshot>,
    focus: Option<RecordFocus>,
    name: &str,
    script: Option<ScriptIdentity>,
    details_open: &mut bool,
) {
    ui.label(RichText::new(name).strong().size(15.0));
    if let Some(s) = script {
        show_script_identity(ui, &s);
    }
    let Some(snap) = snap else {
        ui.label(RichText::new("No session yet. Step or Run to see records.").color(theme::MUTED));
        return;
    };
    let Some(focus) = focus else {
        ui.label(RichText::new("Select a stage or connection.").color(theme::MUTED));
        return;
    };
    show_focus_records(ui, snap, focus, false);
    ui.add_space(8.0);
    ui.checkbox(details_open, "More details");
    if *details_open {
        if let RecordFocus::Stage(id) = focus {
            let live = snap.live.iter().find(|s| s.id == id);
            show_stage(ui, snap, id, live, snap.last_step.as_deref());
        } else {
            ui.label(
                RichText::new("This connection only shows records that went out on it.")
                    .small()
                    .color(theme::MUTED),
            );
        }
    }
}

/// Tiny hover card. Does not read files or run stages.
pub fn show_hover_card(
    ui: &mut Ui,
    snap: Option<&Snapshot>,
    focus: Option<RecordFocus>,
    title: &str,
    detail: &str,
) {
    ui.set_max_width(440.0);
    ui.label(RichText::new(title).strong());
    if !detail.is_empty() {
        ui.label(RichText::new(detail).small().color(theme::MUTED));
    }
    let Some(snap) = snap else {
        ui.label(
            RichText::new("Idle — no records yet.")
                .small()
                .color(theme::MUTED),
        );
        return;
    };
    let Some(focus) = focus else {
        return;
    };
    show_focus_records(ui, snap, focus, true);
}

fn show_focus_records(ui: &mut Ui, snap: &Snapshot, focus: RecordFocus, compact: bool) {
    let c = counts_for_focus(&snap.events, focus);
    match focus {
        RecordFocus::Stage(_) => {
            ui.label(
                RichText::new(format!(
                    "in {}  out {}  discarded {}",
                    c.received, c.sent, c.discarded
                ))
                .small()
                .color(theme::MUTED),
            );
        }
        RecordFocus::Edge { .. } => {
            ui.label(
                RichText::new(format!("out {}  discarded {}", c.sent, c.discarded))
                    .small()
                    .color(theme::MUTED),
            );
        }
    }

    ui.add_space(4.0);
    ui.label(RichText::new("Waiting now").strong());
    let waiting = waiting_items(&snap.slots, focus);
    if waiting.is_empty() {
        ui.label(RichText::new("No connection in this session.").color(theme::MUTED));
    } else {
        let mut any_record = false;
        for item in &waiting {
            show_waiting(ui, item, compact);
            if item.record.is_some() {
                any_record = true;
            }
        }
        if !any_record && waiting.iter().all(|w| w.kind == WaitingKind::Empty) {
            ui.label(RichText::new("Nothing waiting.").color(theme::MUTED));
        }
    }

    ui.add_space(6.0);
    let limit = if compact {
        crate::preview::hover_recent_limit()
    } else {
        pane_recent_limit()
    };
    ui.label(RichText::new("Recently observed").strong());
    ui.label(
        RichText::new(if matches!(focus, RecordFocus::Edge { .. }) {
            format!("Last {limit} outgoing records on this connection. Bounded.")
        } else {
            format!("Last {limit} observations for this stage. Bounded.")
        })
        .small()
        .color(theme::MUTED),
    );
    let hist = recent_items(&snap.events, focus, limit);
    if hist.is_empty() {
        if matches!(focus, RecordFocus::Edge { .. }) && c.sent == 0 {
            ui.label(RichText::new("No outgoing record yet.").color(theme::MUTED));
        } else {
            ui.label(RichText::new("None yet.").color(theme::MUTED));
        }
    }
    for (i, item) in hist.iter().enumerate() {
        show_observed(ui, item, compact, i);
    }
}

fn show_waiting(ui: &mut Ui, item: &WaitingItem, compact: bool) {
    match item.kind {
        WaitingKind::Waiting => {
            ui.label(format!("{}: waiting", item.role));
            if let Some(rec) = &item.record {
                show_mini(ui, rec, compact, "waiting");
            }
        }
        WaitingKind::WriterFinished => {
            ui.label(format!("{}: empty — writer finished", item.role));
        }
        WaitingKind::Empty => {}
    }
}

fn show_observed(ui: &mut Ui, item: &ObservedItem, compact: bool, index: usize) {
    let label = match item.kind {
        ObservedKind::CameIn => format!("came in on stream {}", item.stream),
        ObservedKind::WentOut => format!("went out on stream {}", item.stream),
        ObservedKind::Discarded => format!("discarded on stream {}", item.stream),
        ObservedKind::InputEnded => format!("input ended on stream {}", item.stream),
    };
    ui.label(RichText::new(label).small());
    if let Some(rec) = &item.record {
        let kind = match item.kind {
            ObservedKind::CameIn => "in",
            ObservedKind::WentOut => "out",
            ObservedKind::Discarded => "disc",
            ObservedKind::InputEnded => "eof",
        };
        show_mini(ui, rec, compact, ("obs", kind, item.stream, index));
    }
}

fn show_mini(ui: &mut Ui, rec: &RecordPreview, compact: bool, salt: impl std::hash::Hash) {
    let table = mini_table(rec);
    match table.kind {
        MiniKind::Empty => {
            ui.label(RichText::new("empty record").small().color(theme::MUTED));
        }
        MiniKind::Binary => {
            ui.label(RichText::new("binary").small().color(theme::MUTED));
            show_mini_table(ui, &table, !compact, salt);
        }
        MiniKind::Text | MiniKind::Structured => {
            show_mini_table(ui, &table, !compact, salt);
        }
    }
    if table.truncated {
        ui.label(
            RichText::new(format!("truncated for display ({} bytes live)", rec.len))
                .small()
                .color(theme::MUTED),
        );
    }
}

fn show_mini_table(
    ui: &mut Ui,
    table: &MiniTable,
    inspect_nested: bool,
    salt: impl std::hash::Hash,
) {
    if table.rows.is_empty() {
        return;
    }
    let id = ui.id().with("mini-table").with(salt);
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .inner_margin(egui::Margin::same(6))
        .corner_radius(4)
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt(id)
                .max_height(if inspect_nested { 220.0 } else { 120.0 })
                .auto_shrink([true, true])
                .show(ui, |ui| {
                    egui::Grid::new(id.with("grid"))
                        .striped(true)
                        .min_col_width(48.0)
                        .spacing([10.0_f32, 4.0_f32])
                        .show(ui, |ui| {
                            if !table.headers.is_empty() {
                                for h in &table.headers {
                                    ui.label(RichText::new(h).strong().small());
                                }
                                ui.end_row();
                            }
                            for row in &table.rows {
                                for cell in row {
                                    show_cell(ui, cell, inspect_nested);
                                }
                                ui.end_row();
                            }
                        });
                });
        });
}

fn show_cell(ui: &mut Ui, cell: &MiniCell, inspect_nested: bool) {
    match cell {
        MiniCell::Empty => {
            ui.label(RichText::new("—").small().color(theme::MUTED));
        }
        MiniCell::Text(s) => {
            ui.label(RichText::new(s).small().monospace());
        }
        MiniCell::Nested { summary, json } => {
            if inspect_nested {
                ui.collapsing(summary.clone(), |ui| {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                        show_json(ui, &v, 0);
                    } else {
                        ui.monospace(json);
                    }
                });
            } else {
                ui.label(
                    RichText::new(summary)
                        .small()
                        .monospace()
                        .color(theme::MUTED),
                );
            }
        }
    }
}

#[derive(Clone)]
pub struct ScriptIdentity {
    pub verb: String,
    pub running: String,
    pub cwd: std::path::PathBuf,
}

pub fn show_script_identity(ui: &mut Ui, s: &ScriptIdentity) {
    let lang = crate::graph::script_language(&s.verb).unwrap_or("script");
    let file = crate::graph::script_file_name(&s.verb);
    let args = crate::graph::script_args(&s.running, &s.verb);
    let path = crate::model::resolve_script_path(&s.cwd, &s.verb);
    ui.label(format!("{lang}  {file}"));
    ui.label(
        RichText::new(format!("as written: {}", s.verb))
            .small()
            .monospace()
            .color(theme::MUTED),
    );
    ui.label(
        RichText::new(format!(
            "resolved from working directory:\n{}",
            path.display()
        ))
        .small()
        .monospace()
        .color(theme::MUTED),
    );
    if !args.is_empty() {
        ui.label(format!("arguments: {args}"));
    }
    ui.label(
        RichText::new("File is not opened until you choose View script file. Interpreter locals are not available.")
            .small()
            .color(theme::MUTED),
    );
}

pub fn show_stage(
    ui: &mut Ui,
    snap: &Snapshot,
    id: StageId,
    live: Option<&LiveStage>,
    last_step: Option<&str>,
) {
    if let Some(live) = live {
        ui.label(RichText::new(&live.name).strong().color(theme::ACCENT));
        let state = if live.ended {
            format!("ended rc {}", live.return_code)
        } else if live.waiting_callpipe {
            "waiting in CALLPIPE".into()
        } else if live.waiting_helper {
            "waiting on a helper".into()
        } else if matches!(snap.status, pipe_core::SessionStatus::Running) {
            "running".into()
        } else {
            "paused or idle".into()
        };
        ui.label(state);
    }
    let c = counts_for(&snap.events, id);
    ui.label(format!(
        "This session: {} received, {} sent, {} discarded",
        c.received, c.sent, c.discarded
    ));
    ui.label(
        RichText::new("Counts are dispatcher observations, not a 1-in-1-out proof.")
            .small()
            .color(theme::MUTED),
    );
    if let Some(note) = last_step {
        ui.add_space(6.0);
        ui.label(RichText::new("Last step").strong());
        ui.label(note);
    }

    ui.add_space(8.0);
    ui.label(RichText::new("On the connection now").strong());
    ui.label(
        RichText::new("The record sitting in a pipe (if any). Not the same as history.")
            .small()
            .color(theme::MUTED),
    );
    let mut any_slot = false;
    for slot in snap
        .slots
        .iter()
        .filter(|s| s.writer == id || s.reader == id)
    {
        any_slot = true;
        show_slot(ui, slot, id);
    }
    if !any_slot {
        ui.label(RichText::new("No live connections.").color(theme::MUTED));
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Recent history").strong());
    ui.label(
        RichText::new("Bounded log of reads, writes, and discards for this stage.")
            .small()
            .color(theme::MUTED),
    );
    let hist: Vec<&DebugEvent> = snap
        .events
        .iter()
        .filter(|e| e.stage == id && event_has_record(&e.kind))
        .rev()
        .take(HISTORY)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if hist.is_empty() {
        ui.label(RichText::new("No records observed yet.").color(theme::MUTED));
    }
    for ev in hist {
        show_event(ui, ev);
    }
}

fn event_has_record(kind: &DebugEventKind) -> bool {
    matches!(
        kind,
        DebugEventKind::ReadTaken { .. }
            | DebugEventKind::WriteOffered { .. }
            | DebugEventKind::WriteDiscard { .. }
            | DebugEventKind::ReadEof { .. }
    )
}

fn show_slot(ui: &mut Ui, slot: &SlotSnapshot, id: StageId) {
    ui.add_space(4.0);
    let role = if slot.writer == id {
        if slot.writer_stream.0 == 0 {
            "output (primary)"
        } else if slot.writer_stream.0 == 1 {
            "output (secondary)"
        } else {
            "output"
        }
    } else if slot.reader_stream.0 == 0 {
        "input (primary)"
    } else if slot.reader_stream.0 == 1 {
        "input (secondary)"
    } else {
        "input"
    };
    let wait = if slot.record.is_some() {
        if slot.peeked {
            "a record is waiting (peeked, not consumed)"
        } else {
            "a record is waiting"
        }
    } else if slot.severed || slot.writer_done {
        "empty — writer finished"
    } else {
        "empty"
    };
    ui.label(format!("{role}: {wait}"));
    if let Some(rec) = &slot.record {
        show_mini(ui, rec, false, ("slot", slot.writer.0, slot.reader.0));
    }
}

fn show_event(ui: &mut Ui, ev: &DebugEvent) {
    match &ev.kind {
        DebugEventKind::ReadTaken { record, stream } => {
            ui.label(format!("came in on stream {}", stream.0));
            show_mini(ui, record, false, ("detail-in", stream.0));
        }
        DebugEventKind::WriteOffered { record, stream } => {
            ui.label(format!("went out on stream {}", stream.0));
            show_mini(ui, record, false, ("detail-out", stream.0));
        }
        DebugEventKind::WriteDiscard { record, stream } => {
            ui.label(RichText::new(format!("discarded (stream {})", stream.0)).color(theme::WARN));
            show_mini(ui, record, false, ("detail-disc", stream.0));
        }
        DebugEventKind::ReadEof { stream } => {
            ui.label(format!("end of input on stream {}", stream.0));
        }
        _ => {}
    }
}

fn json_cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn show_json(ui: &mut Ui, v: &serde_json::Value, depth: usize) {
    if depth > 6 {
        ui.monospace("…");
        return;
    }
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                if val.is_object() || val.is_array() {
                    ui.collapsing(k.clone(), |ui| show_json(ui, val, depth + 1));
                } else {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(k).color(theme::MUTED));
                        ui.monospace(json_cell(val));
                    });
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().take(TABLE_ROWS).enumerate() {
                ui.collapsing(format!("[{i}]"), |ui| show_json(ui, val, depth + 1));
            }
            if arr.len() > TABLE_ROWS {
                ui.label(format!("… {} more", arr.len() - TABLE_ROWS));
            }
        }
        other => {
            ui.monospace(json_cell(other));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pipe_core::{RecordPreview, StreamId};

    fn ev(kind: DebugEventKind) -> DebugEvent {
        DebugEvent {
            stage: StageId(1),
            name: "LOCATE".into(),
            kind,
        }
    }

    fn rec(s: &str) -> RecordPreview {
        RecordPreview {
            len: s.len(),
            bytes: s.as_bytes().to_vec(),
        }
    }

    #[test]
    fn counts_are_from_events_not_inferred() {
        let events = vec![
            ev(DebugEventKind::ReadTaken {
                stream: StreamId(0),
                record: rec("apricot"),
            }),
            ev(DebugEventKind::WriteOffered {
                stream: StreamId(0),
                record: rec("apricot"),
            }),
            ev(DebugEventKind::ReadTaken {
                stream: StreamId(0),
                record: rec("banana"),
            }),
            ev(DebugEventKind::WriteDiscard {
                stream: StreamId(0),
                record: rec("banana"),
            }),
            ev(DebugEventKind::ReadTaken {
                stream: StreamId(0),
                record: rec("apple"),
            }),
            ev(DebugEventKind::WriteOffered {
                stream: StreamId(0),
                record: rec("apple"),
            }),
        ];
        let c = counts_for(&events, StageId(1));
        assert_eq!(c.received, 3);
        assert_eq!(c.sent, 2);
        assert_eq!(c.discarded, 1);
        let other = counts_for(&events, StageId(9));
        assert_eq!(other.received, 0);
    }
}
