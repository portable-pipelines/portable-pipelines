//! Plain-language inspector. Counts come from recorded debug events.

use eframe::egui::{self, Align, Layout, RichText, Ui};
use pipe_core::{DebugEvent, DebugEventKind, LiveStage, RecordPreview, SlotSnapshot, StageId};

use crate::preview::{
    counts_for_focus, counts_for_stage, history_for, json_mini_table, mini_table, preview_copy,
    waiting_items, FocusLabels, MiniCell, MiniKind, RecordFocus, RunOverview, WaitingKind,
};
use crate::sheet;
use crate::symbols::{
    heading_mark_place, mark_color, mark_for_chip, mark_for_waiting, mark_icon, mark_line,
    HeadingMarkPlace, RecordMark,
};
use crate::theme;
use crate::warnings::PipelineAbout;
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

/// At-a-glance session when nothing is selected.
pub fn show_run_overview(ui: &mut Ui, overview: &RunOverview) {
    if !overview.has_session {
        ui.label(RichText::new("No session yet. Step or Run to start.").color(theme::MUTED));
    } else {
        ui.label(RichText::new(&overview.status).strong());
        ui.label(
            RichText::new(if overview.steps == 1 {
                "1 step taken".into()
            } else {
                format!("{} steps taken", overview.steps)
            })
            .small()
            .color(theme::MUTED),
        );
        ui.add_space(6.0);
        for row in &overview.stages {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new(&row.name).strong());
                let run = crate::symbols::stage_run_mark(row.ended, row.waiting_in);
                mark_icon(ui, run, 12.0, crate::symbols::mark_legend_tip(run));
                if let Some(n) = row.holding {
                    let tip = row
                        .holding_line
                        .clone()
                        .unwrap_or_else(|| format!("Buffering {n} records"));
                    mark_icon(ui, RecordMark::Holding, 12.0, &tip);
                    ui.label(RichText::new(format!("{n}")).small().color(theme::MUTED))
                        .on_hover_text(&tip);
                }
                if row.sent > 0 {
                    mark_icon(
                        ui,
                        RecordMark::Sent,
                        12.0,
                        &format!("{} sent from {}", row.sent, row.name),
                    );
                    ui.label(
                        RichText::new(format!("{}", row.sent))
                            .small()
                            .color(theme::MUTED),
                    );
                }
                if row.discarded > 0 {
                    mark_icon(
                        ui,
                        RecordMark::Discarded,
                        12.0,
                        &format!("{} discarded at {}", row.discarded, row.name),
                    );
                    ui.label(
                        RichText::new(format!("{}", row.discarded))
                            .small()
                            .color(theme::MUTED),
                    );
                }
            });
        }
        ui.add_space(6.0);
        if overview.waiting.is_empty() {
            if !overview.stages.is_empty() && overview.stages.iter().all(|s| s.ended) {
                mark_line(
                    ui,
                    RecordMark::Done,
                    "All stages finished.",
                    crate::symbols::mark_legend_tip(RecordMark::Done),
                );
            } else {
                mark_line(
                    ui,
                    RecordMark::Idle,
                    "Idle, waiting for input",
                    crate::symbols::mark_legend_tip(RecordMark::Idle),
                );
            }
        } else {
            for w in &overview.waiting {
                mark_line(ui, RecordMark::Waiting, &w.line, &w.tooltip);
            }
        }
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Select a stage or connection to see its records.").color(theme::MUTED));
}

pub fn show_about(ui: &mut Ui, about: &PipelineAbout) {
    egui::CollapsingHeader::new(
        RichText::new("About this pipeline")
            .small()
            .color(theme::MUTED),
    )
    .default_open(false)
    .show(ui, |ui| {
        for line in about.lines() {
            ui.label(RichText::new(line).small().color(theme::MUTED));
        }
        if about.lines().is_empty() {
            ui.label(
                RichText::new("This specification does not read or write host files.")
                    .small()
                    .color(theme::MUTED),
            );
        }
    });
}

/// Compact record-centric pane. Details stay behind a disclosure.
pub fn show_compact(
    ui: &mut Ui,
    snap: Option<&Snapshot>,
    focus: Option<RecordFocus>,
    labels: &FocusLabels,
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
    show_focus_records(ui, snap, focus, labels, false);
    ui.add_space(8.0);
    ui.checkbox(details_open, "More details");
    if *details_open {
        if let RecordFocus::Stage(id) = focus {
            let live = snap.live.iter().find(|s| s.id == id);
            show_stage(ui, snap, id, live, snap.last_step.as_deref());
        } else {
            ui.label(
                RichText::new("This connection only shows records sent on it.")
                    .small()
                    .color(theme::MUTED),
            );
        }
    }
}

