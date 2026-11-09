//! Keep the graph record preview open while the pointer travels onto it.
//!
//! Pure geometry and timing so the retain path can be unit-tested without a
//! live window. The popup is placed against the stage/edge (no gap). A short
//! grace period plus a bridge rect cover Remote Desktop jitter and the path
//! from the anchor onto the popup, including left/above placements.

use std::time::{Duration, Instant};

use egui::{Pos2, Rect, Vec2};

/// Hover-open delay before the preview appears.
pub const OPEN_DELAY: Duration = Duration::from_millis(280);
/// Keep the unpinned preview after the pointer leaves, ~300–400ms.
pub const GRACE: Duration = Duration::from_millis(360);
/// Pad around the anchor, popup, and the corridor between them.
pub const BRIDGE_PAD: f32 = 14.0;
pub const PREVIEW_MAX: Vec2 = Vec2::new(480.0, 540.0);

#[derive(Clone, Copy, Debug)]
pub struct RetainInput {
    pub over_anchor: bool,
    pub over_popup: bool,
    pub over_bridge: bool,
    /// Scroll-bar drag: hover pos is often `None` while the button is down.
    pub pointer_down_in_popup: bool,
    pub last_inside: Option<Instant>,
    pub now: Instant,
    pub grace: Duration,
    pub pinned: bool,
}

/// Whether the preview should stay up, and whether `last_inside` should refresh.
pub fn keep_open(input: RetainInput) -> (bool, bool) {
    if input.pinned {
        return (true, inside(&input));
    }
    if inside(&input) {
        return (true, true);
    }
    if let Some(t) = input.last_inside {
        if input.now.saturating_duration_since(t) < input.grace {
            return (true, false);
        }
    }
    (false, false)
}

fn inside(input: &RetainInput) -> bool {
    input.over_anchor || input.over_popup || input.over_bridge || input.pointer_down_in_popup
}

pub fn pointer_in_rect(pos: Option<Pos2>, rect: Option<Rect>, pad: f32) -> bool {
    pos.zip(rect)
        .is_some_and(|(p, r)| r.expand(pad).contains(p))
}

/// Axis-aligned corridor covering the anchor, the popup, and the space the
/// pointer has to cross — including when the popup sits to the left or above.
pub fn hover_bridge(anchor: Rect, popup: Rect) -> Rect {
    let corridor = Rect::from_two_pos(anchor.center(), popup.center()).expand(BRIDGE_PAD);
    corridor
        .union(anchor.expand(BRIDGE_PAD))
        .union(popup.expand(BRIDGE_PAD))
}

/// Place `size` against `anchor` so they share an edge. Prefer right, then
/// left, then below, then above. Clamp to `screen` if nothing fully fits.
pub fn place_adjacent(anchor: Rect, size: Vec2, screen: Rect) -> Pos2 {
    let candidates = [
        Pos2::new(anchor.right(), anchor.top()),
        Pos2::new(anchor.left() - size.x, anchor.top()),
        Pos2::new(anchor.left(), anchor.bottom()),
        Pos2::new(anchor.left(), anchor.top() - size.y),
    ];
    let fits = |p: Pos2| fully_inside(Rect::from_min_size(p, size), screen);
    if let Some(p) = candidates.into_iter().find(|p| fits(*p)) {
        return p;
    }
    let mut best = candidates[0];
    let mut best_area = -1.0_f32;
    for p in candidates {
        let area = Rect::from_min_size(p, size).intersect(screen).area();
        if area > best_area {
            best_area = area;
            best = p;
        }
    }
    clamp_to_screen(Rect::from_min_size(best, size), screen).min
}

fn fully_inside(inner: Rect, outer: Rect) -> bool {
    inner.min.x >= outer.min.x
        && inner.min.y >= outer.min.y
        && inner.max.x <= outer.max.x + 0.01
        && inner.max.y <= outer.max.y + 0.01
}

