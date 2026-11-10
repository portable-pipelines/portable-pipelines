//! Compact spreadsheet-style record grid for the Records pane.

use std::hash::{Hash, Hasher};

use eframe::egui::{
    self, text::LayoutJob, Color32, FontId, RichText, Sense, Stroke, StrokeKind, TextFormat, Ui,
    Vec2,
};

use crate::preview::{MiniCell, MiniKind, MiniTable};
use crate::theme;

const ROW_H: f32 = 20.0;
const HEADER_H: f32 = 22.0;
const NUM_W: f32 = 32.0;
const CELL_PAD: f32 = 5.0;
const GRIDLINE: Color32 = Color32::from_rgb(72, 80, 92);
const HEADER_BG: Color32 = Color32::from_rgb(38, 48, 62);

pub fn sheet_headers(table: &MiniTable) -> Vec<String> {
    if !table.headers.is_empty() {
        return table.headers.clone();
    }
    match table.kind {
        MiniKind::Binary => vec!["bytes".into()],
        MiniKind::Empty => Vec::new(),
        MiniKind::Text | MiniKind::Structured => vec!["value".into()],
    }
}

pub fn cell_plain(cell: &MiniCell) -> String {
    match cell {
        MiniCell::Empty => String::new(),
        MiniCell::Text(s) => s.clone(),
        MiniCell::Nested { summary, .. } => summary.clone(),
    }
}

pub fn cell_full(cell: &MiniCell) -> String {
    match cell {
        MiniCell::Empty => String::new(),
        MiniCell::Text(s) => s.clone(),
        MiniCell::Nested { json, .. } => json.clone(),
    }
}

/// Spreadsheet grid: header, row numbers, cell borders, stripes, scroll.
pub fn show_sheet(ui: &mut Ui, table: &MiniTable, compact: bool, salt: impl std::hash::Hash) {
    show_sheet_from(ui, table, compact, salt, 1);
}

/// Like [`show_sheet`], numbering rows from `first_seq` (1-based).
pub fn show_sheet_from(
    ui: &mut Ui,
    table: &MiniTable,
    compact: bool,
    salt: impl std::hash::Hash,
    first_seq: usize,
) {
    let headers = sheet_headers(table);
    if headers.is_empty() || table.rows.is_empty() {
        return;
    }
    let cols = headers.len();
    let mut widths = vec![0.0f32; cols];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = col_width(h, table.rows.as_slice(), i);
    }
    let total_w = NUM_W + widths.iter().sum::<f32>();
    let body_h = (table.rows.len() as f32 * ROW_H).max(ROW_H);
    let visible = if compact {
        crate::preview::HOVER_SHEET_ROWS
    } else {
        crate::preview::PANE_SHEET_ROWS
    };
    let max_h = HEADER_H + ROW_H * visible as f32;
    let max_w = if compact {
        440.0
    } else {
        ui.available_width().max(160.0)
    };
    let key = hash_u64(&salt);
    let overflow_y = body_h + HEADER_H > max_h + 0.5;
    let overflow_x = total_w > max_w + 0.5;
    let bars = |overflow: bool| {
        if overflow {
            egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible
        } else {
            egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
        }
    };
    let v_h = if overflow_y {
        (max_h - HEADER_H).max(ROW_H)
    } else {
        body_h.max(ROW_H)
    };

    ui.set_max_width(max_w);
    theme::apply_compact_scroll(ui);
    egui::Frame::new()
        .stroke(Stroke::new(1.0_f32, GRIDLINE))
        .inner_margin(0)
        .show(ui, |ui| {
            theme::apply_compact_scroll(ui);
            egui::ScrollArea::horizontal()
                .id_salt(("pp-sheet-h", ui.id(), key))
                .max_height(max_h)
                .auto_shrink([true, !overflow_y])
                .scroll_bar_visibility(bars(overflow_x))
                .show(ui, |ui| {
                    ui.set_min_width(total_w);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    header_row(ui, &headers, &widths);
                    egui::ScrollArea::vertical()
                        .id_salt(("pp-sheet-v", ui.id(), key))
                        .max_height(v_h)
                        .auto_shrink([true, !overflow_y])
                        .scroll_bar_visibility(bars(overflow_y))
                        .show(ui, |ui| {
                            theme::apply_compact_scroll(ui);
                            ui.set_min_width(total_w);
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            for (i, row) in table.rows.iter().enumerate() {
                                data_row(ui, first_seq.saturating_add(i), row, &widths, cols);
                            }
                        });
                });
        });
}

