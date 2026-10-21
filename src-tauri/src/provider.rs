//! One OpenAI-compatible chat client (OpenRouter by default, but any
//! `{base_url}/chat/completions` endpoint such as Ollama, Groq or OpenAI works).

use crate::conversation::ChatMessage;
use crate::settings::Settings;
use crate::sse::SseParser;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Runtime};
use tokio_util::sync::CancellationToken;

pub enum Outcome {
    Done(String),
    /// Carries whatever was streamed before the user pressed Stop.
    Cancelled(String),
    Failed(String),
}

#[derive(Debug, PartialEq)]
pub enum Event {
    Delta(String),
    Done,
    Error(String),
    Ignore,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_length: Option<u64>,
}

pub fn chat_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

pub fn is_openrouter(base_url: &str) -> bool {
    base_url.contains("openrouter.ai")
}

pub fn build_request(model: &str, messages: &[ChatMessage]) -> Value {
    serde_json::json!({ "model": model, "messages": messages, "stream": true })
}

fn describe_error(error: &Value) -> String {
    let message = error["message"]
        .as_str()
        .or_else(|| error.as_str())
        .unwrap_or("Unknown provider error");
    match error["metadata"]["error_type"].as_str() {
        Some(kind) => format!("{message} ({kind})"),
        None => message.to_string(),
    }
}

/// Interprets one SSE `data:` payload. Mid-stream failures arrive as a chunk with a
/// top-level `error` object and `finish_reason: "error"` (HTTP status is already 200).
pub fn parse_event(data: &str) -> Event {
    let data = data.trim();
    if data == "[DONE]" {
        return Event::Done;
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return Event::Ignore;
    };
    if let Some(error) = value.get("error") {
        return Event::Error(describe_error(error));
    }
    let choice = &value["choices"][0];
    if let Some(content) = choice["delta"]["content"].as_str() {
        if !content.is_empty() {
            return Event::Delta(content.to_string());
        }
    }
    if choice["finish_reason"] == "error" {
        return Event::Error("The model reported an error.".to_string());
    }
    Event::Ignore
}

/// Readable message for a non-2xx response received before streaming started.
pub fn error_for_status(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from));
    let base = match status {
        401 => "Invalid or missing API key. Open Settings to update it.",
        402 => "Insufficient credits for this model. Pick a :free model or add credits.",
        403 => "Request was blocked (moderation or key restrictions).",
        404 => "Model or endpoint not found. Check the model ID and base URL in Settings.",
        408 => "The request timed out.",
        429 => "Rate limited. Free models allow 20 requests/minute and a daily cap; wait and retry.",
        500..=599 => "The provider had an error. Try again or pick another model.",
        _ => "Request failed.",
    };
    match detail {
        Some(d) => format!("{base} ({status}: {d})"),
        None => format!("{base} ({status})"),
    }
}