fn clamp_to_screen(mut r: Rect, screen: Rect) -> Rect {
    if r.right() > screen.right() {
        r = r.translate(Vec2::new(screen.right() - r.right(), 0.0));
    }
    if r.left() < screen.left() {
        r = r.translate(Vec2::new(screen.left() - r.left(), 0.0));
    }
    if r.bottom() > screen.bottom() {
        r = r.translate(Vec2::new(0.0, screen.bottom() - r.bottom()));
    }
    if r.top() < screen.top() {
        r = r.translate(Vec2::new(0.0, screen.top() - r.top()));
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    fn retain(
        over_anchor: bool,
        over_popup: bool,
        over_bridge: bool,
        down: bool,
        last: Option<Instant>,
        now: Instant,
        pinned: bool,
    ) -> (bool, bool) {
        keep_open(RetainInput {
            over_anchor,
            over_popup,
            over_bridge,
            pointer_down_in_popup: down,
            last_inside: last,
            now,
            grace: GRACE,
            pinned,
        })
    }

    #[test]
    fn grace_is_between_300_and_400_ms() {
        assert!(GRACE >= Duration::from_millis(300));
        assert!(GRACE <= Duration::from_millis(400));
    }

    #[test]
    fn stays_open_on_anchor_popup_bridge_or_scroll_drag() {
        let now = Instant::now();
        assert!(retain(true, false, false, false, None, now, false).0);
        assert!(retain(false, true, false, false, None, now, false).0);
        assert!(retain(false, false, true, false, None, now, false).0);
        assert!(retain(false, false, false, true, None, now, false).0);
    }

    #[test]
    fn grace_keeps_unpinned_preview_then_drops() {
        let now = Instant::now();
        let inside = now - Duration::from_millis(200);
        assert!(retain(false, false, false, false, Some(inside), now, false).0);
        let stale = now - Duration::from_millis(500);
        assert!(!retain(false, false, false, false, Some(stale), now, false).0);
        assert!(!retain(false, false, false, false, None, now, false).0);
    }

    #[test]
    fn pinned_stays_without_pointer() {
        let now = Instant::now();
        let (keep, refresh) = retain(false, false, false, false, None, now, true);
        assert!(keep);
        assert!(!refresh);
    }

    #[test]
    fn right_placement_touches_anchor() {
        let screen = rect(0.0, 0.0, 800.0, 600.0);
        let anchor = rect(80.0, 90.0, 164.0, 56.0);
        let size = Vec2::new(320.0, 200.0);
        let p = place_adjacent(anchor, size, screen);
        assert_eq!(p.x, anchor.right());
        assert_eq!(p.y, anchor.top());
        let popup = Rect::from_min_size(p, size);
        assert!(
            (popup.left() - anchor.right()).abs() < 0.01,
            "gap on the right: popup={popup:?} anchor={anchor:?}"
        );
    }

    #[test]
    fn left_placement_when_the_right_edge_is_blocked() {
        let screen = rect(0.0, 0.0, 800.0, 600.0);
        let anchor = rect(500.0, 80.0, 164.0, 56.0);
        let size = Vec2::new(320.0, 200.0);
        let p = place_adjacent(anchor, size, screen);
        assert!((p.x + size.x - anchor.left()).abs() < 0.01, "left x={p:?}");
        assert_eq!(p.y, anchor.top());
        let popup = Rect::from_min_size(p, size);
        let mid = Pos2::new(anchor.left() - 4.0, anchor.top() + 20.0);
        assert!(
            hover_bridge(anchor, popup).contains(mid),
            "bridge must cover the left gap, mid={mid:?}"
        );
    }

    #[test]
    fn above_placement_when_the_bottom_is_blocked() {
        let screen = rect(0.0, 0.0, 800.0, 400.0);
        let anchor = rect(600.0, 250.0, 164.0, 56.0);
        let size = Vec2::new(200.0, 180.0);
        let p = place_adjacent(anchor, size, screen);
        assert!(
            (p.y + size.y - anchor.top()).abs() < 0.01,
            "should sit on top of the anchor, p={p:?}"
        );
        let popup = Rect::from_min_size(p, size);
        let mid = Pos2::new(anchor.left() + 20.0, anchor.top() - 4.0);
        assert!(
            hover_bridge(anchor, popup).contains(mid),
            "bridge must cover the above gap"
        );
    }

    #[test]
    fn pointer_in_rect_uses_pad() {
        let r = rect(10.0, 10.0, 20.0, 20.0);
        assert!(pointer_in_rect(Some(Pos2::new(8.0, 20.0)), Some(r), 4.0));
        assert!(!pointer_in_rect(Some(Pos2::new(8.0, 20.0)), Some(r), 0.0));
        assert!(!pointer_in_rect(None, Some(r), 4.0));
    }
}
