mod conversation;
mod keystore;
mod provider;
mod providers;
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
struct ProviderView {
    id: String,
    name: String,
    base_url: String,
    url_editable: bool,
    needs_key: bool,
    has_key: bool,
    model: String,
    key_hint: String,
}

#[derive(Serialize)]
struct SettingsView {
    active: String,
    providers: Vec<ProviderView>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .map_err(|e| format!("No config directory: {e}"))
}

fn settings_view(settings: &Settings) -> SettingsView {
    SettingsView {
        active: settings.active.clone(),
        providers: providers::PRESETS
            .iter()
            .map(|p| {
                let config = settings.resolved(p.id).expect("preset exists");
                ProviderView {
                    id: p.id.to_string(),
                    name: p.name.to_string(),
                    base_url: config.base_url,
                    url_editable: p.id == providers::CUSTOM_ID,
                    needs_key: p.needs_key,
                    has_key: config.has_key,
                    model: config.model,
                    key_hint: p.key_hint.to_string(),
                }
            })
            .collect(),
    }
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<SettingsView, String> {
    Ok(settings_view(&Settings::load(&settings_path(&app)?)))
}

/// Saves a provider's model (and URL for the custom provider), stores a newly typed
/// key in the keychain, and makes the provider the active one.
#[tauri::command]
fn save_provider(
    app: AppHandle,
    id: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> Result<SettingsView, String> {
    let path = settings_path(&app)?;
    let mut settings = Settings::load(&path);
    settings.update_provider(&id, &base_url, &model)?;
    if let Some(key) = api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        keystore::set(&id, key)?;
        settings.set_has_key(&id, true);
    }
    settings.set_active(&id)?;
    settings.save(&path)?;
    Ok(settings_view(&settings))
}

#[tauri::command]
fn remove_api_key(app: AppHandle, id: String) -> Result<SettingsView, String> {
    let path = settings_path(&app)?;
    let mut settings = Settings::load(&path);
    keystore::clear(&id)?;
    settings.set_has_key(&id, false);
    settings.save(&path)?;
    Ok(settings_view(&settings))
}

/// Models offered by a provider. `base_url` is only honoured for the custom provider,
/// so the list can be fetched before its URL is saved.
#[tauri::command]
async fn list_models(app: AppHandle, id: String, base_url: String) -> Result<Vec<ModelInfo>, String> {
    let settings = Settings::load(&settings_path(&app)?);
    let config = settings.resolved(&id).ok_or_else(|| format!("Unknown provider: {id}"))?;
    let url = if id == providers::CUSTOM_ID { base_url.trim().to_string() } else { config.base_url };
    if url.is_empty() {
        return Ok(Vec::new());
    }
    // Only touch the keychain when a key is known to exist.
    let key = if config.has_key { keystore::get(&id)? } else { None };
    provider::fetch_models(&Client::new(), &url, key.as_deref()).await
}

#[tauri::command]
async fn ask_ai_stream(prompt: String, app: AppHandle, state: State<'_, ChatState>) -> Result<(), String> {
    let active = match Settings::load(&settings_path(&app)?).active() {
        Ok(active) => active,
        Err(message) => {
            let _ = app.emit("ai-response-error", message);
            return Ok(());
        }
    };
    let key = keystore::get(&active.id)?;
    if key.is_none() && active.needs_key {
        let _ = app.emit(
            "ai-response-error",
            format!("No API key saved for {}. Open Settings and add one.", active.name),
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

    let outcome = provider::stream_chat(&Client::new(), &active, key.as_deref(), &messages, &token, &app).await;

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
            save_provider,
            remove_api_key,
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

