//! Drawn record-state marks. Shapes, not font glyphs — ⏳/→/✓/↓ showed as
//! tofu on some Windows/Linux/macOS machines. Idle (💤 U+1F4A4) stays a
//! painted zzz. Done is a painted green check (✓), not a thumbs-up and not
//! “read and passed on”. Discarded is a painted down-arrow (↓); Ubuntu-Light
//! has no U+2193. Codepoints are kept as `IDLE_GLYPH` / `DONE_GLYPH` /
//! `DISCARDED_GLYPH`.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::preview::{ObservedKind, WaitingKind};
use crate::theme;

/// U+1F4A4. In NotoEmoji-Regular’s cmap; live egui still tofu’d — not painted.
pub const IDLE_GLYPH: char = '💤';
/// U+2713 CHECK MARK. Painted, not a font glyph — same tofu risk as ⏳/→.
pub const DONE_GLYPH: char = '✓';
/// U+2193 DOWNWARDS ARROW. Painted: Ubuntu-Light (proportional UI) has no ↓.
pub const DISCARDED_GLYPH: char = '↓';

/// Draw the done check a bit larger than other marks so it stays a ✓
/// at ~55% zoom (badge size is already clamped to 0.75×).
const DONE_SCALE: f32 = 1.25;
const DONE_MIN: f32 = 13.0;

pub fn mark_for_waiting(kind: WaitingKind) -> RecordMark {
    match kind {
        WaitingKind::Waiting => RecordMark::Waiting,
        WaitingKind::Empty => RecordMark::Idle,
        WaitingKind::WriterFinished => RecordMark::Done,
    }
}

pub fn mark_for_observed(kind: ObservedKind) -> Option<RecordMark> {
    match kind {
        ObservedKind::WentOut => Some(RecordMark::Sent),
        // Receiver-handled is implied by the sender’s → sent plus ⏳ waiting.
        ObservedKind::CameIn => None,
        ObservedKind::Discarded => Some(RecordMark::Discarded),
        ObservedKind::InputEnded => Some(RecordMark::Done),
    }
}

pub fn mark_for_chip(kind: crate::preview::ChipKind) -> RecordMark {
    match kind {
        crate::preview::ChipKind::Waiting => RecordMark::Waiting,
        crate::preview::ChipKind::Received => RecordMark::Received,
        crate::preview::ChipKind::Sent => RecordMark::Sent,
        crate::preview::ChipKind::Discarded => RecordMark::Discarded,
        crate::preview::ChipKind::Empty => RecordMark::Idle,
    }
}

pub fn mark_for_peek(status: crate::preview::PeekStatus) -> RecordMark {
    match status {
        crate::preview::PeekStatus::Finished => RecordMark::Done,
        crate::preview::PeekStatus::Waiting => RecordMark::Waiting,
        crate::preview::PeekStatus::Idle => RecordMark::Idle,
        crate::preview::PeekStatus::Buffering => RecordMark::Holding,
    }
}

/// Compact state vocabulary. Color is paired with a distinct shape
/// so the marks stay readable for color-blind users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordMark {
    /// Amber hourglass: a record is sitting at a stage’s input, not yet read.
    Waiting,
    /// Cyan right-pointing arrow: records flowing into this stage (shown as → N).
    Received,
    /// Accent right-pointing arrow: records this stage has sent (shown as N →).
    Sent,
    /// Grey-red down-arrow: the record was discarded instead of sent.
    Discarded,
    /// Sleepy blue zzz: idle, nothing on input, but the stage is not finished.
    Idle,
    /// Green check: this stage has finished and will do no more work.
    Done,
    /// Violet stack: this stage is buffering records inside (SORT, BUFFER).
    Holding,
}

/// Order in the always-visible legend.
pub const LEGEND_MARKS: [RecordMark; 7] = [
    RecordMark::Waiting,
    RecordMark::Idle,
    RecordMark::Received,
    RecordMark::Sent,
    RecordMark::Discarded,
    RecordMark::Holding,
    RecordMark::Done,
];

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
        RecordMark::Received => Color32::from_rgb(64, 196, 210),
        RecordMark::Sent => theme::ACCENT,
        RecordMark::Discarded => Color32::from_rgb(196, 96, 96),
        RecordMark::Idle => Color32::from_rgb(92, 148, 196),
        RecordMark::Done => Color32::from_rgb(46, 176, 96),
        RecordMark::Holding => Color32::from_rgb(140, 124, 220),
    }
}

pub fn mark_word(mark: RecordMark) -> &'static str {
    match mark {
        RecordMark::Waiting => "waiting",
        RecordMark::Received => "in",
        RecordMark::Sent => "out",
        RecordMark::Discarded => "discarded",
        RecordMark::Idle => "idle (waiting for input)",
        RecordMark::Done => "done",
        RecordMark::Holding => "buffering",
    }
}

