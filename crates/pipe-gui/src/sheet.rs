//! Compact spreadsheet-style record grid for the Records pane.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, MutexGuard, OnceLock};

use eframe::egui::{
    self, text::LayoutJob, Color32, CursorIcon, FontId, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, TextFormat, Ui, Vec2,
};

use crate::preview::{MiniCell, MiniKind, MiniTable};
use crate::theme;

const ROW_H: f32 = 20.0;
const HEADER_H: f32 = 22.0;
const NUM_W: f32 = 32.0;
const CELL_PAD: f32 = 5.0;
const GRIDLINE: Color32 = Color32::from_rgb(72, 80, 92);
const HEADER_BG: Color32 = Color32::from_rgb(38, 48, 62);
const HANDLE: f32 = 5.0;
const HANDLE_HOVER: Color32 = Color32::from_rgb(180, 196, 220);

/// Narrowest a data column can be dragged.
pub const MIN_COL_W: f32 = 48.0;
/// Widest a data column can be dragged.
pub const MAX_COL_W: f32 = 560.0;

struct ColumnStore {
    widths: HashMap<String, Vec<f32>>,
    seed: Option<Vec<f32>>,
    loaded: bool,
    resizing: bool,
}

fn store() -> MutexGuard<'static, ColumnStore> {
    static STORE: OnceLock<Mutex<ColumnStore>> = OnceLock::new();
    STORE
        .get_or_init(|| {
            Mutex::new(ColumnStore {
                widths: HashMap::new(),
                seed: None,
                loaded: false,
                resizing: false,
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn ensure_loaded(st: &mut ColumnStore) {
    if st.loaded {
        return;
    }
    st.loaded = true;
    for (k, v) in crate::settings::load_column_widths() {
        st.widths.entry(k).or_insert(v);
    }
}

/// Screenshot/test aid: use these widths until a grid is resized.
pub fn seed_session_widths(widths: &[f32]) {
    let mut st = store();
    st.seed = Some(widths.iter().copied().map(clamp_width).collect());
}

/// True while a header border is being dragged.
pub fn is_resizing() -> bool {
    store().resizing
}

/// Wheel and bars always; content-drag off while a column border is dragged.
pub fn scroll_source() -> egui::containers::scroll_area::ScrollSource {
    use egui::containers::scroll_area::ScrollSource;
    if is_resizing() {
        ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL
    } else {
        ScrollSource::ALL
    }
}

fn sheet_scroll_source(allow_drag: bool) -> egui::containers::scroll_area::ScrollSource {
    use egui::containers::scroll_area::ScrollSource;
    if allow_drag {
        ScrollSource::ALL
    } else {
        ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL
    }
}

fn set_resizing(on: bool) {
    store().resizing = on;
}

/// Merge stored (or seed) widths with this frame's auto-fit defaults.
pub fn merge_widths(stored: &[f32], auto: &[f32]) -> Vec<f32> {
    auto.iter()
        .enumerate()
        .map(|(i, a)| {
            stored
                .get(i)
                .copied()
                .unwrap_or(*a)
                .clamp(MIN_COL_W, MAX_COL_W)
        })
        .collect()
}

pub fn clamp_width(w: f32) -> f32 {
    w.clamp(MIN_COL_W, MAX_COL_W)
}

/// Viewport width for a pane-hosted sheet. Hover cards keep a fixed cap.
pub fn pane_viewport_width(available: f32) -> f32 {
    available.max(160.0)
}

/// True when the sheet is wider than the pane and needs a horizontal bar.
pub fn content_overflows(content_w: f32, viewport_w: f32) -> bool {
    content_w > viewport_w + 0.5
}

/// Fit one column to its header and cell text.
pub fn auto_fit_col(header: &str, rows: &[Vec<MiniCell>], col: usize) -> f32 {
    let mut chars = header.chars().count().max(1);
    for row in rows {
        if let Some(cell) = row.get(col) {
            chars = chars.max(cell_plain(cell).chars().count().min(80));
        }
    }
    clamp_width(chars as f32 * 7.4 + 12.0)
}

fn default_col_width(header: &str, rows: &[Vec<MiniCell>], col: usize) -> f32 {
    let mut chars = header.chars().count().max(3);
    for row in rows.iter().take(24) {
        if let Some(cell) = row.get(col) {
            chars = chars.max(cell_plain(cell).chars().count().min(22));
        }
    }
    (chars as f32 * 7.4 + 12.0).clamp(72.0, 168.0)
}

fn resolved_widths(key: &str, auto: &[f32]) -> Vec<f32> {
    let mut st = store();
    ensure_loaded(&mut st);
    if let Some(stored) = st.widths.get(key) {
        return merge_widths(stored, auto);
    }
    if let Some(seed) = st.seed.clone() {
        return merge_widths(&seed, auto);
    }
    auto.to_vec()
}

fn put_widths(key: &str, widths: &[f32]) {
    let mut st = store();
    ensure_loaded(&mut st);
    st.widths.insert(key.to_string(), widths.to_vec());
}

fn persist_widths() {
    let st = store();
    let _ = crate::settings::save_column_widths(&st.widths);
}

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
pub fn show_sheet(
    ui: &mut Ui,
    table: &MiniTable,
    compact: bool,
    salt: impl std::hash::Hash,
    grid_key: &str,
) {
    show_sheet_from(ui, table, compact, salt, 1, grid_key);
}

/// Like [`show_sheet`], numbering rows from `first_seq` (1-based).
pub fn show_sheet_from(
    ui: &mut Ui,
    table: &MiniTable,
    compact: bool,
    salt: impl std::hash::Hash,
    first_seq: usize,
    grid_key: &str,
) {
    let headers = sheet_headers(table);
    if headers.is_empty() || table.rows.is_empty() {
        return;
    }
    let cols = headers.len();
    let auto: Vec<f32> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| default_col_width(h, table.rows.as_slice(), i))
        .collect();
    let mut widths = resolved_widths(grid_key, &auto);
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
        pane_viewport_width(ui.available_width())
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
    if !compact {
        ui.set_min_width(max_w);
    }
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
                .scroll_source(sheet_scroll_source(false))
                .scroll_bar_visibility(bars(overflow_x))
                .show(ui, |ui| {
                    ui.set_min_width(total_w);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    header_row(ui, table, &headers, &mut widths, grid_key);
                    egui::ScrollArea::vertical()
                        .id_salt(("pp-sheet-v", ui.id(), key))
                        .max_height(v_h)
                        .auto_shrink([true, !overflow_y])
                        .scroll_source(sheet_scroll_source(false))
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

fn header_row(
    ui: &mut Ui,
    table: &MiniTable,
    headers: &[String],
    widths: &mut [f32],
    grid_key: &str,
) {
    let mut cell_rects = Vec::with_capacity(headers.len());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        paint_cell(ui, NUM_W, HEADER_H, "#", "", HEADER_BG, true, true);
        for (h, w) in headers.iter().zip(widths.iter()) {
            cell_rects.push(paint_cell(ui, *w, HEADER_H, h, h, HEADER_BG, true, false));
        }
    });
    let mut persist = false;
    let mut dragging = false;
    for (i, rect) in cell_rects.iter().enumerate() {
        let last = i + 1 == headers.len();
        let handle = if last {
            Rect::from_min_max(
                Pos2::new(rect.right() - HANDLE * 2.0, rect.top()),
                Pos2::new(rect.right(), rect.bottom()),
            )
        } else {
            Rect::from_min_max(
                Pos2::new(rect.right() - HANDLE, rect.top()),
                Pos2::new(rect.right() + HANDLE, rect.bottom()),
            )
        };
        let id = ui.id().with(("pp-col-resize", grid_key, i));
        let resp = ui.interact(handle, id, Sense::click_and_drag());
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
            ui.painter().vline(
                rect.right() - 0.5,
                rect.y_range(),
                Stroke::new(2.0_f32, HANDLE_HOVER),
            );
        }
        if resp.double_clicked() {
            widths[i] = auto_fit_col(&headers[i], table.rows.as_slice(), i);
            persist = true;
        } else if resp.dragged() {
            widths[i] = clamp_width(widths[i] + resp.drag_delta().x);
            dragging = true;
            ui.ctx().request_repaint();
        }
        if resp.drag_stopped() {
            persist = true;
        }
    }
    set_resizing(dragging);
    if dragging || persist {
        put_widths(grid_key, widths);
    }
    if persist {
        set_resizing(false);
        persist_widths();
    }
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
) -> Rect {
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
    let elided = galley.elided;
    painter.galley(egui::Pos2::new(rect.left() + CELL_PAD, y), galley, color);
    if !full.is_empty() && (elided || full != text || full.chars().count() > 18) {
        resp.on_hover_text(RichText::new(full).monospace());
    }
    rect
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

    #[test]
    fn merge_keeps_stored_widths_when_rows_grow() {
        let auto = vec![100.0, 110.0];
        let stored = vec![52.0, 240.0];
        assert_eq!(merge_widths(&stored, &auto), vec![52.0, 240.0]);
        let auto3 = vec![100.0, 110.0, 90.0];
        assert_eq!(merge_widths(&stored, &auto3), vec![52.0, 240.0, 90.0]);
        assert_eq!(
            merge_widths(&[10.0, 9000.0], &auto),
            vec![MIN_COL_W, MAX_COL_W]
        );
    }

    #[test]
    fn auto_fit_widens_for_long_cells_and_respects_min() {
        let t = mini_table(&rec(
            r#"{"name":"Kestrel-the-long-named-vessel","berth":"B"}"#,
        ));
        let h = sheet_headers(&t);
        let name_i = h.iter().position(|x| x == "name").unwrap();
        let berth_i = h.iter().position(|x| x == "berth").unwrap();
        let name_w = auto_fit_col(&h[name_i], &t.rows, name_i);
        let berth_w = auto_fit_col(&h[berth_i], &t.rows, berth_i);
        assert!(name_w > berth_w, "{name_w} vs {berth_w}");
        assert!(name_w >= MIN_COL_W && name_w <= MAX_COL_W);
        assert!(berth_w >= MIN_COL_W);
    }

    #[test]
    fn default_width_stays_in_the_compact_band() {
        let t = mini_table(&rec("x"));
        let w = default_col_width("value", &t.rows, 0);
        assert!((72.0..=168.0).contains(&w), "{w}");
    }

    #[test]
    fn pane_grid_fills_available_width_and_scrolls_when_wider() {
        assert_eq!(pane_viewport_width(400.0), 400.0);
        assert_eq!(pane_viewport_width(10.0), 160.0);
        assert!(!content_overflows(200.0, 400.0));
        assert!(content_overflows(800.0, 400.0));
    }
}
