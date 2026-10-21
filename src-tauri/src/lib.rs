mod conversation;
mod sse;

use conversation::{Conversation, ConversationMessage};
use futures_util::StreamExt;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use reqwest::Client;
use sse::SseParser;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio_util::sync::CancellationToken;

// Temporary: Phase 3 replaces the proxy with direct, key-based provider calls.
const DEFAULT_PROXY_URL: &str = "https://proxy-server-2smrrg86i-prem-thatikondas-projects.vercel.app";

#[derive(Default)]
struct ChatState {
    conversation: Mutex<Conversation>,
    /// Token of the in-flight request, tagged with its id so a finished request
    /// never clears the token of a newer one.
    active: Mutex<Option<(u64, CancellationToken)>>,
    next_id: AtomicU64,
}

enum Outcome {
    Done(String),
    /// Carries whatever was streamed before the user pressed Stop.
    Cancelled(String),
    Failed(String),
}

async fn stream_from_proxy<R: Runtime>(
    client: &Client,
    url: &str,
    body: serde_json::Value,
    token: &CancellationToken,
    app: &AppHandle<R>,
) -> Outcome {
    let mut full = String::new();

    let response = tokio::select! {
        _ = token.cancelled() => return Outcome::Cancelled(full),
        result = client.post(url).json(&body).send() => match result {
            Ok(r) => r,
            Err(e) => return Outcome::Failed(format!("Failed to connect to proxy server: {e}")),
        },
    };

    if !response.status().is_success() {
        let status = response.status();
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error response".to_string());
        return Outcome::Failed(format!("Proxy server returned error: {status} - {text}"));
    }

    let mut stream = response.bytes_stream();
    let mut parser = SseParser::new();

    loop {
        let next = tokio::select! {
            _ = token.cancelled() => return Outcome::Cancelled(full),
            next = stream.next() => next,
        };
        let Some(chunk) = next else {
            // Stream closed without [DONE]; keep what we have.
            return Outcome::Done(full);
        };
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => return Outcome::Failed(format!("Stream error: {e}")),
        };

        for data in parser.push(&chunk) {
            if data == "[DONE]" {
                return Outcome::Done(full);
            }
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) else {
                continue;
            };
            if let Some(content) = parsed["content"].as_str() {
                full.push_str(content);
                let _ = app.emit("ai-response-chunk", content);
            } else if let Some(error) = parsed["error"].as_str() {
                return Outcome::Failed(format!("Proxy server error: {error}"));
            }
        }
    }
}

#[tauri::command]
async fn ask_ai_stream(
    prompt: String,
    model: String,
    app: AppHandle,
    state: State<'_, ChatState>,
) -> Result<(), String> {
    let proxy_url = std::env::var("PROXY_URL").unwrap_or_else(|_| DEFAULT_PROXY_URL.to_string());

    // History is only read here; it is written once the exchange has finished.
    let contextual_prompt = state.conversation.lock().unwrap().build_prompt(&prompt);

    let (url, body) = if model.starts_with("gemini") {
        (
            format!("{proxy_url}/api/gemini"),
            serde_json::json!({
                "model": model,
                "prompt": contextual_prompt,
                "temperature": 0.7,
                "maxTokens": 2048,
                "stream": true
            }),
        )
    } else if model == "sonar" {
        (
            format!("{proxy_url}/api/perplexity"),
            serde_json::json!({ "model": model, "prompt": contextual_prompt, "stream": true }),
        )
    } else {
        let _ = app.emit("ai-response-error", format!("Unsupported model: {model}"));
        return Ok(());
    };

    let token = CancellationToken::new();
    let id = state.next_id.fetch_add(1, Ordering::Relaxed);
    if let Some((_, previous)) = state.active.lock().unwrap().replace((id, token.clone())) {
        previous.cancel();
    }

    let outcome = stream_from_proxy(&Client::new(), &url, body, &token, &app).await;

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
            stop_streaming
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serves one SSE chunk, then stalls forever (a slow model).
    async fn stalling_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await;
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n";
            let event = "data: {\"content\":\"partial\"}\n\n";
            let _ = sock
                .write_all(format!("{head}{:x}\r\n{event}\r\n", event.len()).as_bytes())
                .await;
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn stop_aborts_a_stalled_stream_and_keeps_partial_output() {
        let url = stalling_server().await;
        let app = tauri::test::mock_app();
        let token = CancellationToken::new();

        let canceller = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            canceller.cancel();
        });

        let started = std::time::Instant::now();
        let outcome = stream_from_proxy(
            &Client::new(),
            &url,
            serde_json::json!({}),
            &token,
            app.handle(),
        )
        .await;

        assert!(started.elapsed().as_secs() < 5, "Stop did not abort the stream");
        match outcome {
            Outcome::Cancelled(partial) => assert_eq!(partial, "partial"),
            Outcome::Done(_) => panic!("expected Cancelled, got Done"),
            Outcome::Failed(e) => panic!("expected Cancelled, got Failed: {e}"),
        }
    }

    #[tokio::test]
    async fn unreachable_server_fails_instead_of_hanging() {
        let app = tauri::test::mock_app();
        let outcome = stream_from_proxy(
            &Client::new(),
            "http://127.0.0.1:9",
            serde_json::json!({}),
            &CancellationToken::new(),
            app.handle(),
        )
        .await;
        assert!(matches!(outcome, Outcome::Failed(_)));
    }
}