fn hash_u64(v: &impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn col_width(header: &str, rows: &[Vec<MiniCell>], col: usize) -> f32 {
    let mut chars = header.chars().count().max(3);
    for row in rows.iter().take(24) {
        if let Some(cell) = row.get(col) {
            chars = chars.max(cell_plain(cell).chars().count().min(22));
        }
    }
    (chars as f32 * 7.4 + 12.0).clamp(72.0, 168.0)
}

fn header_row(ui: &mut Ui, headers: &[String], widths: &[f32]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        paint_cell(ui, NUM_W, HEADER_H, "#", "", HEADER_BG, true, true);
        for (h, w) in headers.iter().zip(widths.iter()) {
            paint_cell(ui, *w, HEADER_H, h, h, HEADER_BG, true, false);
        }
    });
}

fn data_row(ui: &mut Ui, seq: usize, row: &[MiniCell], widths: &[f32], cols: usize) {
    let bg = if seq % 2 == 0 {
        theme::PANEL_ALT
    } else {
        theme::PANEL
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        paint_cell(ui, NUM_W, ROW_H, &format!("{seq}"), "", bg, false, true);
        for i in 0..cols {
            let cell = row.get(i).unwrap_or(&MiniCell::Empty);
            let plain = cell_plain(cell);
            let full = cell_full(cell);
            let shown = if plain.is_empty() {
                "—"
            } else {
                plain.as_str()
            };
            paint_cell(ui, widths[i], ROW_H, shown, &full, bg, false, false);
        }
    });
}

fn paint_cell(
    ui: &mut Ui,
    w: f32,
    h: f32,
    text: &str,
    full: &str,
    bg: Color32,
    header: bool,
    muted: bool,
) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, bg);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, GRIDLINE),
        StrokeKind::Inside,
    );
    let color = if muted {
        theme::MUTED
    } else if header {
        theme::TEXT
    } else {
        theme::TEXT
    };
    let font = FontId::monospace(11.0);
    let galley = ui.fonts(|f| {
        let mut job = LayoutJob::default();
        job.wrap.max_width = (w - CELL_PAD * 2.0).max(8.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: font,
                color,
                ..Default::default()
            },
        );
        f.layout_job(job)
    });
    let y = rect.top() + ((h - galley.size().y) * 0.5).max(0.0);
    painter.galley(egui::Pos2::new(rect.left() + CELL_PAD, y), galley, color);
    if !full.is_empty() && (full != text || full.chars().count() > 18) {
        resp.on_hover_text(RichText::new(full).monospace());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::{mini_table, rec, MiniKind};

    #[test]
    fn text_sheet_uses_value_header() {
        let t = mini_table(&rec("apple"));
        assert_eq!(sheet_headers(&t), vec!["value".to_string()]);
        assert_eq!(t.kind, MiniKind::Text);
        assert!(
            t.headers.is_empty(),
            "MiniTable keeps no invented CSV names"
        );
        assert_eq!(cell_plain(&t.rows[0][0]), "apple");
    }

    #[test]
    fn raw_csv_stays_one_value_column() {
        let line = "vessel_id,name,type,berth_id,owner_id";
        let t = mini_table(&rec(line));
        assert_eq!(t.kind, MiniKind::Text);
        assert_eq!(sheet_headers(&t), vec!["value".to_string()]);
        assert_eq!(t.rows[0].len(), 1);
        assert!(cell_plain(&t.rows[0][0]).contains("vessel_id,name"));
    }

    #[test]
    fn long_text_is_not_clipped_in_the_cell() {
        let s = "abcdefghijklmnopqrstuvwxyz-0123456789-ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let t = mini_table(&rec(s));
        assert_eq!(cell_full(&t.rows[0][0]), s);
        assert_eq!(cell_plain(&t.rows[0][0]), s);
        assert!(s.len() > 48);
    }

    #[test]
    fn json_sheet_keeps_field_headers() {
        let t = mini_table(&rec(r#"{"vessel_id":"V1","name":"Kestrel"}"#));
        let h = sheet_headers(&t);
        assert!(h.contains(&"vessel_id".into()), "{h:?}");
        assert!(h.contains(&"name".into()), "{h:?}");
        assert!(!h.iter().any(|x| x == "value"));
    }

    #[test]
    fn nested_json_cell_has_full_object_on_full_text() {
        let t = mini_table(&rec(r#"{"id":"V1","owner":{"name":"Ann"}}"#));
        assert!(matches!(t.rows[0][1], MiniCell::Nested { .. }));
        let full = cell_full(&t.rows[0][1]);
        assert!(full.contains("Ann"), "{full}");
    }
}
