// Prevents a console window from showing up on Windows in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, Emitter};
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, hotkey::{HotKey, Modifiers, Code}};
use serde::{Deserialize, Serialize};
// use futures_util::StreamExt; // Not needed for non-streaming
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;
use reqwest::Client;
use std::env;
use dotenv::dotenv;

// --- The following is for Windows-specific stealthing ---
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrA, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW};
#[cfg(target_os = "windows")]
use winapi::shared::windef::HWND;

// API Provider structures
#[derive(Debug, Serialize, Deserialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    generation_config: GeminiGenerationConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiGenerationConfig {
    temperature: f32,
    max_output_tokens: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PerplexityRequest {
    model: String,
    messages: Vec<PerplexityMessage>,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct PerplexityMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PerplexityResponse {
    choices: Vec<PerplexityChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PerplexityChoice {
    message: PerplexityMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ConversationMessage {
    id: String,
    role: String, // "user" or "assistant"
    content: String,
    timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Conversation {
    messages: VecDeque<ConversationMessage>,
    max_messages: usize,
}

impl Conversation {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            max_messages: 20, // Keep last 20 messages for context
        }
    }

    fn add_message(&mut self, role: String, content: String) -> String {
        let id = Uuid::new_v4().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let message = ConversationMessage {
            id: id.clone(),
            role,
            content,
            timestamp,
        };

        self.messages.push_back(message);

        // Keep only the last max_messages
        if self.messages.len() > self.max_messages {
            self.messages.pop_front();
        }

        id
    }

    fn get_context(&self) -> String {
        self.messages
            .iter()
            .map(|msg| format!("{}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn clear(&mut self) {
        self.messages.clear();
    }
}

// Global conversation state (in a real app, you'd want proper state management)
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref CONVERSATION: Arc<Mutex<Conversation>> = Arc::new(Mutex::new(Conversation::new()));
    static ref STREAM_CANCELLED: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

#[tauri::command]
async fn ask_ai_stream(prompt: String, model: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    // Reset cancellation flag
    STREAM_CANCELLED.store(false, Ordering::Relaxed);
    
    // Add user message to conversation and get context
    let contextual_prompt = {
        let mut conversation = CONVERSATION.lock().unwrap();
        conversation.add_message("user".to_string(), prompt.clone());
        
        // Build context-aware prompt
        let context = conversation.get_context();
        if context.is_empty() {
            prompt.clone()
        } else {
            format!("Previous conversation:\n{}\n\nUser: {}", context, prompt)
        }
    };
    
    let client = Client::new();
    
    // Determine which API provider to use based on model
    if model.starts_with("gemini") {
        call_gemini_api(&client, &model, &contextual_prompt, &app_handle).await
    } else if model == "sonar" {
        call_perplexity_api(&client, &model, &contextual_prompt, &app_handle).await
    } else {
        let error_msg = format!("Unsupported model: {}", model);
        let _ = app_handle.emit("ai-response-error", &error_msg);
        Err(error_msg)
    }
}

async fn call_gemini_api(
    client: &Client,
    model: &str,
    prompt: &str,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    // Use proxy server instead of direct API calls
    let proxy_url = env::var("PROXY_URL")
        .unwrap_or_else(|_| "https://proxy-server-p9wzc2v53-prem-thatikondas-projects.vercel.app".to_string());
    
    let url = format!("{}/api/gemini", proxy_url);
    
    let request_body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "temperature": 0.7,
        "maxTokens": 2048,
        "stream": true
    });

    match client.post(&url).json(&request_body).send().await {
        Ok(response) => {
            if response.status().is_success() {
                let mut stream = response.bytes_stream();
                let mut full_content = String::new();
                
                use futures_util::StreamExt;
                
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
                    let chunk_str = String::from_utf8_lossy(&chunk);
                    
                    // Process each line in the chunk
                    for line in chunk_str.lines() {
                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if data == "[DONE]" {
                                // Stream finished
                                let mut conversation = CONVERSATION.lock().unwrap();
                                conversation.add_message("assistant".to_string(), full_content.clone());
                                let _ = app_handle.emit("ai-response-done", &full_content);
                                return Ok(());
                            }
                            
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(content) = parsed["content"].as_str() {
                                    full_content.push_str(content);
                                    let _ = app_handle.emit("ai-response-chunk", content);
                                } else if let Some(error_msg) = parsed["error"].as_str() {
                                    let _ = app_handle.emit("ai-response-error", error_msg);
                                    return Err(format!("Proxy server error: {}", error_msg));
                                }
                            }
                        }
                    }
                }
                
                // If we get here, stream ended without [DONE]
                let mut conversation = CONVERSATION.lock().unwrap();
                conversation.add_message("assistant".to_string(), full_content.clone());
                let _ = app_handle.emit("ai-response-done", &full_content);
                Ok(())
            } else {
                let status = response.status();
                let response_text = response.text().await.unwrap_or_else(|_| "Failed to read error response".to_string());
                let error_msg = format!("Proxy server returned error: {} - {}", status, response_text);
                let _ = app_handle.emit("ai-response-error", &error_msg);
                Err(error_msg)
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to connect to proxy server: {}", e);
            let _ = app_handle.emit("ai-response-error", &error_msg);
            Err(error_msg)
        }
    }
}

