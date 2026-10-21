//! Guards ghost-window settings that live in the app's config files rather than code.

#[test]
fn config_makes_the_window_a_hidden_frameless_always_on_top_panel() {
    let conf: serde_json::Value =
        serde_json::from_str(include_str!("../../src-tauri/tauri.conf.json")).unwrap();
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
    let caps = include_str!("../../src-tauri/capabilities/default.json");
    assert!(caps.contains("core:window:allow-start-dragging"));
}

#[test]
fn release_builds_build_the_frontend_first() {
    let conf: serde_json::Value =
        serde_json::from_str(include_str!("../../src-tauri/tauri.conf.json")).unwrap();
    assert!(conf["build"]["beforeBuildCommand"].as_str().unwrap().contains("build"));
}
