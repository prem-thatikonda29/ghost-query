//! Ghost-window behaviour: summon/dismiss with a global shortcut, without a Dock or
//! taskbar presence. Frameless / always-on-top / hidden-from-capture are set in
//! `tauri.conf.json`; the capture flag is also re-applied from user settings at startup.

use std::time::{Duration, Instant};
use tauri::{Runtime, WebviewWindow};

const DEBOUNCE: Duration = Duration::from_millis(200);

/// Ignores toggles that arrive too soon after the previous one (key repeat).
pub struct Debounce {
    last: Option<Instant>,
}

impl Debounce {
    pub fn new() -> Self {
        Self { last: None }
    }

    pub fn allow(&mut self, now: Instant) -> bool {
        match self.last {
            Some(previous) if now.duration_since(previous) < DEBOUNCE => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

pub fn show<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

pub fn toggle<R: Runtime>(window: &WebviewWindow<R>) {
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show(window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_blocks_rapid_repeats_but_allows_later_presses() {
        let mut d = Debounce::new();
        let t0 = Instant::now();
        assert!(d.allow(t0));
        assert!(!d.allow(t0 + Duration::from_millis(50)));
        assert!(!d.allow(t0 + Duration::from_millis(199)));
        assert!(d.allow(t0 + Duration::from_millis(250)));
        // The window restarts from the last accepted press.
        assert!(!d.allow(t0 + Duration::from_millis(300)));
    }

    /// Guards the ghost-window settings that live in config rather than code.
    #[test]
    fn config_makes_the_window_a_hidden_frameless_always_on_top_panel() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let w = &conf["app"]["windows"][0];
        assert_eq!(w["decorations"], false);
        assert_eq!(w["alwaysOnTop"], true);
        assert_eq!(w["skipTaskbar"], true);
        assert_eq!(w["contentProtected"], true);
        assert_eq!(w["visible"], false);
        assert_eq!(w["visibleOnAllWorkspaces"], true);
    }

    #[test]
    fn frameless_window_may_be_dragged_by_its_header() {
        let caps = include_str!("../capabilities/default.json");
        assert!(caps.contains("core:window:allow-start-dragging"));
    }
}