/// Tiny hover card. Does not read files or run stages.
/// Returns true when the Close button was clicked.
pub fn show_hover_card(
    ui: &mut Ui,
    snap: Option<&Snapshot>,
    focus: Option<RecordFocus>,
    labels: &FocusLabels,
    title: &str,
    detail: &str,
    pinned: bool,
) -> bool {
    theme::apply_compact_scroll(ui);
    ui.set_max_width(440.0);
    ui.set_max_height(520.0);
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("Close").clicked() {
                close = true;
            }
            if pinned {
                ui.label(RichText::new("Pinned").small().color(theme::MUTED));
            }
        });
    });
    egui::ScrollArea::vertical()
        .id_salt("hover-card-scroll")
        .max_height(500.0)
        .auto_shrink([true, true])
        .show(ui, |ui| {
            theme::apply_compact_scroll(ui);
            ui.set_max_width(440.0);
            show_hover_card_inner(ui, snap, focus, labels, detail);
        });
    close
}

fn show_hover_card_inner(
    ui: &mut Ui,
    snap: Option<&Snapshot>,
    focus: Option<RecordFocus>,
    labels: &FocusLabels,
    detail: &str,
) {
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
    show_focus_records(ui, snap, focus, labels, true);
}

fn show_focus_records(
    ui: &mut Ui,
    snap: &Snapshot,
    focus: RecordFocus,
    labels: &FocusLabels,
    compact: bool,
) {
    let waiting = waiting_items(&snap.slots, focus);
    let hist = history_for(&snap.events, focus, &snap.record_logs);
    let counts = match focus {
        RecordFocus::Stage(id) => counts_for_stage(&snap.events, id, &snap.io_counts),
        RecordFocus::Edge {
            writer,
            writer_stream,
            ..
        } => {
            let mut c = counts_for_focus(&snap.events, focus);
            if let Some(log) = snap.record_logs.iter().find(|(s, _)| *s == writer) {
                if let Some((_, n)) = log
                    .1
                    .sent_stream_totals
                    .iter()
                    .find(|(s, _)| *s == writer_stream)
                {
                    c.sent = *n;
                }
                if let Some((_, n)) = log
                    .1
                    .discarded_stream_totals
                    .iter()
                    .find(|(s, _)| *s == writer_stream)
                {
                    c.discarded = *n;
                }
            }
            c
        }
    };
    let copy = preview_copy(counts, &waiting, &hist, focus, labels);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for chip in &copy.chips {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                mark_icon(ui, mark_for_chip(chip.kind), 12.0, &chip.tooltip);
                ui.label(RichText::new(&chip.text).small().color(theme::MUTED))
                    .on_hover_text(&chip.tooltip);
            });
        }
    });

    ui.add_space(4.0);
    let stage_ended = match focus {
        RecordFocus::Stage(id) => snap.live.iter().any(|l| l.id == id && l.ended),
        RecordFocus::Edge { writer, .. } => snap.live.iter().any(|l| l.id == writer && l.ended),
    };
    if stage_ended {
        mark_line(
            ui,
            RecordMark::Done,
            "This stage has finished.",
            crate::symbols::mark_legend_tip(RecordMark::Done),
        );
        ui.add_space(4.0);
    }
    if let Some(h) = &copy.holding {
        mark_line(ui, RecordMark::Holding, &h.line, &h.tooltip);
        ui.add_space(4.0);
    }
    if copy.no_connection {
        ui.label(RichText::new("No connection in this session.").color(theme::MUTED));
    }

    // Data-flow order: Received (input waiting + sheet), then Sent
    // (output waiting + sheet). Stage-level idle stays above both.
    show_waiting_group(ui, &copy.waiting, None, compact, stage_ended);
    show_waiting_group(ui, &copy.waiting, Some(false), compact, stage_ended);
    if copy.no_more_input {
        ui.add_space(2.0);
        ui.label(RichText::new("No more input").small().color(theme::MUTED));
    }
    ui.add_space(6.0);
    show_dir_sheet(
        ui,
        copy.received_heading.as_deref(),
        &copy.received_table,
        copy.received_first_seq,
        compact,
        "recv",
        RecordMark::Received,
        false,
    );
    show_waiting_group(ui, &copy.waiting, Some(true), compact, stage_ended);
    show_dir_sheet(
        ui,
        copy.sent_heading.as_deref(),
        &copy.sent_table,
        copy.sent_first_seq,
        compact,
        "sent",
        RecordMark::Sent,
        copy.sent_still_waiting,
    );
    show_dir_sheet(
        ui,
        copy.discarded_heading.as_deref(),
        &copy.discarded_table,
        copy.discarded_first_seq,
        compact,
        "disc",
        RecordMark::Discarded,
        false,
    );
    if let Some(empty) = &copy.recent_empty {
        ui.label(RichText::new(empty).color(theme::MUTED));
    }
}

