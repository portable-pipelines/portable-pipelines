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
/// Smallest a grid frame can be dragged.
pub const MIN_FRAME: Vec2 = Vec2::new(NUM_W + MIN_COL_W, HEADER_H + ROW_H);
/// Largest a grid frame can be dragged (the pane or hover card then scrolls).
pub const MAX_FRAME: Vec2 = Vec2::new(1600.0, 560.0);
const FRAME_HANDLE: f32 = 6.0;

struct ColumnStore {
    widths: HashMap<String, Vec<f32>>,
    frames: HashMap<String, Vec2>,
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
                frames: HashMap::new(),
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
    for (k, (w, h)) in crate::settings::load_grid_frames() {
        st.frames.entry(k).or_insert(Vec2::new(w, h));
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

/// Content width of a sheet: row-number column plus data columns.
pub fn content_width(col_widths: &[f32]) -> f32 {
    NUM_W + col_widths.iter().sum::<f32>()
}

/// Default visible height for `visible` rows (plus header).
pub fn default_frame_height(rows: usize, visible: usize) -> f32 {
    HEADER_H + ROW_H * (rows.min(visible).max(1) as f32)
}

/// Grid frame size: tracks column content unless the user dragged the frame.
/// `preferred` is the last right/bottom-edge drag. Column widening grows
/// the stored width separately so the frame follows the columns.
pub fn fitted_frame(content: Vec2, preferred: Option<Vec2>) -> Vec2 {
    let w = preferred.map(|p| p.x).unwrap_or(content.x);
    let h = preferred.map(|p| p.y).unwrap_or(content.y);
    Vec2::new(
        w.clamp(MIN_FRAME.x, MAX_FRAME.x),
        h.clamp(MIN_FRAME.y, MAX_FRAME.y),
    )
}

/// After columns grow, bump a user-set frame so the grid stays at least as
/// wide as its columns (no inner bar from widening).
pub fn grow_frame_to_content(preferred: Option<Vec2>, content_w: f32) -> Option<Vec2> {
    preferred.map(|p| Vec2::new(p.x.max(content_w), p.y))
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

fn persist_layout() {
    let st = store();
    let _ = crate::settings::save_column_widths(&st.widths);
    let frames: HashMap<String, (f32, f32)> = st
        .frames
        .iter()
        .map(|(k, v)| (k.clone(), (v.x, v.y)))
        .collect();
    let _ = crate::settings::save_grid_frames(&frames);
}

fn resolved_frame(key: &str) -> Option<Vec2> {
    let mut st = store();
    ensure_loaded(&mut st);
    st.frames.get(key).copied()
}

fn put_frame(key: &str, size: Vec2) {
    let mut st = store();
    ensure_loaded(&mut st);
    st.frames.insert(
        key.to_string(),
        Vec2::new(
            size.x.clamp(MIN_FRAME.x, MAX_FRAME.x),
            size.y.clamp(MIN_FRAME.y, MAX_FRAME.y),
        ),
    );
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
    let total_w = content_width(&widths);
    let body_h = (table.rows.len() as f32 * ROW_H).max(ROW_H);
    let visible = if compact {
        crate::preview::HOVER_SHEET_ROWS
    } else {
        crate::preview::PANE_SHEET_ROWS
    };
    let default_h = default_frame_height(table.rows.len(), visible);
    let frame_size = fitted_frame(Vec2::new(total_w, default_h), resolved_frame(grid_key));
    let key = hash_u64(&salt);
    let overflow_y = body_h + HEADER_H > frame_size.y + 0.5;
    let overflow_x = total_w > frame_size.x + 0.5;
    let bars = |overflow: bool| {
        if overflow {
            egui::containers::scroll_area::ScrollBarVisibility::AlwaysVisible
        } else {
            egui::containers::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
        }
    };
    let v_h = (frame_size.y - HEADER_H).max(ROW_H);
    theme::apply_compact_scroll(ui);
    let inner = ui.allocate_ui(frame_size, |ui| {
        ui.set_min_size(frame_size);
        ui.set_max_size(frame_size);
        theme::apply_compact_scroll(ui);
        egui::Frame::new()
            .stroke(Stroke::new(1.0_f32, GRIDLINE))
            .inner_margin(0)
            .show(ui, |ui| {
                theme::apply_compact_scroll(ui);
                let mut draw_body = |ui: &mut Ui| {
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
                };
                if overflow_x {
                    egui::ScrollArea::horizontal()
                        .id_salt(("pp-sheet-h", ui.id(), key))
                        .max_height(frame_size.y)
                        .auto_shrink([false, false])
                        .scroll_source(sheet_scroll_source(false))
                        .scroll_bar_visibility(bars(true))
                        .show(ui, draw_body);
                } else {
                    draw_body(ui);
                }
            });
    });
    frame_edge_resize(ui, inner.response.rect, grid_key, frame_size);
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
        let content_w = content_width(widths);
        if let Some(grown) = grow_frame_to_content(resolved_frame(grid_key), content_w) {
            put_frame(grid_key, grown);
        }
    }
    if persist {
        set_resizing(false);
        persist_layout();
    }
}

fn frame_edge_resize(ui: &mut Ui, rect: Rect, grid_key: &str, current: Vec2) {
    // Handles sit just outside the frame so they do not steal the last
    // column-header border drag.
    let right = Rect::from_min_max(
        Pos2::new(rect.right(), rect.top()),
        Pos2::new(rect.right() + FRAME_HANDLE, rect.bottom()),
    );
    let bottom = Rect::from_min_max(
        Pos2::new(rect.left(), rect.bottom()),
        Pos2::new(rect.right(), rect.bottom() + FRAME_HANDLE),
    );
    let corner = Rect::from_min_max(
        Pos2::new(rect.right(), rect.bottom()),
        Pos2::new(rect.right() + FRAME_HANDLE, rect.bottom() + FRAME_HANDLE),
    );
    let mut size = current;
    let mut dragging = false;
    let mut persist = false;
    let mut hover_icon: Option<CursorIcon> = None;

    let corner_id = ui.id().with(("pp-frame-corner", grid_key));
    let corner_resp = ui.interact(corner, corner_id, Sense::click_and_drag());
    if corner_resp.hovered() || corner_resp.dragged() {
        hover_icon = Some(CursorIcon::ResizeNwSe);
        ui.painter().rect_filled(corner, 0.0, HANDLE_HOVER);
    }
    if corner_resp.dragged() {
        size += corner_resp.drag_delta();
        dragging = true;
        ui.ctx().request_repaint();
    }
    if corner_resp.drag_stopped() {
        persist = true;
    }

    let right_id = ui.id().with(("pp-frame-right", grid_key));
    let right_resp = ui.interact(right, right_id, Sense::click_and_drag());
    if (right_resp.hovered() || right_resp.dragged()) && hover_icon.is_none() {
        hover_icon = Some(CursorIcon::ResizeHorizontal);
        ui.painter().vline(
            rect.right() - 0.5,
            rect.y_range(),
            Stroke::new(2.0_f32, HANDLE_HOVER),
        );
    }
    if right_resp.dragged() && !corner_resp.dragged() {
        size.x += right_resp.drag_delta().x;
        dragging = true;
        ui.ctx().request_repaint();
    }
    if right_resp.drag_stopped() {
        persist = true;
    }

    let bottom_id = ui.id().with(("pp-frame-bottom", grid_key));
    let bottom_resp = ui.interact(bottom, bottom_id, Sense::click_and_drag());
    if (bottom_resp.hovered() || bottom_resp.dragged()) && hover_icon.is_none() {
        hover_icon = Some(CursorIcon::ResizeVertical);
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 0.5,
            Stroke::new(2.0_f32, HANDLE_HOVER),
        );
    }
    if bottom_resp.dragged() && !corner_resp.dragged() {
        size.y += bottom_resp.drag_delta().y;
        dragging = true;
        ui.ctx().request_repaint();
    }
    if bottom_resp.drag_stopped() {
        persist = true;
    }

    if let Some(icon) = hover_icon {
        ui.ctx().set_cursor_icon(icon);
    }
    if dragging || persist {
        put_frame(grid_key, fitted_frame(size, Some(size)));
        set_resizing(true);
    }
    if persist {
        set_resizing(false);
        persist_layout();
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
    fn frame_grows_with_columns_instead_of_a_fixed_cap() {
        let narrow = vec![72.0, 80.0];
        let wide = vec![72.0, 360.0];
        let n = content_width(&narrow);
        let w = content_width(&wide);
        assert!(w > n, "{w} vs {n}");
        assert!(
            w > 440.0,
            "wide columns must outgrow the old 440 cap, got {w}"
        );
        let frame_n = fitted_frame(Vec2::new(n, 80.0), None);
        let frame_w = fitted_frame(Vec2::new(w, 80.0), None);
        assert_eq!(frame_n.x, n);
        assert_eq!(frame_w.x, w);
        assert!(frame_w.x > frame_n.x);
    }

    #[test]
    fn user_frame_drag_can_be_wider_or_shorter_than_content() {
        let content = Vec2::new(200.0, 82.0);
        let wider = fitted_frame(content, Some(Vec2::new(360.0, 140.0)));
        assert_eq!(wider.x, 360.0);
        assert_eq!(wider.y, 140.0);
        let smaller = fitted_frame(content, Some(Vec2::new(120.0, 60.0)));
        assert_eq!(smaller.x, 120.0);
        assert!(smaller.y >= MIN_FRAME.y);
        let grown = grow_frame_to_content(Some(Vec2::new(120.0, 80.0)), 400.0).unwrap();
        assert_eq!(grown.x, 400.0);
        assert_eq!(grown.y, 80.0);
        assert!(grow_frame_to_content(None, 400.0).is_none());
    }

    #[test]
    fn default_frame_height_uses_visible_rows() {
        assert_eq!(default_frame_height(3, 10), HEADER_H + ROW_H * 3.0);
        assert_eq!(default_frame_height(40, 10), HEADER_H + ROW_H * 10.0);
    }
}
