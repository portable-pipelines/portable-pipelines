//! Drawn record-state marks. Shapes, not font glyphs — ⏳/→/✓/✕ showed as
//! tofu on some Windows/Linux/macOS machines. Idle (💤 U+1F4A4) and Done
//! (👍 U+1F44D) are in NotoEmoji-Regular’s cmap (emoji-icon-font has 👍 but
//! not 💤); a live egui source pane still rendered replacement glyphs, so
//! the painted silhouettes match the other marks at 12px. Codepoints are
//! kept as `IDLE_GLYPH` / `DONE_GLYPH`.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::preview::{ObservedKind, WaitingKind};
use crate::theme;

/// U+1F4A4. In NotoEmoji-Regular’s cmap; live egui still tofu’d — not painted.
pub const IDLE_GLYPH: char = '💤';
/// U+1F44D. In NotoEmoji-Regular’s cmap; live egui still tofu’d — not painted.
pub const DONE_GLYPH: char = '👍';

pub fn mark_for_waiting(kind: WaitingKind) -> RecordMark {
    match kind {
        WaitingKind::Waiting => RecordMark::Waiting,
        WaitingKind::Empty => RecordMark::Idle,
        WaitingKind::WriterFinished => RecordMark::Done,
    }
}

pub fn mark_for_observed(kind: ObservedKind) -> RecordMark {
    match kind {
        ObservedKind::WentOut => RecordMark::Sent,
        ObservedKind::CameIn => RecordMark::Passed,
        ObservedKind::Discarded => RecordMark::Discarded,
        ObservedKind::InputEnded => RecordMark::Done,
    }
}

pub fn mark_for_chip(kind: crate::preview::ChipKind) -> RecordMark {
    match kind {
        crate::preview::ChipKind::Waiting => RecordMark::Waiting,
        crate::preview::ChipKind::Sent => RecordMark::Sent,
        crate::preview::ChipKind::Discarded => RecordMark::Discarded,
        crate::preview::ChipKind::Empty => RecordMark::Idle,
    }
}

/// Compact state vocabulary. Color is paired with a distinct shape
/// so the marks stay readable for color-blind users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordMark {
    /// Amber hourglass: a record is sitting at a stage’s input, not yet read.
    Waiting,
    /// Blue arrow: the record was sent along this connection.
    Sent,
    /// Green check: this stage read it and passed it on.
    Passed,
    /// Grey-red X: the record was discarded instead of sent.
    Discarded,
    /// Sleepy blue zzz: idle, nothing on input, but the stage is not finished.
    Idle,
    /// Green thumbs-up: this stage has finished and will do no more work.
    Done,
    /// Violet stack: this stage is holding records inside (SORT, BUFFER).
    Holding,
}

/// Done vs idle when a stage has no live input waiting.
pub fn stage_run_mark(ended: bool, waiting_in: bool) -> RecordMark {
    if ended {
        RecordMark::Done
    } else if waiting_in {
        RecordMark::Waiting
    } else {
        RecordMark::Idle
    }
}

pub fn mark_color(mark: RecordMark) -> Color32 {
    match mark {
        RecordMark::Waiting => Color32::from_rgb(230, 168, 42),
        RecordMark::Sent => theme::ACCENT,
        RecordMark::Passed => Color32::from_rgb(46, 184, 140),
        RecordMark::Discarded => Color32::from_rgb(196, 96, 96),
        RecordMark::Idle => Color32::from_rgb(92, 148, 196),
        RecordMark::Done => Color32::from_rgb(58, 176, 98),
        RecordMark::Holding => Color32::from_rgb(140, 124, 220),
    }
}

pub fn mark_word(mark: RecordMark) -> &'static str {
    match mark {
        RecordMark::Waiting => "waiting",
        RecordMark::Sent => "sent",
        RecordMark::Passed => "read and passed on",
        RecordMark::Discarded => "discarded",
        RecordMark::Idle => "idle, waiting for input",
        RecordMark::Done => "done",
        RecordMark::Holding => "holding",
    }
}

pub fn mark_legend_tip(mark: RecordMark) -> &'static str {
    match mark {
        RecordMark::Waiting => "Hourglass: a record is waiting for a stage to read it.",
        RecordMark::Sent => "Arrow: a record was sent along a connection.",
        RecordMark::Passed => "Check: this stage read the record and sent it on.",
        RecordMark::Discarded => "X: the record was discarded instead of sent.",
        RecordMark::Idle => "zzz: this stage is idle, waiting for input. It is not finished.",
        RecordMark::Done => "Thumbs up: this stage has finished (done).",
        RecordMark::Holding => {
            "Stack: this stage is holding records inside until it is ready to send."
        }
    }
}

