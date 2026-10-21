//! Where the ghost window sits. It has two sizes, a Spotlight-like bar and a chat
//! panel; switching between them keeps the window's top edge and horizontal centre
//! (its "anchor") so the panel grows down out of the bar. All values are logical
//! pixels with a top-left origin.

use serde::{Deserialize, Serialize};

pub const BAR_SIZE: (f64, f64) = (680.0, 56.0);
pub const PANEL_SIZE: (f64, f64) = (800.0, 600.0);
/// The panel can be resized, but not below this.
pub const PANEL_MIN_SIZE: (f64, f64) = (480.0, 320.0);

/// The point a window is placed by: the middle of its top edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub center_x: f64,
    pub top: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn anchor(&self) -> Anchor {
        Anchor { center_x: self.x + self.width / 2.0, top: self.y }
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// First-run spot: centred, a quarter of the way down the screen, so the panel
/// still fits below the bar on a laptop display.
pub fn default_anchor(work_area: Rect) -> Anchor {
    Anchor {
        center_x: work_area.x + work_area.width / 2.0,
        top: work_area.y + (work_area.height / 4.0).round(),
    }
}

/// The screen a saved anchor is on. A monitor may have been unplugged since, and then
/// the anchor is not reused.
pub fn screen_for(anchor: Anchor, work_areas: &[Rect]) -> Option<Rect> {
    work_areas.iter().copied().find(|area| area.contains(anchor.center_x, anchor.top))
}

/// The frame of a window of `size` placed at `anchor`, shrunk and moved as needed to
/// stay inside `work_area` (for example a panel growing past the bottom of the screen
/// moves up).
pub fn place(anchor: Anchor, size: (f64, f64), work_area: Rect) -> Rect {
    let width = size.0.min(work_area.width);
    let height = size.1.min(work_area.height);
    let max_x = work_area.x + work_area.width - width;
    let max_y = work_area.y + work_area.height - height;
    Rect {
        x: (anchor.center_x - width / 2.0).round().clamp(work_area.x, max_x),
        y: anchor.top.clamp(work_area.y, max_y),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0.0, y: 25.0, width: 1440.0, height: 875.0 };

    #[test]
    fn first_run_bar_is_centred_in_the_upper_quarter() {
        let bar = place(default_anchor(SCREEN), BAR_SIZE, SCREEN);
        assert_eq!(bar, Rect { x: 380.0, y: 244.0, width: 680.0, height: 56.0 });
    }

    #[test]
    fn panel_grows_down_from_the_bar_and_collapses_back() {
        let bar = place(default_anchor(SCREEN), BAR_SIZE, SCREEN);
        let panel = place(bar.anchor(), PANEL_SIZE, SCREEN);
        assert_eq!((panel.x, panel.y), (320.0, bar.y));
        assert_eq!(place(panel.anchor(), BAR_SIZE, SCREEN), bar);
    }

    #[test]
    fn a_panel_that_would_leave_the_screen_is_moved_back_on() {
        let low = Anchor { center_x: 1400.0, top: 600.0 };
        let panel = place(low, PANEL_SIZE, SCREEN);
        assert_eq!(panel, Rect { x: 640.0, y: 300.0, width: 800.0, height: 600.0 });
    }

    #[test]
    fn a_window_larger_than_the_screen_is_shrunk_to_fit() {
        let tiny = Rect { x: 0.0, y: 0.0, width: 640.0, height: 480.0 };
        let panel = place(default_anchor(tiny), PANEL_SIZE, tiny);
        assert_eq!(panel, Rect { x: 0.0, y: 0.0, width: 640.0, height: 480.0 });
    }

    #[test]
    fn saved_anchors_on_a_missing_monitor_are_ignored() {
        let second = Rect { x: 1440.0, y: 0.0, width: 1920.0, height: 1080.0 };
        let on_second = Anchor { center_x: 2000.0, top: 300.0 };
        assert_eq!(screen_for(on_second, &[SCREEN, second]), Some(second));
        assert_eq!(screen_for(on_second, &[SCREEN]), None);
    }
}
