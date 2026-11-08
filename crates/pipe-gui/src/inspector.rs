//! Plain-language inspector. Counts come from recorded debug events.

use eframe::egui::{self, RichText, Ui};
use pipe_core::{DebugEvent, DebugEventKind, LiveStage, RecordPreview, SlotSnapshot, StageId};

use crate::theme;
use crate::worker::Snapshot;

const HISTORY: usize = 24;
const HEX: usize = 64;
const TABLE_ROWS: usize = 16;
const TABLE_COLS: usize = 8;

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

const HOVER_HIST: usize = 4;

/// Compact record-centric pane. Details stay behind a disclosure.
pub fn show_compact(
    ui: &mut Ui,
    snap: Option<&Snapshot>,
    id: Option<StageId>,
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
    let Some(id) = id else {
        ui.label(RichText::new("Select a stage to inspect records.").color(theme::MUTED));
        return;
    };
    let live = snap.live.iter().find(|s| s.id == id);
    if let Some(st) = live {
        let state = if st.ended {
            format!("ended rc {}", st.return_code)
        } else if st.waiting_debug {
            "waiting".to_string()
        } else if st.waiting_callpipe {
            "waiting in CALLPIPE".to_string()
        } else {
            "in this session".to_string()
        };
        ui.label(format!("{} · {state}", st.name));
    }
    let c = counts_for(&snap.events, id);
    ui.label(format!(
        "observed  in {}  out {}  discarded {}",
        c.received, c.sent, c.discarded
    ));

    ui.add_space(6.0);
    ui.label(RichText::new("Waiting now").strong());
    ui.label(
        RichText::new("The record in a pipe, if any. Not a replay.")
            .small()
            .color(theme::MUTED),
    );
    let mut waiting = 0usize;
    for slot in snap
        .slots
        .iter()
        .filter(|s| s.writer == id || s.reader == id)
    {
        waiting += 1;
        show_slot_compact(ui, slot, id);
    }
    if waiting == 0 {
        ui.label(RichText::new("Nothing waiting on a connection.").color(theme::MUTED));
    }

    ui.add_space(6.0);
    ui.label(RichText::new("Recent observations").strong());
    ui.label(
        RichText::new(format!(
            "Last {HOVER_HIST} recorded events for this stage. Bounded; not a complete history."
        ))
        .small()
        .color(theme::MUTED),
    );
    let hist = recent_record_events(&snap.events, id, HOVER_HIST);
    if hist.is_empty() {
        ui.label(RichText::new("None observed yet.").color(theme::MUTED));
    }
    for ev in hist {
        show_event(ui, ev);
    }

    ui.add_space(8.0);
    ui.checkbox(details_open, "More details");
    if *details_open {
        show_stage(ui, snap, id, live, None);
    }
}