async fn call_perplexity_api(
    client: &Client,
    model: &str,
    prompt: &str,
    app_handle: &tauri::AppHandle,
) -> Result<(), String> {
    // Use proxy server instead of direct API calls
    let proxy_url = env::var("PROXY_URL")
        .unwrap_or_else(|_| "https://proxy-server-p9wzc2v53-prem-thatikondas-projects.vercel.app".to_string());
    
    let url = format!("{}/api/perplexity", proxy_url);
    
    let request_body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": true
    });

    match client.post(&url).json(&request_body).send().await {
        Ok(response) => {
            if response.status().is_success() {
                let mut stream = response.bytes_stream();
                let mut full_content = String::new();
                
                use futures_util::StreamExt;
                
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
                    let chunk_str = String::from_utf8_lossy(&chunk);
                    
                    // Process each line in the chunk
                    for line in chunk_str.lines() {
                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if data == "[DONE]" {
                                // Stream finished
                                let mut conversation = CONVERSATION.lock().unwrap();
                                conversation.add_message("assistant".to_string(), full_content.clone());
                                let _ = app_handle.emit("ai-response-done", &full_content);
                                return Ok(());
                            }
                            
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(content) = parsed["content"].as_str() {
                                    full_content.push_str(content);
                                    let _ = app_handle.emit("ai-response-chunk", content);
                                } else if let Some(error_msg) = parsed["error"].as_str() {
                                    let _ = app_handle.emit("ai-response-error", error_msg);
                                    return Err(format!("Proxy server error: {}", error_msg));
                                }
                            }
                        }
                    }
                }
                
                // If we get here, stream ended without [DONE]
                let mut conversation = CONVERSATION.lock().unwrap();
                conversation.add_message("assistant".to_string(), full_content.clone());
                let _ = app_handle.emit("ai-response-done", &full_content);
                Ok(())
            } else {
                let status = response.status();
                let response_text = response.text().await.unwrap_or_else(|_| "Failed to read error response".to_string());
                let error_msg = format!("Proxy server returned error: {} - {}", status, response_text);
                let _ = app_handle.emit("ai-response-error", &error_msg);
                Err(error_msg)
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to connect to proxy server: {}", e);
            let _ = app_handle.emit("ai-response-error", &error_msg);
            Err(error_msg)
        }
    }
}

#[tauri::command]
fn get_conversation_history() -> Result<Vec<ConversationMessage>, String> {
    let conversation = CONVERSATION.lock().unwrap();
    Ok(conversation.messages.iter().cloned().collect())
}

#[tauri::command]
fn clear_conversation() -> Result<(), String> {
    let mut conversation = CONVERSATION.lock().unwrap();
    conversation.clear();
    Ok(())
}

#[tauri::command]
fn stop_streaming() -> Result<(), String> {
    STREAM_CANCELLED.store(true, Ordering::Relaxed);
    Ok(())
}

fn main() {
    // Load environment variables from .env file
    dotenv().ok();
    
    // We need to create the hotkey manager before the app starts
    let manager = GlobalHotKeyManager::new().unwrap();
    
    // Register our hotkey
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
    manager.register(hotkey).unwrap();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ask_ai_stream, get_conversation_history, clear_conversation, stop_streaming])
        .setup(move |app| {
            // Get a handle to the main window
            let window = app.get_webview_window("main").unwrap();
            // Start the app hidden
            window.hide().unwrap();

            // --- macOS Specific: Hide Dock icon and make it a utility panel ---
            #[cfg(target_os = "macos")]
            {
                // Hide the Dock icon using Tauri v2 methods
                let app_handle = app.app_handle();
                let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                
                // Make the window a non-activating utility panel
                use cocoa::appkit::{NSWindow, NSWindowStyleMask, NSWindowCollectionBehavior};
                let ns_window = window.ns_window().unwrap() as cocoa::base::id;
                unsafe {
                    ns_window.setStyleMask_(NSWindowStyleMask::NSTitledWindowMask | NSWindowStyleMask::NSClosableWindowMask | NSWindowStyleMask::NSMiniaturizableWindowMask | NSWindowStyleMask::NSResizableWindowMask);
                    ns_window.setCollectionBehavior_(NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary);
                }
            }

            // --- Windows Specific: Apply the stealth styles ---
            #[cfg(target_os = "windows")]
            {
                let hwnd = window.hwnd().unwrap() as HWND;
                unsafe {
                    let style = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
                    SetWindowLongPtrA(hwnd as isize, GWL_EXSTYLE, style as isize);
                }
            }
            
            // --- This thread listens for the hotkey press ---
            let main_window = window.clone();
            std::thread::spawn(move || {
                let event_receiver = GlobalHotKeyEvent::receiver();
                let mut last_toggle = std::time::Instant::now();
                
                for event in event_receiver.iter() {
                    if event.id == hotkey.id() {
                        // Debounce: only allow toggling every 200ms
                        if last_toggle.elapsed().as_millis() < 200 {
                            continue;
                        }
                        last_toggle = std::time::Instant::now();
                        
                        // Toggle window visibility
                        if main_window.is_visible().unwrap() {
                            main_window.hide().unwrap();
                        } else {
                            // Center the window on screen when showing
                            main_window.center().unwrap();
                            main_window.show().unwrap();
                            main_window.set_focus().unwrap();
                            // Ensure the window stays on top and focused
                            main_window.set_always_on_top(true).unwrap();
                            main_window.unminimize().unwrap();
                            // Bring to front
                            main_window.set_always_on_top(false).unwrap(); // Reset to allow normal window behavior
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}