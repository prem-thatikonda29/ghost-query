mod window;

use ghost_core::conversation::{build_request_messages, HISTORY_TOKEN_BUDGET};
use ghost_core::db::{ConversationSummary, Db, StoredMessage};
use ghost_core::debounce::Debounce;
use ghost_core::provider::{ModelInfo, Outcome};
use ghost_core::settings::Settings;
use ghost_core::keystore::KeyStore;
use ghost_core::{provider, providers};
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
    /// One client for all requests, so connections and TLS sessions are reused.
    client: Client,
    /// API keys, read from the OS keychain at most once per launch (each read can make
    /// macOS ask for the user's password).
    keys: KeyStore,
    /// Token of the in-flight request, tagged with its id so a finished request
    /// never clears the token of a newer one.
    active: Mutex<Option<(u64, CancellationToken)>>,
    next_id: AtomicU64,
}

impl ChatState {
    fn new(db: Db) -> Self {
        // Only the connect phase is bounded; a streaming reply can legitimately take
        // minutes, and the user ends it with Stop.
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { db, client, keys: KeyStore::new(), active: Mutex::new(None), next_id: AtomicU64::new(0) }
    }
}

/// Registers the in-flight request's cancellation token and clears it on drop, however
/// the request ends (including early returns), without clearing a newer request's token.
struct ActiveRequest<'a> {
    state: &'a ChatState,
    id: u64,
    token: CancellationToken,
}

impl<'a> ActiveRequest<'a> {
    fn register(state: &'a ChatState) -> Self {
        let token = CancellationToken::new();
        let id = state.next_id.fetch_add(1, Ordering::Relaxed);
        if let Some((_, previous)) = state.active.lock().unwrap().replace((id, token.clone())) {
            previous.cancel();
        }
        Self { state, id, token }
    }
}

impl Drop for ActiveRequest<'_> {
    fn drop(&mut self) {
        let mut slot = self.state.active.lock().unwrap();
        if matches!(&*slot, Some((current, _)) if *current == self.id) {
            *slot = None;
        }
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
    state: State<'_, ChatState>,
    id: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> Result<SettingsView, String> {
    let path = settings_path(&app)?;
    let mut settings = Settings::load(&path);
    settings.update_provider(&id, &base_url, &model)?;
    if let Some(key) = api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        state.keys.set(&id, key)?;
        settings.set_has_key(&id, true);
    }
    settings.set_active(&id)?;
    settings.save(&path)?;
    Ok(settings_view(&settings))
}

#[tauri::command]
fn remove_api_key(app: AppHandle, state: State<'_, ChatState>, id: String) -> Result<SettingsView, String> {
    let path = settings_path(&app)?;
    let mut settings = Settings::load(&path);
    state.keys.clear(&id)?;
    settings.set_has_key(&id, false);
    settings.save(&path)?;
    Ok(settings_view(&settings))
}

