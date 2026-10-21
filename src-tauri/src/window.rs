//! Ghost-window behaviour: summon/dismiss with a global shortcut, without a Dock or
//! taskbar presence. Frameless / always-on-top / hidden-from-capture are set in
//! `tauri.conf.json` (guarded by ghost-core's `app_config` test); the capture flag is
//! also re-applied from user settings at startup.
//!
//! On macOS a normal window cannot appear over another app's fullscreen Space, so the
//! main window is converted into a non-activating `NSPanel` that joins all Spaces as a
//! fullscreen auxiliary window. Show/hide then have to go through the panel.
//!
//! The window is either a Spotlight-like bar or the chat panel; see
//! `ghost_core::geometry` for how one turns into the other.

use ghost_core::geometry::{self, Rect, BAR_SIZE, PANEL_MIN_SIZE, PANEL_SIZE};
use tauri::{LogicalSize, Monitor, Runtime, WebviewWindow};

#[cfg(target_os = "macos")]
tauri_nspanel::tauri_panel! {
    panel!(GhostPanel {
        config: {
            // Without this a borderless panel could not receive keystrokes.
            can_become_key_window: true,
            is_floating_panel: true
        }
    })
}

/// Turns the main window into the ghost panel. Call once at startup (macOS only).
#[cfg(target_os = "macos")]
/// The generated panel class is implemented for the real (wry) runtime only.
pub fn convert_to_panel(window: &WebviewWindow<tauri::Wry>) -> tauri::Result<()> {
    use tauri_nspanel::{CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt};

    let panel = window.to_panel::<GhostPanel>()?;
    // Above normal and floating windows, like a system overlay.
    panel.set_level(PanelLevel::Status.value());
    // Follow the user into every Space, including another app's fullscreen Space.
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .into(),
    );
    // Taking focus must not activate the app, or macOS would switch away from the
    // fullscreen Space the user is in.
    let _ = panel.add_style_mask(StyleMask::empty().nonactivating_panel().value());
    // Stay visible when another app is in front; the shortcut/Esc dismiss it.
    panel.set_hides_on_deactivate(false);
    Ok(())
}

#[cfg(target_os = "macos")]
fn panel_of<R: Runtime>(window: &WebviewWindow<R>) -> Option<tauri_nspanel::PanelHandle<R>> {
    use tauri::Manager;
    use tauri_nspanel::ManagerExt;
    window.app_handle().get_webview_panel(window.label()).ok()
}

pub fn is_visible<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    #[cfg(target_os = "macos")]
    if let Some(panel) = panel_of(window) {
        return panel.is_visible();
    }
    window.is_visible().unwrap_or(false)
}

pub fn show<R: Runtime>(window: &WebviewWindow<R>) {
    #[cfg(target_os = "macos")]
    if let Some(panel) = panel_of(window) {
        panel.show_and_make_key();
        return;
    }
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

pub fn hide<R: Runtime>(window: &WebviewWindow<R>) {
    #[cfg(target_os = "macos")]
    if let Some(panel) = panel_of(window) {
        panel.hide();
        return;
    }
    let _ = window.hide();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Bar,
    Panel,
}

impl Mode {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "bar" => Some(Self::Bar),
            "panel" => Some(Self::Panel),
            _ => None,
        }
    }
}

/// The window's outer frame in logical pixels.
pub fn frame<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<Rect> {
    let scale = window.scale_factor()?;
    let position = window.outer_position()?.to_logical::<f64>(scale);
    let size = window.outer_size()?.to_logical::<f64>(scale);
    Ok(Rect { x: position.x, y: position.y, width: size.width, height: size.height })
}

/// A monitor's usable area (without the menu bar, Dock or taskbar) in logical pixels.
pub fn work_area(monitor: &Monitor) -> Rect {
    let area = monitor.work_area();
    let position = area.position.to_logical::<f64>(monitor.scale_factor());
    let size = area.size.to_logical::<f64>(monitor.scale_factor());
    Rect { x: position.x, y: position.y, width: size.width, height: size.height }
}

/// Resizes the window into the bar or the panel, keeping its top edge and centre. Only
/// the panel can be resized by the user.
pub fn set_mode<R: Runtime>(window: &WebviewWindow<R>, mode: Mode, animate: bool) -> tauri::Result<()> {
    let size = match mode {
        Mode::Bar => BAR_SIZE,
        Mode::Panel => PANEL_SIZE,
    };
    let anchor = frame(window)?.anchor();
    let target = match window.current_monitor()? {
        Some(monitor) => geometry::place(anchor, size, work_area(&monitor)),
        None => Rect { x: anchor.center_x - size.0 / 2.0, y: anchor.top, width: size.0, height: size.1 },
    };
    window.set_min_size(None::<LogicalSize<f64>>)?;
    window.set_resizable(mode == Mode::Panel)?;
    set_frame(window, target, animate)?;
    if mode == Mode::Panel {
        window.set_min_size(Some(LogicalSize::new(PANEL_MIN_SIZE.0, PANEL_MIN_SIZE.1)))?;
    }
    Ok(())
}

/// Moves and resizes the window in one step and returns once it is done, so the
/// caller can draw into the final size. On macOS this goes through AppKit, which can
/// animate the change (AppKit returns when the animation ends); elsewhere it is
/// immediate.
pub fn set_frame<R: Runtime>(window: &WebviewWindow<R>, target: Rect, animate: bool) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSWindow;
        use objc2_foundation::{NSPoint, NSRect, NSSize};

        let current = frame(window)?;
        // A raw pointer is not Send; the address is only turned back into a reference
        // on the main thread, where AppKit requires it to be used.
        let ns_window = window.ns_window()? as usize;
        let apply = move || {
            // SAFETY: the address is this window's live NSWindow, which outlives the call.
            let ns_window = unsafe { &*(ns_window as *const NSWindow) };
            let old = ns_window.frame();
            // AppKit's origin is the bottom-left corner with y pointing up, so the change
            // is applied relative to the current frame rather than converted.
            let top = old.origin.y + old.size.height - (target.y - current.y);
            let rect = NSRect::new(
                NSPoint::new(old.origin.x + (target.x - current.x), top - target.height),
                NSSize::new(target.width, target.height),
            );
            ns_window.setFrame_display_animate(rect, true, animate);
        };
        if MainThreadMarker::new().is_some() {
            apply();
            return Ok(());
        }
        // Off the main thread: hand the change over and wait for it, which cannot
        // deadlock because this thread is not the one doing the work.
        let (done, finished) = std::sync::mpsc::channel();
        window.run_on_main_thread(move || {
            apply();
            let _ = done.send(());
        })?;
        let _ = finished.recv();
        return Ok(());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = animate;
        window.set_size(LogicalSize::new(target.width, target.height))?;
        window.set_position(tauri::LogicalPosition::new(target.x, target.y))
    }
}
