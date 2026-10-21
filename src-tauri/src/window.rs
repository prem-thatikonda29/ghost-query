//! Ghost-window behaviour: summon/dismiss with a global shortcut, without a Dock or
//! taskbar presence. Frameless / always-on-top / hidden-from-capture are set in
//! `tauri.conf.json` (guarded by ghost-core's `app_config` test); the capture flag is
//! also re-applied from user settings at startup.
//!
//! On macOS a normal window cannot appear over another app's fullscreen Space, so the
//! main window is converted into a non-activating `NSPanel` that joins all Spaces as a
//! fullscreen auxiliary window. Show/hide then have to go through the panel.

use tauri::{Runtime, WebviewWindow};

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

pub fn toggle<R: Runtime>(window: &WebviewWindow<R>) {
    if is_visible(window) {
        hide(window);
    } else {
        show(window);
    }
}
