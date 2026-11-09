use egui::{style::ScrollStyle, Color32, CornerRadius, Stroke, Ui, Visuals};

pub const BG: Color32 = Color32::from_rgb(14, 17, 22);
pub const PANEL: Color32 = Color32::from_rgb(22, 27, 34);
pub const PANEL_ALT: Color32 = Color32::from_rgb(28, 34, 43);
pub const BORDER: Color32 = Color32::from_rgb(48, 54, 61);
pub const TEXT: Color32 = Color32::from_rgb(230, 237, 243);
pub const MUTED: Color32 = Color32::from_rgb(139, 148, 158);
pub const ACCENT: Color32 = Color32::from_rgb(56, 166, 255);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(31, 111, 184);
pub const SELECT: Color32 = Color32::from_rgb(240, 193, 74);
pub const SELECT_BG: Color32 = Color32::from_rgb(64, 52, 20);
pub const SECONDARY: Color32 = Color32::from_rgb(188, 140, 255);
pub const ERROR: Color32 = Color32::from_rgb(248, 81, 73);
pub const OK: Color32 = Color32::from_rgb(63, 185, 80);
pub const WARN: Color32 = Color32::from_rgb(210, 153, 34);
pub const NODE: Color32 = Color32::from_rgb(33, 41, 52);
pub const NODE_SCRIPT: Color32 = Color32::from_rgb(28, 48, 62);
pub const NODE_UNKNOWN: Color32 = Color32::from_rgb(72, 32, 28);

/// Solid ~6px bars that reserve their own column so they never cover cells.
pub fn compact_scroll() -> ScrollStyle {
    let mut s = ScrollStyle::solid();
    s.bar_width = 6.0;
    s.bar_inner_margin = 0.0;
    s.bar_outer_margin = 0.0;
    s.handle_min_length = 18.0;
    s.foreground_color = true;
    s
}

pub fn apply_compact_scroll(ui: &mut Ui) {
    ui.spacing_mut().scroll = compact_scroll();
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = PANEL_ALT;
    v.override_text_color = Some(TEXT);
    v.widgets.inactive.bg_fill = PANEL_ALT;
    v.widgets.hovered.bg_fill = Color32::from_rgb(40, 48, 60);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    ctx.set_visuals(v);
    ctx.style_mut(|style| {
        style.spacing.scroll = compact_scroll();
    });
    ctx.set_pixels_per_point(ctx.pixels_per_point().clamp(1.0, 2.5));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_scroll_is_thin_and_solid() {
        let s = compact_scroll();
        assert!(!s.floating, "solid bars must reserve space beside cells");
        assert!(s.bar_width > 0.0 && s.bar_width <= 6.0, "{}", s.bar_width);
        assert_eq!(s.bar_inner_margin, 0.0);
        assert!(s.handle_min_length >= 16.0);
        let allocated = if s.floating {
            s.floating_allocated_width
        } else {
            s.bar_width + s.bar_inner_margin + s.bar_outer_margin
        };
        assert!(
            allocated > 0.0 && allocated <= 6.0,
            "reserved bar column {allocated}"
        );
    }
}