pub fn mark_legend_tip(mark: RecordMark) -> &'static str {
    match mark {
        RecordMark::Waiting => "Hourglass: a record is waiting for a stage to read it.",
        RecordMark::Received => {
            "Arrow in: records flowing into this stage (shown as → N, pointing right)."
        }
        RecordMark::Sent => {
            "Arrow out: records this stage has sent (shown as N →, pointing right)."
        }
        RecordMark::Discarded => {
            "Down arrow: the record was discarded instead of sent (shown as ↓ N, pointing down)."
        }
        RecordMark::Idle => "zzz: this stage is idle, waiting for input. It is not finished.",
        RecordMark::Done => "Check: this stage has finished (done).",
        RecordMark::Holding => {
            "Stack: this stage is buffering records inside until it is ready to send."
        }
    }
}

/// Where the section-heading mark sits, matching the in/out badges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingMarkPlace {
    /// `→ Received` and `↓ Discarded`.
    Before,
    /// `Sent →`.
    After,
}

pub fn heading_mark_place(mark: RecordMark) -> HeadingMarkPlace {
    match mark {
        RecordMark::Sent => HeadingMarkPlace::After,
        _ => HeadingMarkPlace::Before,
    }
}

pub fn paint_mark(painter: &egui::Painter, center: Pos2, size: f32, mark: RecordMark) {
    let color = mark_color(mark);
    match mark {
        RecordMark::Waiting => paint_hourglass(painter, center, size, color),
        RecordMark::Received | RecordMark::Sent => {
            paint_arrow(painter, center, size, color, FLOW_ARROW_POINTS_LEFT)
        }
        RecordMark::Discarded => paint_down_arrow(painter, center, size, color),
        RecordMark::Idle => paint_zzz(painter, center, size, color),
        RecordMark::Done => paint_check(painter, center, done_draw_size(size), color),
        RecordMark::Holding => paint_stack(painter, center, size, color),
    }
}

