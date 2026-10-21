mod conversation;
mod keystore;
mod provider;
mod settings;
mod sse;

use conversation::{ChatMessage, Conversation, ConversationMessage};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use provider::{ModelInfo, Outcome};
use reqwest::Client;
use serde::Serialize;
use settings::Settings;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

const SYSTEM_PROMPT: &str = "You are Ghost Query, a concise assistant. Answer directly. \
Use Markdown for structure and fenced code blocks for code.";

#[derive(Default)]
struct ChatState {
    conversation: Mutex<Conversation>,
    /// Token of the in-flight request, tagged with its id so a finished request
    /// never clears the token of a newer one.
    active: Mutex<Option<(u64, CancellationToken)>>,
    next_id: AtomicU64,
}

#[derive(Serialize)]
struct SettingsView {
    base_url: String,
    model: String,
    has_key: bool,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .map_err(|e| format!("No config directory: {e}"))
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<SettingsView, String> {
    let settings = Settings::load(&settings_path(&app)?);
    Ok(SettingsView {
        base_url: settings.base_url,
        model: settings.model,
        has_key: keystore::get()?.is_some(),
    })
}

#[tauri::command]
fn save_settings(app: AppHandle, base_url: String, model: String) -> Result<(), String> {
    let settings = Settings { base_url, model }.normalized();
    settings.validate()?;
    settings.save(&settings_path(&app)?)
}

#[tauri::command]
fn set_api_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API key must not be empty".to_string());
    }
    keystore::set(key)
}

#[tauri::command]
fn clear_api_key() -> Result<(), String> {
    keystore::clear()
}

#[tauri::command]
async fn list_models(app: AppHandle) -> Result<Vec<ModelInfo>, String> {
    let settings = Settings::load(&settings_path(&app)?).normalized();
    let key = keystore::get()?;
    provider::fetch_models(&Client::new(), &settings.base_url, key.as_deref()).await
}

#[tauri::command]
async fn ask_ai_stream(prompt: String, app: AppHandle, state: State<'_, ChatState>) -> Result<(), String> {
    let settings = Settings::load(&settings_path(&app)?).normalized();
    let key = keystore::get()?;
    if key.is_none() && provider::is_openrouter(&settings.base_url) {
        let _ = app.emit(
            "ai-response-error",
            "No API key set. Open Settings and add your OpenRouter key (free to create at openrouter.ai).",
        );
        return Ok(());
    }

    // History is only read here; it is written once the exchange has finished.
    let mut messages = vec![ChatMessage { role: "system".into(), content: SYSTEM_PROMPT.into() }];
    messages.extend(state.conversation.lock().unwrap().history_for_request(&prompt));

    let token = CancellationToken::new();
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    if let Some((_, previous)) = state.active.lock().unwrap().replace((id, token.clone())) {
        previous.cancel();
    }

    let outcome = provider::stream_chat(&Client::new(), &settings, key.as_deref(), &messages, &token, &app).await;

    {
        let mut active = state.active.lock().unwrap();
        if matches!(&*active, Some((current, _)) if *current == id) {
            *active = None;
        }
    }

    match outcome {
        Outcome::Done(full) => {
            state.conversation.lock().unwrap().commit_exchange(&prompt, &full);
            let _ = app.emit("ai-response-done", &full);
        }
        Outcome::Cancelled(partial) => {
            state.conversation.lock().unwrap().commit_exchange(&prompt, &partial);
            let _ = app.emit("ai-response-cancelled", &partial);
        }
        Outcome::Failed(message) => {
            let _ = app.emit("ai-response-error", &message);
        }
    }
    Ok(())
}

#[tauri::command]
fn get_conversation_history(state: State<'_, ChatState>) -> Result<Vec<ConversationMessage>, String> {
    Ok(state.conversation.lock().unwrap().messages())
}

#[tauri::command]
fn clear_conversation(state: State<'_, ChatState>) -> Result<(), String> {
    state.conversation.lock().unwrap().clear();
    Ok(())
}

#[tauri::command]
fn stop_streaming(state: State<'_, ChatState>) -> Result<(), String> {
    if let Some((_, token)) = state.active.lock().unwrap().as_ref() {
        token.cancel();
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The hotkey manager must be created before the app starts and kept alive.
    // Phase 2 replaces this with tauri-plugin-global-shortcut.
    let manager = GlobalHotKeyManager::new().unwrap();
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    manager.register(hotkey).unwrap();

    tauri::Builder::default()
        .manage(ChatState::default())
        .invoke_handler(tauri::generate_handler![
            ask_ai_stream,
            get_conversation_history,
            clear_conversation,
            stop_streaming,
            get_settings,
            save_settings,
            set_api_key,
            clear_api_key,
            list_models
        ])
        .setup(move |app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let window = app.get_webview_window("main").unwrap();
            window.hide().unwrap();

            // No Dock icon. The window-style tweaks that used to live here made it a
            // regular titled window; the real ghost-panel setup arrives in Phase 2.
            #[cfg(target_os = "macos")]
            {
                let _ = app.app_handle().set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            let main_window = window.clone();
            std::thread::spawn(move || {
                let mut last_toggle = std::time::Instant::now();
                for event in GlobalHotKeyEvent::receiver().iter() {
                    if event.id != hotkey.id() {
                        continue;
                    }
                    // Debounce: only allow toggling every 200ms
                    if last_toggle.elapsed().as_millis() < 200 {
                        continue;
                    }
                    last_toggle = std::time::Instant::now();

                    if main_window.is_visible().unwrap_or(false) {
                        let _ = main_window.hide();
                    } else {
                        let _ = main_window.center();
                        let _ = main_window.show();
                        let _ = main_window.unminimize();
                        let _ = main_window.set_focus();
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    drop(manager);
}