pub fn paint_mark(painter: &egui::Painter, center: Pos2, size: f32, mark: RecordMark) {
    let color = mark_color(mark);
    match mark {
        RecordMark::Waiting => paint_hourglass(painter, center, size, color),
        RecordMark::Sent => paint_arrow(painter, center, size, color),
        RecordMark::Passed => paint_check(painter, center, size, color),
        RecordMark::Discarded => paint_x(painter, center, size, color),
        RecordMark::Idle => paint_zzz(painter, center, size, color),
        RecordMark::Done => paint_thumbs(painter, center, size, color),
        RecordMark::Holding => paint_stack(painter, center, size, color),
    }
}

fn paint_hourglass(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let h = s * 0.48;
    let w = s * 0.36;
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x - w, c.y - h),
            Pos2::new(c.x + w, c.y - h),
            Pos2::new(c.x, c.y - s * 0.04),
        ],
        color,
        Stroke::NONE,
    ));
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x, c.y + s * 0.04),
            Pos2::new(c.x + w, c.y + h),
            Pos2::new(c.x - w, c.y + h),
        ],
        color,
        Stroke::NONE,
    ));
    painter.line_segment(
        [
            Pos2::new(c.x - s * 0.08, c.y),
            Pos2::new(c.x + s * 0.08, c.y),
        ],
        Stroke::new(1.1_f32, color),
    );
}

fn paint_arrow(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new((s * 0.16).max(1.4_f32), color);
    let x0 = c.x - s * 0.42;
    let x1 = c.x + s * 0.18;
    let tip = c.x + s * 0.46;
    painter.line_segment([Pos2::new(x0, c.y), Pos2::new(x1, c.y)], st);
    painter.line_segment(
        [
            Pos2::new(c.x + s * 0.04, c.y - s * 0.34),
            Pos2::new(tip, c.y),
        ],
        st,
    );
    painter.line_segment(
        [
            Pos2::new(c.x + s * 0.04, c.y + s * 0.34),
            Pos2::new(tip, c.y),
        ],
        st,
    );
}

fn paint_check(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new((s * 0.16).max(1.4_f32), color);
    let a = Pos2::new(c.x - s * 0.34, c.y + s * 0.04);
    let b = Pos2::new(c.x - s * 0.06, c.y + s * 0.34);
    let d = Pos2::new(c.x + s * 0.40, c.y - s * 0.32);
    painter.line_segment([a, b], st);
    painter.line_segment([b, d], st);
}

fn paint_x(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new((s * 0.16).max(1.4_f32), color);
    let o = s * 0.32;
    painter.line_segment(
        [Pos2::new(c.x - o, c.y - o), Pos2::new(c.x + o, c.y + o)],
        st,
    );
    painter.line_segment(
        [Pos2::new(c.x + o, c.y - o), Pos2::new(c.x - o, c.y + o)],
        st,
    );
}

/// Fist plus a thumb: 👍 without relying on an emoji font.
fn paint_thumbs(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let fist = egui::Rect::from_center_size(
        Pos2::new(c.x + s * 0.04, c.y + s * 0.12),
        Vec2::new(s * 0.58, s * 0.46),
    );
    painter.rect_filled(fist, 1.8_f32, color);
    let thumb = egui::Rect::from_center_size(
        Pos2::new(c.x - s * 0.14, c.y - s * 0.20),
        Vec2::new(s * 0.24, s * 0.52),
    );
    painter.rect_filled(thumb, 1.4_f32, color);
}

/// Three rising Z strokes: 💤 without relying on an emoji font.
fn paint_zzz(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let specs = [
        (-0.28_f32, 0.16_f32, 0.20_f32),
        (-0.02, -0.02, 0.28),
        (0.26, -0.20, 0.36),
    ];
    for (dx, dy, sc) in specs {
        paint_z(
            painter,
            Pos2::new(c.x + s * dx, c.y + s * dy),
            s * sc,
            color,
        );
    }
}

fn paint_z(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new((s * 0.22).max(1.15_f32), color);
    let w = s * 0.55;
    let h = s * 0.45;
    painter.line_segment(
        [Pos2::new(c.x - w, c.y - h), Pos2::new(c.x + w, c.y - h)],
        st,
    );
    painter.line_segment(
        [Pos2::new(c.x + w, c.y - h), Pos2::new(c.x - w, c.y + h)],
        st,
    );
    painter.line_segment(
        [Pos2::new(c.x - w, c.y + h), Pos2::new(c.x + w, c.y + h)],
        st,
    );
}

