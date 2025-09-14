// Prevents a console window from showing up on Windows in release mode
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, Emitter};
use global_hotkey::{GlobalHotKeyManager, GlobalHotKeyEvent, hotkey::{HotKey, Modifiers, Code}};
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// --- The following is for Windows-specific stealthing ---
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrA, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW};
#[cfg(target_os = "windows")]
use winapi::shared::windef::HWND;

#[derive(Debug, Serialize, Deserialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaResponse {
    response: String,
    done: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct StreamChunk {
    response: String,
    done: bool,
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
async fn ask_ai_stream(prompt: String, app_handle: tauri::AppHandle) -> Result<(), String> {
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
    
    let client = reqwest::Client::new();
    let request_body = OllamaRequest {
        model: "llama3.2".to_string(), // Default model, can be made configurable
        prompt: contextual_prompt,
        stream: true,
    };

    match client
        .post("http://localhost:11434/api/generate")
        .json(&request_body)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                let mut stream = response.bytes_stream();
                let mut full_response = String::new();
                
                while let Some(chunk) = stream.next().await {
                    // Check if streaming was cancelled
                    if STREAM_CANCELLED.load(Ordering::Relaxed) {
                        let _ = app_handle.emit("ai-response-cancelled", "Stream cancelled by user");
                        return Ok(());
                    }
                    
                    match chunk {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            let lines: Vec<&str> = text.lines().collect();
                            
                            for line in lines {
                                if !line.trim().is_empty() {
                                    if let Ok(chunk_data) = serde_json::from_str::<StreamChunk>(line) {
                                        full_response.push_str(&chunk_data.response);
                                        
                                        // Emit the current response chunk to the frontend
                                        let _ = app_handle.emit("ai-response-chunk", &chunk_data.response);
                                        
                                        if chunk_data.done {
                                            // Add assistant response to conversation
                                            let mut conversation = CONVERSATION.lock().unwrap();
                                            conversation.add_message("assistant".to_string(), full_response.clone());
                                            
                                            let _ = app_handle.emit("ai-response-done", &full_response);
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let _ = app_handle.emit("ai-response-error", &format!("Stream error: {}", e));
                            return Err(format!("Stream error: {}", e));
                        }
                    }
                }
                Ok(())
            } else {
                let _ = app_handle.emit("ai-response-error", &format!("Ollama API returned error: {}", response.status()));
                Err(format!("Ollama API returned error: {}", response.status()))
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to connect to Ollama: {}. Make sure Ollama is running on localhost:11434", e);
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