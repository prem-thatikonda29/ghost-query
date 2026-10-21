mod window;

use ghost_core::conversation::{build_request_messages, HISTORY_TOKEN_BUDGET};
use ghost_core::db::{ConversationSummary, Db, StoredMessage};
use ghost_core::debounce::Debounce;
use ghost_core::provider::{ModelInfo, Outcome};
use ghost_core::settings::Settings;
use ghost_core::{keystore, provider, providers};
use reqwest::Client;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};
use tokio_util::sync::CancellationToken;

const SYSTEM_PROMPT: &str = "You are Ghost Query, a concise assistant. Answer directly. \
Use Markdown for structure and fenced code blocks for code.";

struct ChatState {
    db: Db,
    /// Token of the in-flight request, tagged with its id so a finished request
    /// never clears the token of a newer one.
    active: Mutex<Option<(u64, CancellationToken)>>,
    next_id: AtomicU64,
}

impl ChatState {
    fn new(db: Db) -> Self {
        Self { db, active: Mutex::new(None), next_id: AtomicU64::new(0) }
    }
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
    hide_from_capture: bool,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .map_err(|e| format!("No config directory: {e}"))
}

fn settings_view(settings: &Settings) -> SettingsView {
    SettingsView {
        hide_from_capture: settings.hide_from_capture,
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
fn list_conversations(state: State<'_, ChatState>) -> Result<Vec<ConversationSummary>, String> {
    state.db.list_conversations()
}

#[tauri::command]
fn create_conversation(state: State<'_, ChatState>) -> Result<ConversationSummary, String> {
    state.db.create_conversation()
}

#[tauri::command]
fn rename_conversation(state: State<'_, ChatState>, conversation_id: String, title: String) -> Result<(), String> {
    state.db.rename_conversation(&conversation_id, &title)
}

#[tauri::command]
fn delete_conversation(state: State<'_, ChatState>, conversation_id: String) -> Result<(), String> {
    state.db.delete_conversation(&conversation_id)
}

#[tauri::command]
fn get_messages(state: State<'_, ChatState>, conversation_id: String) -> Result<Vec<StoredMessage>, String> {
    state.db.messages(&conversation_id)
}

/// Sends `prompt` in a conversation (or, with `regenerate`, re-asks its last user
/// message). Nothing is written until the reply finishes or is stopped, so a failed
/// request leaves the conversation exactly as it was.
#[tauri::command]
async fn ask_ai_stream(
    conversation_id: String,
    prompt: String,
    regenerate: bool,
    app: AppHandle,
    state: State<'_, ChatState>,
) -> Result<(), String> {
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

    let (prompt, history) = if regenerate {
        let Some(last) = state.db.last_user_message(&conversation_id)? else {
            let _ = app.emit("ai-response-error", "Nothing to regenerate.");
            return Ok(());
        };
        (last, state.db.history(&conversation_id, true)?)
    } else {
        (prompt, state.db.history(&conversation_id, false)?)
    };
    let messages = build_request_messages(SYSTEM_PROMPT, history, &prompt, HISTORY_TOKEN_BUDGET);

    let token = CancellationToken::new();
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    if let Some((_, previous)) = state.active.lock().unwrap().replace((id, token.clone())) {
        previous.cancel();
    }

    let outcome = provider::stream_chat(
        &Client::new(),
        &active,
        key.as_deref(),
        &messages,
        &token,
        &|text| {
            let _ = app.emit("ai-response-chunk", text);
        },
    )
    .await;

    {
        let mut slot = state.active.lock().unwrap();
        if matches!(&*slot, Some((current, _)) if *current == id) {
            *slot = None;
        }
    }

    let save = |text: &str, status: &str| state.db.commit_exchange(&conversation_id, &prompt, text, status, regenerate);
    match outcome {
        Outcome::Done(full) if full.trim().is_empty() => {
            let _ = app.emit("ai-response-error", "The model returned an empty reply.");
        }
        Outcome::Done(full) => match save(&full, "complete") {
            Ok(()) => {
                let _ = app.emit("ai-response-done", &full);
            }
            Err(e) => {
                let _ = app.emit("ai-response-error", e);
            }
        },
        // Stopped before any text arrived: nothing worth saving, the prompt stays in the input box.
        Outcome::Cancelled(partial) if partial.is_empty() => {
            let _ = app.emit("ai-response-cancelled", "");
        }
        Outcome::Cancelled(partial) => match save(&partial, "cancelled") {
            Ok(()) => {
                let _ = app.emit("ai-response-cancelled", &partial);
            }
            Err(e) => {
                let _ = app.emit("ai-response-error", e);
            }
        },
        Outcome::Failed(message) => {
            let _ = app.emit("ai-response-error", &message);
        }
    }
    Ok(())
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}

/// With no Dock icon or taskbar entry, the app needs its own way to quit.
#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn set_hide_from_capture(app: AppHandle, window: WebviewWindow, enabled: bool) -> Result<SettingsView, String> {
    let path = settings_path(&app)?;
    let mut settings = Settings::load(&path);
    settings.hide_from_capture = enabled;
    settings.save(&path)?;
    window.set_content_protected(enabled).map_err(|e| e.to_string())?;
    Ok(settings_view(&settings))
}

/// Set when the global shortcut could not be registered (for example another app
/// owns it), so the UI can say why the hotkey does nothing.
#[derive(Default)]
struct HotkeyStatus(Mutex<Option<String>>);

#[tauri::command]
fn get_hotkey_status(status: State<'_, HotkeyStatus>) -> Option<String> {
    status.0.lock().unwrap().clone()
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
    let debounce = std::sync::Arc::new(Mutex::new(Debounce::new()));

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state != ShortcutState::Pressed
                        || !shortcut.matches(Modifiers::CONTROL | Modifiers::SHIFT, Code::Space)
                    {
                        return;
                    }
                    if !debounce.lock().unwrap().allow(std::time::Instant::now()) {
                        return;
                    }
                    if let Some(main) = app.get_webview_window("main") {
                        window::toggle(&main);
                    }
                })
                .build(),
        )
        .manage(HotkeyStatus::default())
        .invoke_handler(tauri::generate_handler![
            ask_ai_stream,
            list_conversations,
            create_conversation,
            rename_conversation,
            delete_conversation,
            get_messages,
            hide_window,
            quit_app,
            set_hide_from_capture,
            get_hotkey_status,
            stop_streaming,
            get_settings,
            save_provider,
            remove_api_key,
            list_models
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let data_dir = app.path().app_data_dir()?;
            app.manage(ChatState::new(Db::open(&data_dir.join("ghost-query.db"))?));

            let window = app.get_webview_window("main").unwrap();

            // The config protects the window by default; honour the user's choice.
            let settings = Settings::load(&settings_path(app.handle())?);
            let _ = window.set_content_protected(settings.hide_from_capture);

            // No Dock icon.
            #[cfg(target_os = "macos")]
            {
                let _ = app.app_handle().set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // Ctrl+Shift+Space summons/dismisses the window. If it cannot be registered the
            // app would be invisible and unreachable, so show the window and record why.
            if let Err(e) = app.global_shortcut().register("ctrl+shift+space") {
                let message = format!("Ctrl+Shift+Space could not be registered ({e}). Another app may be using it.");
                log::warn!("{message}");
                *app.state::<HotkeyStatus>().0.lock().unwrap() = Some(message);
                window::show(&window);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
