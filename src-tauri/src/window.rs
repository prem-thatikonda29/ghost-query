//! Ghost-window behaviour: summon/dismiss with a global shortcut, without a Dock or
//! taskbar presence. Frameless / always-on-top / hidden-from-capture are set in
//! `tauri.conf.json` (guarded by ghost-core's `app_config` test); the capture flag is
//! also re-applied from user settings at startup.

use tauri::{Runtime, WebviewWindow};

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