/// Tiny hover card. Does not read files or run stages.
pub fn show_hover_card(
    ui: &mut Ui,
    snap: Option<&Snapshot>,
    id: Option<StageId>,
    title: &str,
    detail: &str,
) {
    ui.set_max_width(320.0);
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
    let Some(id) = id else {
        return;
    };
    let c = counts_for(&snap.events, id);
    ui.label(
        RichText::new(format!(
            "observed in {} · out {} · discarded {}",
            c.received, c.sent, c.discarded
        ))
        .small(),
    );
    let mut shown_wait = false;
    for slot in snap
        .slots
        .iter()
        .filter(|s| (s.writer == id || s.reader == id) && s.record.is_some())
        .take(1)
    {
        shown_wait = true;
        ui.label(RichText::new("Waiting now").small().strong());
        if let Some(rec) = &slot.record {
            show_preview(ui, rec, "waiting");
        }
    }
    if !shown_wait {
        ui.label(
            RichText::new("Nothing waiting.")
                .small()
                .color(theme::MUTED),
        );
    }
    let hist = recent_record_events(&snap.events, id, 2);
    if !hist.is_empty() {
        ui.label(RichText::new("Recent (bounded)").small().strong());
        for ev in hist {
            show_event(ui, ev);
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

fn recent_record_events(events: &[DebugEvent], id: StageId, n: usize) -> Vec<&DebugEvent> {
    events
        .iter()
        .filter(|e| e.stage == id && event_has_record(&e.kind))
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn show_slot_compact(ui: &mut Ui, slot: &SlotSnapshot, id: StageId) {
    let role = if slot.writer == id { "output" } else { "input" };
    let wait = if slot.record.is_some() {
        "a record is waiting"
    } else if slot.severed || slot.writer_done {
        "empty — writer finished"
    } else {
        "empty"
    };
    ui.label(format!("{role}: {wait}"));
    if let Some(rec) = &slot.record {
        show_preview(ui, rec, "waiting record");
    }
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
        show_preview(ui, rec, "waiting record");
    }
}

fn show_event(ui: &mut Ui, ev: &DebugEvent) {
    match &ev.kind {
        DebugEventKind::ReadTaken { record, stream } => {
            ui.label(format!("came in on stream {}", stream.0));
            show_preview(ui, record, "read");
        }
        DebugEventKind::WriteOffered { record, stream } => {
            ui.label(format!("went out on stream {}", stream.0));
            show_preview(ui, record, "written");
        }
        DebugEventKind::WriteDiscard { record, stream } => {
            ui.label(RichText::new(format!("discarded (stream {})", stream.0)).color(theme::WARN));
            show_preview(ui, record, "discarded");
        }
        DebugEventKind::ReadEof { stream } => {
            ui.label(format!("end of input on stream {}", stream.0));
        }
        _ => {}
    }
}

fn show_preview(ui: &mut Ui, rec: &RecordPreview, caption: &str) {
    ui.label(
        RichText::new(format!("{caption}: {} bytes", rec.len))
            .small()
            .color(theme::MUTED),
    );
    match std::str::from_utf8(&rec.bytes) {
        Ok(s) => show_text_record(ui, s, rec.len > rec.bytes.len()),
        Err(_) => {
            ui.monospace(hex_preview(&rec.bytes));
            if rec.len > rec.bytes.len() {
                ui.label(
                    RichText::new("truncated in the debugger; the live record was longer")
                        .small()
                        .color(theme::MUTED),
                );
            }
        }
    }
}

fn show_text_record(ui: &mut Ui, s: &str, truncated: bool) {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        if v.is_object() || v.is_array() {
            if let Some(table) = object_array_table(&v) {
                show_table(ui, &table);
            }
            show_json(ui, &v, 0);
            if truncated {
                ui.label(
                    RichText::new("JSON truncated for display")
                        .small()
                        .color(theme::MUTED),
                );
            }
            return;
        }
    }
    ui.monospace(s);
    if truncated {
        ui.label(
            RichText::new("truncated for display")
                .small()
                .color(theme::MUTED),
        );
    }
}

fn object_array_table(v: &serde_json::Value) -> Option<(Vec<String>, Vec<Vec<String>>)> {
    let arr = v.as_array()?;
    if arr.is_empty() || arr.len() > TABLE_ROWS {
        return None;
    }
    if !arr.iter().all(|x| x.is_object()) {
        return None;
    }
    let mut keys = Vec::new();
    for item in arr {
        if let Some(map) = item.as_object() {
            for k in map.keys() {
                if !keys.iter().any(|e| e == k) {
                    keys.push(k.clone());
                }
            }
        }
    }
    if keys.is_empty() || keys.len() > TABLE_COLS {
        return None;
    }
    let mut rows = Vec::new();
    for item in arr {
        let map = item.as_object()?;
        rows.push(
            keys.iter()
                .map(|k| json_cell(map.get(k).unwrap_or(&serde_json::Value::Null)))
                .collect(),
        );
    }
    Some((keys, rows))
}

fn json_cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn show_table(ui: &mut Ui, table: &(Vec<String>, Vec<Vec<String>>)) {
    egui::Grid::new(ui.next_auto_id())
        .striped(true)
        .show(ui, |ui| {
            for h in &table.0 {
                ui.label(RichText::new(h).strong().small());
            }
            ui.end_row();
            for row in &table.1 {
                for cell in row {
                    ui.label(RichText::new(cell).small().monospace());
                }
                ui.end_row();
            }
        });
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

fn hex_preview(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(HEX)
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
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
