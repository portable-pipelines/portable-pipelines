//! Drawn record-state marks. Shapes, not font glyphs — egui’s default fonts
//! lack ⏳/→/✓/✕ on some Windows/Linux/macOS machines (they show as tofu).

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::preview::{ObservedKind, WaitingKind};
use crate::theme;

pub fn mark_for_waiting(kind: WaitingKind) -> RecordMark {
    match kind {
        WaitingKind::Waiting => RecordMark::Waiting,
        WaitingKind::Empty | WaitingKind::WriterFinished => RecordMark::Empty,
    }
}

pub fn mark_for_observed(kind: ObservedKind) -> RecordMark {
    match kind {
        ObservedKind::WentOut => RecordMark::Sent,
        ObservedKind::CameIn => RecordMark::Passed,
        ObservedKind::Discarded => RecordMark::Discarded,
        ObservedKind::InputEnded => RecordMark::Empty,
    }
}

pub fn mark_for_chip(kind: crate::preview::ChipKind) -> RecordMark {
    match kind {
        crate::preview::ChipKind::Waiting => RecordMark::Waiting,
        crate::preview::ChipKind::Sent => RecordMark::Sent,
        crate::preview::ChipKind::Discarded => RecordMark::Discarded,
        crate::preview::ChipKind::Empty => RecordMark::Empty,
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
    /// Dim ring: nothing waiting on this connection.
    Empty,
}

pub fn mark_color(mark: RecordMark) -> Color32 {
    match mark {
        RecordMark::Waiting => Color32::from_rgb(230, 168, 42),
        RecordMark::Sent => theme::ACCENT,
        RecordMark::Passed => Color32::from_rgb(46, 184, 140),
        RecordMark::Discarded => Color32::from_rgb(196, 96, 96),
        RecordMark::Empty => theme::MUTED,
    }
}

pub fn mark_word(mark: RecordMark) -> &'static str {
    match mark {
        RecordMark::Waiting => "waiting",
        RecordMark::Sent => "sent",
        RecordMark::Passed => "read and passed on",
        RecordMark::Discarded => "discarded",
        RecordMark::Empty => "nothing waiting",
    }
}

pub fn mark_legend_tip(mark: RecordMark) -> &'static str {
    match mark {
        RecordMark::Waiting => "Hourglass: a record is waiting for a stage to read it.",
        RecordMark::Sent => "Arrow: a record was sent along a connection.",
        RecordMark::Passed => "Check: this stage read the record and sent it on.",
        RecordMark::Discarded => "X: the record was discarded instead of sent.",
        RecordMark::Empty => "Dim ring: nothing is waiting on this connection.",
    }
}

pub fn paint_mark(painter: &egui::Painter, center: Pos2, size: f32, mark: RecordMark) {
    let color = mark_color(mark);
    match mark {
        RecordMark::Waiting => paint_hourglass(painter, center, size, color),
        RecordMark::Sent => paint_arrow(painter, center, size, color),
        RecordMark::Passed => paint_check(painter, center, size, color),
        RecordMark::Discarded => paint_x(painter, center, size, color),
        RecordMark::Empty => paint_ring(painter, center, size, color),
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

fn paint_ring(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    painter.circle_stroke(c, s * 0.28, Stroke::new(1.3_f32, color));
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
            RecordMark::Sent,
            RecordMark::Passed,
            RecordMark::Discarded,
            RecordMark::Empty,
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
            RecordMark::Empty,
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
        assert!(mark_legend_tip(RecordMark::Waiting).contains("Hourglass"));
        assert!(mark_legend_tip(RecordMark::Sent).contains("Arrow"));
        assert!(mark_legend_tip(RecordMark::Passed).contains("Check"));
        assert!(mark_legend_tip(RecordMark::Discarded).starts_with("X:"));
        assert!(mark_legend_tip(RecordMark::Empty).contains("ring"));
    }
}