/// Three stacked bars: a pile of records, not an hourglass and not tofu ▤.
fn paint_stack(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let w = s * 0.40;
    let h = (s * 0.14).max(1.6_f32);
    let gap = (s * 0.08).max(1.0_f32);
    for i in 0..3 {
        let y = c.y + (i as f32 - 1.0) * (h + gap);
        painter.rect_filled(
            egui::Rect::from_center_size(Pos2::new(c.x, y), Vec2::new(w * 2.0, h)),
            0.6_f32,
            color,
        );
    }
}

/// Allocate a square and paint a mark. Hover shows `tooltip`.
pub fn mark_icon(ui: &mut Ui, mark: RecordMark, size: f32, tooltip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_mark(ui.painter(), rect.center(), size * 0.92, mark);
    if !tooltip.is_empty() {
        resp.clone().on_hover_text(tooltip)
    } else {
        resp
    }
}

pub fn mark_line(ui: &mut Ui, mark: RecordMark, text: &str, tooltip: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        mark_icon(ui, mark, 14.0, tooltip);
        ui.label(egui::RichText::new(text).strong());
    });
}

/// Always-visible one-line legend. Collapse to icons so it never eats the Records pane.
pub fn show_legend(ui: &mut Ui, expanded: &mut bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.label(
            egui::RichText::new("Marks")
                .small()
                .color(theme::MUTED)
                .strong(),
        );
        for mark in [
            RecordMark::Waiting,
            RecordMark::Idle,
            RecordMark::Done,
            RecordMark::Sent,
            RecordMark::Passed,
            RecordMark::Discarded,
            RecordMark::Holding,
        ] {
            let tip = mark_legend_tip(mark);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                mark_icon(ui, mark, 12.0, tip);
                if *expanded {
                    ui.label(
                        egui::RichText::new(mark_word(mark))
                            .small()
                            .color(theme::MUTED),
                    )
                    .on_hover_text(tip);
                }
            });
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if *expanded {
                "Hide words"
            } else {
                "Show words"
            };
            if ui.small_button(label).clicked() {
                *expanded = !*expanded;
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mark_has_its_own_shape_and_color() {
        let marks = [
            RecordMark::Waiting,
            RecordMark::Sent,
            RecordMark::Passed,
            RecordMark::Discarded,
            RecordMark::Idle,
            RecordMark::Done,
            RecordMark::Holding,
        ];
        let colors: Vec<_> = marks.iter().map(|m| mark_color(*m)).collect();
        for i in 0..colors.len() {
            for j in 0..colors.len() {
                if i != j {
                    assert_ne!(colors[i], colors[j], "{:?} vs {:?}", marks[i], marks[j]);
                }
            }
        }
        assert_eq!(mark_word(RecordMark::Waiting), "waiting");
        assert_eq!(mark_word(RecordMark::Sent), "sent");
        assert_eq!(mark_word(RecordMark::Idle), "idle, waiting for input");
        assert_eq!(mark_word(RecordMark::Done), "done");
        assert!(mark_legend_tip(RecordMark::Waiting).contains("Hourglass"));
        assert!(mark_legend_tip(RecordMark::Sent).contains("Arrow"));
        assert!(mark_legend_tip(RecordMark::Passed).contains("Check"));
        assert!(mark_legend_tip(RecordMark::Discarded).starts_with("X:"));
        assert!(mark_legend_tip(RecordMark::Idle).contains("idle"));
        assert!(mark_legend_tip(RecordMark::Done).contains("finished"));
        assert_eq!(mark_word(RecordMark::Holding), "holding");
        assert!(mark_legend_tip(RecordMark::Holding).contains("Stack"));
        assert_eq!(IDLE_GLYPH as u32, 0x1F4A4);
        assert_eq!(DONE_GLYPH as u32, 0x1F44D);
        assert_eq!(stage_run_mark(true, false), RecordMark::Done);
        assert_eq!(stage_run_mark(true, true), RecordMark::Done);
        assert_eq!(stage_run_mark(false, true), RecordMark::Waiting);
        assert_eq!(stage_run_mark(false, false), RecordMark::Idle);
        assert_eq!(mark_for_waiting(WaitingKind::Empty), RecordMark::Idle);
        assert_eq!(
            mark_for_waiting(WaitingKind::WriterFinished),
            RecordMark::Done
        );
    }
}