/// Models offered by a provider. `base_url` is only honoured for the custom provider,
/// so the list can be fetched before its URL is saved.
#[tauri::command]
async fn list_models(
    app: AppHandle,
    state: State<'_, ChatState>,
    id: String,
    base_url: String,
) -> Result<Vec<ModelInfo>, String> {
    let settings = Settings::load(&settings_path(&app)?);
    let config = settings.resolved(&id).ok_or_else(|| format!("Unknown provider: {id}"))?;
    let url = if id == providers::CUSTOM_ID {
        base_url.trim().trim_end_matches('/').to_string()
    } else {
        config.base_url.clone()
    };
    if url.is_empty() {
        return Ok(Vec::new());
    }
    // The saved key only ever goes to the saved URL: a URL still being typed in the form
    // is queried without credentials. Also avoids touching the keychain needlessly.
    let key = if config.has_key && url == config.base_url { state.keys.get(&id)? } else { None };
    provider::fetch_models(&state.client, &url, key.as_deref()).await
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
    // Registered first so Stop works during the setup below (keychain prompts, DB reads).
    let request = ActiveRequest::register(&state);

    let active = match Settings::load(&settings_path(&app)?).active() {
        Ok(active) => active,
        Err(message) => {
            let _ = app.emit("ai-response-error", message);
            return Ok(());
        }
    };
    let key = state.keys.get(&active.id)?;
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

    let outcome = provider::stream_chat(
        &state.client,
        &active,
        key.as_deref(),
        &messages,
        &request.token,
        &|text| {
            let _ = app.emit("ai-response-chunk", text);
        },
    )
    .await;
    drop(request);

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
        // Stopped by the user, or the connection dropped mid-reply: keep the partial text,
        // marked incomplete.
        Outcome::Cancelled(partial) | Outcome::Truncated(partial) => match save(&partial, "cancelled") {
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
    window::hide(&window);
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

/// Opens a link from a model reply in the system browser. Replies are untrusted, so
/// only http(s) links are accepted and the app window itself never navigates.
#[tauri::command]
fn open_link(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only http(s) links can be opened".to_string());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// Problems found at startup that the user should see (the shortcut could not be
/// registered, the chat database could not be opened).
struct StartupNotices(Vec<String>);

#[tauri::command]
fn get_startup_notices(notices: State<'_, StartupNotices>) -> Vec<String> {
    notices.0.clone()
}

/// Opens the on-disk database; if that fails, falls back to a temporary in-memory one
/// so the app still starts and can tell the user why chats will not be saved.
fn open_database(app: &AppHandle) -> (Db, Option<String>) {
    let opened = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no data directory: {e}"))
        .and_then(|dir| Db::open(&dir.join("ghost-query.db")));
    match opened {
        Ok(db) => (db, None),
        Err(e) => (
            Db::open_in_memory().expect("in-memory database"),
            Some(format!("Could not open the chat database ({e}). Chats will not be saved this session.")),
        ),
    }
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

    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
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
        .plugin(tauri_plugin_opener::init())
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
            get_startup_notices,
            open_link,
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

            let mut notices = Vec::new();
            let (db, db_notice) = open_database(app.handle());
            notices.extend(db_notice);
            app.manage(ChatState::new(db));

            let window = app.get_webview_window("main").unwrap();

            // The config protects the window by default; honour the user's choice.
            let settings = settings_path(app.handle())
                .map(|path| Settings::load(&path))
                .unwrap_or_default();
            let _ = window.set_content_protected(settings.hide_from_capture);

            // No Dock icon, and a panel that can float over fullscreen apps.
            #[cfg(target_os = "macos")]
            {
                let _ = app.app_handle().set_activation_policy(tauri::ActivationPolicy::Accessory);
                if let Err(e) = window::convert_to_panel(&window) {
                    log::warn!("could not convert the window to a panel: {e}");
                    notices.push(format!("The window could not be set up to float over fullscreen apps ({e})."));
                }
            }

            // Ctrl+Shift+Space summons/dismisses the window.
            if let Err(e) = app.global_shortcut().register("ctrl+shift+space") {
                let message = format!("Ctrl+Shift+Space could not be registered ({e}). Another app may be using it.");
                log::warn!("{message}");
                notices.push(message);
            }

            // With no Dock icon and a hidden window, a startup problem would otherwise be
            // invisible, so show the window whenever there is something to tell the user.
            if !notices.is_empty() {
                window::show(&window);
            }
            // GHOST_QUERY_SHOW_ON_START is a development aid: a few seconds after launch (like
            // a real shortcut press, not during setup) it shows the window so scripts can
            // inspect it without needing the shortcut.
            if cfg!(debug_assertions) && std::env::var_os("GHOST_QUERY_SHOW_ON_START").is_some() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(4));
                    let inner = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        if let Some(main) = inner.get_webview_window("main") {
                            window::show(&main);
                            log::info!("dev show; visible now: {}", window::is_visible(&main));
                        }
                    });
                });
            }
            app.manage(StartupNotices(notices));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
