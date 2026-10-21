//! Everything testable without a GUI. The Tauri app crate (`src-tauri`) is a thin
//! shell over this: commands, window and shortcut handling live there.

pub mod conversation;
pub mod db;
pub mod debounce;
pub mod keystore;
pub mod provider;
pub mod providers;
pub mod settings;
pub mod sse;