fn done_draw_size(size: f32) -> f32 {
    (size * DONE_SCALE).max(DONE_MIN)
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

/// In (`→ N`) and out (`N →`) both point right: records flow left to right.
const FLOW_ARROW_POINTS_LEFT: bool = false;

fn paint_arrow(painter: &egui::Painter, c: Pos2, s: f32, color: Color32, point_left: bool) {
    let st = Stroke::new((s * 0.16).max(1.4_f32), color);
    let dir = if point_left { -1.0_f32 } else { 1.0_f32 };
    let x0 = c.x - s * 0.42 * dir;
    let x1 = c.x + s * 0.18 * dir;
    let tip = c.x + s * 0.46 * dir;
    painter.line_segment([Pos2::new(x0, c.y), Pos2::new(x1, c.y)], st);
    painter.line_segment(
        [
            Pos2::new(c.x + s * 0.04 * dir, c.y - s * 0.34),
            Pos2::new(tip, c.y),
        ],
        st,
    );
    painter.line_segment(
        [
            Pos2::new(c.x + s * 0.04 * dir, c.y + s * 0.34),
            Pos2::new(tip, c.y),
        ],
        st,
    );
}

/// Filled ✓ so the done mark stays a check at small zoom, not two hairlines.
fn paint_check(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let w = (s * 0.20).max(2.2_f32);
    let a = Pos2::new(c.x - s * 0.38, c.y + s * 0.06);
    let b = Pos2::new(c.x - s * 0.06, c.y + s * 0.40);
    let d = Pos2::new(c.x + s * 0.44, c.y - s * 0.38);
    paint_thick_segment(painter, a, b, w, color);
    paint_thick_segment(painter, b, d, w, color);
}

fn paint_thick_segment(painter: &egui::Painter, p0: Pos2, p1: Pos2, width: f32, color: Color32) {
    let delta = p1 - p0;
    let len = delta.length();
    if len < 0.5 {
        return;
    }
    let n = Vec2::new(-delta.y, delta.x) / len * (width * 0.5);
    painter.add(egui::Shape::convex_polygon(
        vec![p0 + n, p1 + n, p1 - n, p0 - n],
        color,
        Stroke::NONE,
    ));
}

/// Same stroke and span as `paint_arrow`, rotated to point down.
fn paint_down_arrow(painter: &egui::Painter, c: Pos2, s: f32, color: Color32) {
    let st = Stroke::new((s * 0.16).max(1.4_f32), color);
    let y0 = c.y - s * 0.42;
    let y1 = c.y + s * 0.18;
    let tip = c.y + s * 0.46;
    painter.line_segment([Pos2::new(c.x, y0), Pos2::new(c.x, y1)], st);
    painter.line_segment(
        [
            Pos2::new(c.x - s * 0.34, c.y + s * 0.04),
            Pos2::new(c.x, tip),
        ],
        st,
    );
    painter.line_segment(
        [
            Pos2::new(c.x + s * 0.34, c.y + s * 0.04),
            Pos2::new(c.x, tip),
        ],
        st,
    );
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
    let alloc = if mark == RecordMark::Done {
        done_draw_size(size)
    } else {
        size
    };
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(alloc), Sense::hover());
    let draw = if mark == RecordMark::Done {
        size
    } else {
        size * 0.92
    };
    paint_mark(ui.painter(), rect.center(), draw, mark);
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
        for mark in LEGEND_MARKS {
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
            RecordMark::Received,
            RecordMark::Sent,
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
        assert_eq!(
            LEGEND_MARKS,
            [
                RecordMark::Waiting,
                RecordMark::Idle,
                RecordMark::Received,
                RecordMark::Sent,
                RecordMark::Discarded,
                RecordMark::Holding,
                RecordMark::Done,
            ]
        );
        assert_eq!(mark_word(RecordMark::Waiting), "waiting");
        assert_eq!(mark_word(RecordMark::Received), "in");
        assert_eq!(mark_word(RecordMark::Sent), "out");
        assert_eq!(mark_word(RecordMark::Idle), "idle (waiting for input)");
        assert_eq!(mark_word(RecordMark::Done), "done");
        assert_eq!(mark_word(RecordMark::Discarded), "discarded");
        assert_eq!(mark_word(RecordMark::Holding), "buffering");
        assert!(mark_legend_tip(RecordMark::Waiting).contains("Hourglass"));
        assert!(mark_legend_tip(RecordMark::Received).contains("Arrow in"));
        assert!(mark_legend_tip(RecordMark::Received).contains("→ N"));
        assert!(mark_legend_tip(RecordMark::Received).contains("pointing right"));
        assert!(!mark_legend_tip(RecordMark::Received).contains('←'));
        assert!(mark_legend_tip(RecordMark::Sent).contains("Arrow out"));
        assert!(mark_legend_tip(RecordMark::Sent).contains("N →"));
        assert!(mark_legend_tip(RecordMark::Sent).contains("pointing right"));
        assert!(!mark_legend_tip(RecordMark::Sent).contains('←'));
        assert!(
            !FLOW_ARROW_POINTS_LEFT,
            "in badge must be → N and out badge N →, not a left-pointing in-arrow"
        );
        assert_eq!(
            heading_mark_place(RecordMark::Received),
            HeadingMarkPlace::Before,
            "grid heading must read → Received"
        );
        assert_eq!(
            heading_mark_place(RecordMark::Sent),
            HeadingMarkPlace::After,
            "grid heading must read Sent →"
        );
        assert_eq!(
            heading_mark_place(RecordMark::Discarded),
            HeadingMarkPlace::Before,
            "Discarded keeps ↓ before the word"
        );
        assert!(mark_legend_tip(RecordMark::Done).contains("Check"));
        assert!(mark_legend_tip(RecordMark::Done).contains("finished"));
        assert!(!mark_legend_tip(RecordMark::Done)
            .to_lowercase()
            .contains("thumbs"));
        assert!(!mark_legend_tip(RecordMark::Done).contains("passed"));
        assert!(mark_legend_tip(RecordMark::Discarded).starts_with("Down arrow:"));
        assert!(mark_legend_tip(RecordMark::Discarded).contains("↓ N"));
        assert!(mark_legend_tip(RecordMark::Discarded).contains("pointing down"));
        assert!(!mark_legend_tip(RecordMark::Discarded).contains('✕'));
        assert!(!mark_legend_tip(RecordMark::Discarded).contains('×'));
        assert_eq!(DISCARDED_GLYPH, '↓');
        assert_eq!(DISCARDED_GLYPH as u32, 0x2193);
        assert!(mark_legend_tip(RecordMark::Idle).contains("idle"));
        assert!(mark_legend_tip(RecordMark::Holding).contains("Stack"));
        assert!(mark_legend_tip(RecordMark::Holding).contains("buffering"));
        assert!(!mark_legend_tip(RecordMark::Holding).contains("holding"));
        assert_eq!(IDLE_GLYPH as u32, 0x1F4A4);
        assert_eq!(DONE_GLYPH, '✓');
        assert_eq!(DONE_GLYPH as u32, 0x2713);
        assert_eq!(stage_run_mark(true, false), RecordMark::Done);
        assert_eq!(stage_run_mark(true, true), RecordMark::Done);
        assert_eq!(stage_run_mark(false, true), RecordMark::Waiting);
        assert_eq!(stage_run_mark(false, false), RecordMark::Idle);
        assert_eq!(mark_for_waiting(WaitingKind::Empty), RecordMark::Idle);
        assert_eq!(
            mark_for_waiting(WaitingKind::WriterFinished),
            RecordMark::Done
        );
        assert_eq!(
            mark_for_observed(ObservedKind::WentOut),
            Some(RecordMark::Sent)
        );
        assert_eq!(mark_for_observed(ObservedKind::CameIn), None);
        assert_eq!(
            mark_for_observed(ObservedKind::Discarded),
            Some(RecordMark::Discarded)
        );
        assert_eq!(
            mark_for_observed(ObservedKind::InputEnded),
            Some(RecordMark::Done)
        );
        assert!(
            done_draw_size(8.25) >= DONE_MIN,
            "55% zoom badges stay at least {DONE_MIN}px"
        );
    }
}