pub fn parse_models(catalog: &Value) -> Vec<ModelInfo> {
    let mut models: Vec<ModelInfo> = catalog["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|m| {
                    let id = m["id"].as_str()?.to_string();
                    Some(ModelInfo {
                        name: m["name"].as_str().unwrap_or(&id).to_string(),
                        context_length: m["context_length"].as_u64(),
                        id,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // Free models first, then alphabetical.
    models.sort_by(|a, b| {
        let free = |m: &ModelInfo| m.id.ends_with(":free") || m.id == "openrouter/free";
        free(b).cmp(&free(a)).then_with(|| a.id.cmp(&b.id))
    });
    models
}

pub async fn fetch_models(client: &Client, base_url: &str, api_key: Option<&str>) -> Result<Vec<ModelInfo>, String> {
    let mut request = client.get(format!("{}/models", base_url.trim_end_matches('/')));
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|e| format!("Could not reach {base_url}: {e}"))?;
    let status = response.status();
    let body = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(error_for_status(status.as_u16(), &body));
    }
    let catalog: Value = serde_json::from_str(&body).map_err(|e| format!("Unexpected models response: {e}"))?;
    Ok(parse_models(&catalog))
}

pub async fn stream_chat<R: Runtime>(
    client: &Client,
    settings: &Settings,
    api_key: Option<&str>,
    messages: &[ChatMessage],
    token: &CancellationToken,
    app: &AppHandle<R>,
) -> Outcome {
    let mut full = String::new();

    let mut request = client
        .post(chat_url(&settings.base_url))
        .json(&build_request(&settings.model, messages));
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    if is_openrouter(&settings.base_url) {
        request = request
            .header("X-OpenRouter-Title", "Ghost Query")
            .header("HTTP-Referer", "https://github.com/prem-thatikonda29/ghost-query");
    }

    let response = tokio::select! {
        _ = token.cancelled() => return Outcome::Cancelled(full),
        result = request.send() => match result {
            Ok(r) => r,
            Err(e) => return Outcome::Failed(format!("Could not reach {}: {e}", settings.base_url)),
        },
    };

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Outcome::Failed(error_for_status(status, &body));
    }

    let mut stream = response.bytes_stream();
    let mut parser = SseParser::new();

    loop {
        let next = tokio::select! {
            _ = token.cancelled() => return Outcome::Cancelled(full),
            next = stream.next() => next,
        };
        let Some(chunk) = next else {
            // Closed without [DONE]; keep what we have.
            return Outcome::Done(full);
        };
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => return Outcome::Failed(format!("Stream error: {e}")),
        };

        for data in parser.push(&chunk) {
            match parse_event(&data) {
                Event::Delta(text) => {
                    full.push_str(&text);
                    let _ = app.emit("ai-response-chunk", text);
                }
                Event::Done => return Outcome::Done(full),
                Event::Error(message) => return Outcome::Failed(message),
                Event::Ignore => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into() }
    }

    #[test]
    fn chat_url_appends_path_once() {
        assert_eq!(chat_url("https://openrouter.ai/api/v1"), "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(chat_url("http://localhost:11434/v1/"), "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn request_body_is_structured_messages() {
        let body = build_request("m", &[msg("system", "s"), msg("user", "hi")]);
        assert_eq!(body["model"], "m");
        assert_eq!(body["stream"], true);
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "hi");
    }

    #[test]
    fn parses_stream_events() {
        assert_eq!(parse_event("[DONE]"), Event::Done);
        assert_eq!(
            parse_event(r#"{"choices":[{"delta":{"content":"Hi"}}]}"#),
            Event::Delta("Hi".into())
        );
        assert_eq!(parse_event(r#"{"choices":[{"delta":{"content":""}}]}"#), Event::Ignore);
        assert_eq!(parse_event(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#), Event::Ignore);
        assert_eq!(parse_event("not json"), Event::Ignore);
    }

    #[test]
    fn parses_mid_stream_error_chunk() {
        let chunk = r#"{"id":"gen-1","error":{"code":429,"message":"Rate limit exceeded","metadata":{"error_type":"rate_limit_exceeded"}},"choices":[{"index":0,"delta":{"content":""},"finish_reason":"error"}]}"#;
        assert_eq!(
            parse_event(chunk),
            Event::Error("Rate limit exceeded (rate_limit_exceeded)".into())
        );
    }

    #[test]
    fn maps_http_errors_to_readable_messages() {
        let body = r#"{"error":{"code":401,"message":"No auth credentials found"}}"#;
        let m = error_for_status(401, body);
        assert!(m.contains("API key") && m.contains("No auth credentials found"));
        assert!(error_for_status(402, "").contains("credits"));
        assert!(error_for_status(429, "").contains("Rate limited"));
        assert!(error_for_status(503, "garbage").contains("503"));
    }

    #[test]
    fn model_catalog_lists_free_models_first() {
        let catalog = serde_json::json!({"data":[
            {"id":"z/paid","name":"Paid","context_length":8000},
            {"id":"a/model:free","name":"Free One","context_length":32000},
            {"id":"openrouter/free"}
        ]});
        let ids: Vec<_> = parse_models(&catalog).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["a/model:free", "openrouter/free", "z/paid"]);
    }

    /// Serves `events` as an SSE response, then keeps the connection open for
    /// `then_stall` (or closes it). Returns the base URL and the raw request text.
    async fn fake_server(events: &'static str, then_stall: bool) -> (String, tokio::sync::oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap();
            let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string());
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n";
            let _ = sock
                .write_all(format!("{head}{:x}\r\n{events}\r\n", events.len()).as_bytes())
                .await;
            if then_stall {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            } else {
                let _ = sock.write_all(b"0\r\n\r\n").await;
            }
        });
        (format!("http://{addr}/v1"), rx)
    }

    fn settings(base_url: String) -> Settings {
        Settings { base_url, model: "test/model".into() }
    }

    #[tokio::test]
    async fn streams_to_completion_and_sends_auth_to_the_right_path() {
        let events = ": OPENROUTER PROCESSING\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n";
        let (base, request_rx) = fake_server(events, false).await;
        let app = tauri::test::mock_app();

        let outcome = stream_chat(
            &Client::new(),
            &settings(base),
            Some("test-key"),
            &[msg("user", "hi")],
            &CancellationToken::new(),
            app.handle(),
        )
        .await;

        assert!(matches!(outcome, Outcome::Done(ref t) if t == "Hello"));
        let request = request_rx.await.unwrap().to_lowercase();
        assert!(request.starts_with("post /v1/chat/completions"), "{request}");
        assert!(request.contains("authorization: bearer test-key"));
        assert!(request.contains("\"stream\":true"));
    }

    #[tokio::test]
    async fn mid_stream_error_fails_the_request() {
        let events = "data: {\"choices\":[{\"delta\":{\"content\":\"par\"}}]}\n\ndata: {\"error\":{\"code\":502,\"message\":\"Provider disconnected\"},\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":\"error\"}]}\n\n";
        let (base, _rx) = fake_server(events, false).await;
        let app = tauri::test::mock_app();

        let outcome = stream_chat(&Client::new(), &settings(base), None, &[msg("user", "hi")], &CancellationToken::new(), app.handle()).await;

        assert!(matches!(outcome, Outcome::Failed(ref e) if e.contains("Provider disconnected")));
    }

    #[tokio::test]
    async fn stop_aborts_a_stalled_stream_and_keeps_partial_output() {
        let events = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
        let (base, _rx) = fake_server(events, true).await;
        let app = tauri::test::mock_app();
        let token = CancellationToken::new();

        let canceller = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            canceller.cancel();
        });

        let started = std::time::Instant::now();
        let outcome = stream_chat(&Client::new(), &settings(base), None, &[msg("user", "hi")], &token, app.handle()).await;

        assert!(started.elapsed().as_secs() < 5, "Stop did not abort the stream");
        assert!(matches!(outcome, Outcome::Cancelled(ref t) if t == "partial"));
    }

    #[tokio::test]
    async fn unreachable_server_fails_instead_of_hanging() {
        let app = tauri::test::mock_app();
        let outcome = stream_chat(
            &Client::new(),
            &settings("http://127.0.0.1:9/v1".into()),
            None,
            &[msg("user", "hi")],
            &CancellationToken::new(),
            app.handle(),
        )
        .await;
        assert!(matches!(outcome, Outcome::Failed(_)));
    }
}