fn show_waiting_group(
    ui: &mut Ui,
    waiting: &[crate::preview::WaitingSectionCopy],
    on_output: Option<bool>,
    compact: bool,
    stage_ended: bool,
) {
    for (i, sec) in waiting.iter().enumerate() {
        if sec.on_output != on_output {
            continue;
        }
        let mark = mark_for_waiting(sec.kind);
        if stage_ended
            && sec.record.is_none()
            && matches!(sec.kind, WaitingKind::WriterFinished | WaitingKind::Empty)
        {
            continue;
        }
        let line = if !sec.heading.is_empty() {
            sec.heading.as_str()
        } else if sec.record.is_some() {
            "Waiting"
        } else if matches!(mark, RecordMark::Done) {
            "This stage has finished."
        } else {
            "Idle, waiting for input"
        };
        mark_line(ui, mark, line, &sec.tooltip);
        if let Some(rec) = &sec.record {
            show_mini(ui, rec, compact, ("waiting", i));
        } else if !sec.heading.is_empty() && sec.kind != WaitingKind::Waiting {
            ui.label(RichText::new("Idle, waiting for input.").color(theme::MUTED));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn show_dir_sheet(
    ui: &mut Ui,
    heading: Option<&str>,
    table: &crate::preview::MiniTable,
    first_seq: usize,
    compact: bool,
    salt: &str,
    mark: RecordMark,
    still_waiting: bool,
) {
    let Some(heading) = heading else {
        return;
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let tip = crate::symbols::mark_legend_tip(mark);
        let color = mark_color(mark);
        let paint_word = |ui: &mut Ui| {
            ui.label(RichText::new(heading).strong().color(color))
                .on_hover_text(tip);
        };
        match heading_mark_place(mark) {
            HeadingMarkPlace::Before => {
                mark_icon(ui, mark, 12.0, tip);
                paint_word(ui);
            }
            HeadingMarkPlace::After => {
                paint_word(ui);
                mark_icon(ui, mark, 12.0, tip);
            }
        }
        if still_waiting && matches!(mark, RecordMark::Sent) {
            mark_icon(
                ui,
                RecordMark::Waiting,
                12.0,
                "Still waiting for the next stage to read",
            );
            ui.label(
                RichText::new("still waiting")
                    .small()
                    .color(mark_color(RecordMark::Waiting)),
            )
            .on_hover_text("Still waiting for the next stage to read");
        }
    });
    if table.rows.is_empty() {
        return;
    }
    ui.push_id(salt, |ui| {
        sheet::show_sheet_from(ui, table, compact, salt, first_seq);
    });
    ui.add_space(4.0);
}

fn show_mini(ui: &mut Ui, rec: &RecordPreview, compact: bool, salt: impl std::hash::Hash) {
    ui.push_id(salt, |ui| {
        let table = mini_table(rec);
        match table.kind {
            MiniKind::Empty => {
                ui.label(RichText::new("empty record").small().color(theme::MUTED));
            }
            MiniKind::Binary => {
                ui.label(RichText::new("binary").small().color(theme::MUTED));
                show_mini_table(ui, &table, !compact, 1u64);
            }
            MiniKind::Text | MiniKind::Structured => {
                show_mini_table(ui, &table, !compact, 1u64);
            }
        }
        if table.truncated {
            ui.label(
                RichText::new(format!("truncated for display ({} bytes live)", rec.len))
                    .small()
                    .color(theme::MUTED),
            );
        }
    });
}

fn show_mini_table(
    ui: &mut Ui,
    table: &crate::preview::MiniTable,
    inspect_nested: bool,
    salt: impl std::hash::Hash,
) {
    if table.rows.is_empty() {
        return;
    }
    let compact = !inspect_nested;
    let parent = {
        use std::hash::Hasher;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        salt.hash(&mut h);
        h.finish()
    };
    sheet::show_sheet(ui, table, compact, (parent, compact));
    if !inspect_nested {
        return;
    }
    for (ri, row) in table.rows.iter().enumerate() {
        for (ci, cell) in row.iter().enumerate() {
            let MiniCell::Nested { summary, json } = cell else {
                continue;
            };
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("nested · {summary}"))
                    .small()
                    .color(theme::MUTED),
            );
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
                show_json(
                    ui,
                    &v,
                    0,
                    parent
                        .wrapping_mul(33)
                        .wrapping_add(1 + (ri as u64) * 32 + ci as u64)
                        .wrapping_add(if compact { 0 } else { 1 << 48 }),
                );
            } else {
                ui.monospace(json);
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
            mark_line(
                ui,
                RecordMark::Done,
                "This stage has finished.",
                crate::symbols::mark_legend_tip(RecordMark::Done),
            );
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

fn show_json(ui: &mut Ui, v: &serde_json::Value, depth: usize, salt: u64) {
    if depth > 6 {
        ui.monospace("…");
        return;
    }
    ui.push_id(("json-sheet", salt, depth), |ui| {
        show_json_inner(ui, v, depth, salt);
    });
}

fn show_json_inner(ui: &mut Ui, v: &serde_json::Value, depth: usize, salt: u64) {
    if let Some(table) = json_mini_table(v) {
        if !table.rows.is_empty() {
            sheet::show_sheet(ui, &table, true, (salt, depth));
            return;
        }
    }
    match v {
        serde_json::Value::Object(map) if map.is_empty() => {
            ui.label(RichText::new("empty object").small().color(theme::MUTED));
        }
        serde_json::Value::Array(arr) => {
            for (i, val) in arr.iter().take(TABLE_ROWS).enumerate() {
                ui.label(RichText::new(format!("[{i}]")).small().color(theme::MUTED));
                show_json(
                    ui,
                    val,
                    depth + 1,
                    salt.wrapping_mul(31).wrapping_add(i as u64),
                );
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
