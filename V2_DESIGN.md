# Ghost Query v2 — Design Document

> Architecture decisions, provider strategy, agentic features, and privacy system.

---

## 1. Provider Architecture — No Proxy for BYOK, Optional Proxy for Free Tier

### The Core Principle

**Any secret embedded in a binary you distribute can be extracted.** It doesn't matter if you base64-encode it, split it across files, or compile it into Rust instead of JavaScript — someone with a decompiler or a packet sniffer will find it. This is not a theoretical concern; it's a guarantee.

So the security model splits into two cases:

| Case | Who holds the key? | Proxy needed? |
|------|-------------------|---------------|
| **BYOK** (user provides their own key) | User, stored in OS keychain | No — direct API calls from Rust |
| **Ollama** (local model) | Nobody — it's local | No — localhost, no auth |
| **Free tier** (app pays for inference) | You, the developer | **Yes** — key must never leave the server |

### BYOK + Ollama: Direct from Rust

When the user brings their own key, the Rust desktop app talks directly to APIs. No middleman. The Tauri binary IS the server.

```
React Frontend
    ↕ Tauri IPC
Rust Backend → OpenRouter API (user's key from OS keychain)
Rust Backend → OpenAI API (user's key from OS keychain)
Rust Backend → Ollama (localhost, no auth)
```

- **No deployment step** — users download the app, enter their keys, done
- **No server bill** — no server to host or maintain
- **Lower latency** — one fewer network hop
- **Works offline** — Ollama mode needs no internet
- **Secure** — keys live in OS keychain, never in config files or env vars

### Free Tier: A Minimal Server Is Non-Negotiable

If you want users to open the app and chat immediately without providing any API key, you need a server-side component that holds your key and never ships it to the client. There is no way around this.

The options are variations on the same idea:

**Option A: Hardened proxy (recommended)**
Same shape as v1's proxy, but done properly:
- Authentication (device fingerprint or anonymous rate limiting by IP)
- CORS allowlist (not `origin: "*"`)
- Request validation and rate limiting per device
- The proxy holds your OpenRouter/Anthropic key and forwards requests
- Deployed as a serverless function (Vercel, Cloudflare Workers) — minimal cost

**Option B: OpenRouter free models only**
OpenRouter has free-tier models (suffix `:free` — e.g., `openai/gpt-4o-mini:free`). If you provision a shared OpenRouter key scoped only to free models, worst case if it leaks is people burn through free-tier limits, not your money. Still a shared secret, still extractable, but blast radius is bounded. This limits model selection but eliminates server costs.

**Option C: Ollama as the actual free default**
Ollama runs locally. "Free" here means local compute, not free cloud inference. This sidesteps the entire problem — no key to protect, no server to run. The tradeoff is the user needs decent hardware and must download models.

**Recommended approach:** Default to Ollama (free, private, no server). Offer BYOK for cloud models. Optionally offer a free tier via a hardened proxy for users who can't run Ollama.

```
User opens app
    │
    ├── "Use Ollama" (default) → localhost, no key needed
    ├── "I have an API key" → store in OS keychain, direct calls
    └── "Free cloud" → proxy server (rate-limited, your key)
```

---

## 2. Unified Provider Architecture

### The OpenRouter Strategy

OpenRouter (`openrouter.ai`) is a single API that proxies requests to OpenAI, Anthropic, Google, Meta, Mistral, and 100+ other models. You send one request to one endpoint with one API key:

```
POST https://openrouter.ai/api/v1/chat/completions
Authorization: Bearer sk-or-...
```

Same format as OpenAI's API. Supports streaming. Supports every major model.

**This means your provider code is one function, not five.**

### Provider Enum

```rust
enum Provider {
    OpenRouter { api_key: String },       // Gateway to 100+ models
    Ollama { base_url: String },           // Local, no auth
    OpenAI { api_key: String },            // Direct BYOK
    Anthropic { api_key: String },         // Direct BYOK
    Google { api_key: String },            // Direct BYOK
    Custom { base_url: String, api_key: String, format: ApiFormat },
}

enum ApiFormat {
    OpenAI,      // /v1/chat/completions — OpenAI, OpenRouter, many others
    Anthropic,   // /v1/messages
    Google,      // /v1beta/models/{model}:generateContent
    Ollama,      // /api/chat
}
```

### Why This Matters

| v1 (Current) | v2 (Proposed) |
|-------------|--------------|
| `call_gemini_api()` — 75 lines | One `send_request(provider, messages, model)` function |
| `call_perplexity_api()` — 75 lines | Same function, different `Provider` enum variant |
| Adding a new provider = copy-paste 75 lines | Adding a new provider = add enum variant + format handler |
| Proxy server mandatory | No proxy needed |
| Hardcoded models | Dynamic model list from API |

### Request Flow (Simplified)

```
User types message
       │
       ▼
Frontend: invoke("send_message", { content, model_id })
       │
       ▼
Rust: Resolve model_id → (Provider, model_name)
       │
       ▼
Rust: Build messages array, call provider's API
       │
       ▼
Rust: Stream SSE chunks → emit("response-chunk", token)
       │
       ▼
Frontend: Accumulate and render
```

One code path. One streaming implementation. Providers are just configuration.

### Model Resolution

Instead of hardcoding models, fetch available models from each configured provider at startup:

```rust
async fn fetch_models(provider: &Provider) -> Vec<Model> {
    match provider {
        Provider::Ollama { base_url } => {
            // GET {base_url}/api/tags
            // Returns locally available models
        }
        Provider::OpenRouter { .. } => {
            // GET https://openrouter.ai/api/v1/models
            // Returns all available models with pricing
        }
        _ => {
            // Provider-specific model listing endpoints
        }
    }
}
```

Users see a unified model list grouped by provider. They pick one. The app resolves it to the right API format.

---

## 3. API Key Management — OS Keychain

Provider API keys (OpenRouter, OpenAI, Anthropic, etc.) must never be stored in `.env`, config files, or plain text. Use the OS keychain. This rule applies to **provider API keys only** — MCP server secrets are handled differently (see Section 4).

### Rust Implementation

```rust
// Use keyring crate for cross-platform keychain access
use keyring::Entry;

fn store_key(provider: &str, api_key: &str) -> Result<()> {
    let entry = Entry::new("ghost-query", provider)?;
    entry.set_password(api_key)?;
    Ok(())
}

fn get_key(provider: &str) -> Result<String> {
    let entry = Entry::new("ghost-query", provider)?;
    Ok(entry.get_password()?)
}

fn delete_key(provider: &str) -> Result<()> {
    let entry = Entry::new("ghost-query", provider)?;
    entry.delete_password()?;
    Ok(())
}
```

This uses:
- **macOS:** Keychain
- **Windows:** Credential Manager
- **Linux:** Secret Service (GNOME Keyring / KWallet)

### Onboarding Flow

```
┌──────────────────────────────────────────┐
│         Welcome to Ghost Query           │
│                                          │
│  Choose how you want to chat with AI:    │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │  OpenRouter (Recommended)          │  │
│  │  One key, 100+ models              │  │
│  │  [Paste API Key]                   │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │  Local Ollama                      │  │
│  │  Free, private, runs on your machine│ │
│  │  [Auto-detected: llama3.2, mistral] │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │  Bring Your Own Key                │  │
│  │  OpenAI, Anthropic, Google, etc.   │  │
│  │  [Select provider] [Paste key]     │  │
│  └────────────────────────────────────┘  │
│                                          │
│  Skip for now — explore with Ollama      │
└──────────────────────────────────────────┘
```

### Multi-Provider Support

Users can configure multiple providers simultaneously. The model selector shows all available models across all configured providers:

```
Model Selector
├── OpenRouter
│   ├── claude-sonnet-4-20250514 (Anthropic)
│   ├── gpt-4o (OpenAI)
│   ├── gemini-2.5-flash (Google)
│   └── llama-3.3-70b (Meta)
├── Ollama (Local)
│   ├── llama3.2
│   ├── mistral
│   └── codellama
├── OpenAI (Direct)
│   └── gpt-4o
└── Anthropic (Direct)
    └── claude-sonnet-4-20250514
```

---

## 4. Agentic Features — MCP Integration

### What is MCP?

The **Model Context Protocol** (Anthropic, 2024) is an open standard for connecting AI models to external tools and data sources. It defines:

- **MCP Server** — exposes tools, resources, and prompts
- **MCP Client** — connects to servers, calls tools, reads resources

This is exactly what you need for email, calendar, file operations, etc. Instead of writing integrations yourself, you connect to existing MCP servers.

### Architecture

```
Ghost Query (MCP Client)
    │
    ├── connects to ──► Email MCP Server (IMAP/Gmail API)
    ├── connects to ──► Calendar MCP Server (CalDAV/Google Calendar)
    ├── connects to ──► Filesystem MCP Server (read/write files)
    ├── connects to ──► Browser MCP Server (web search, scraping)
    └── connects to ──► Custom MCP Servers (user-defined)
```

### MCP Server Discovery and Secrets

MCP servers are configured in a config file. **Secrets are not stored in this file** — they're pulled from the OS keychain at spawn time and injected as environment variables.

Config file (structure only, no secrets):

```json
// ~/.config/ghost-query/mcp.json
{
  "servers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/prem/Documents"]
    },
    "email": {
      "command": "npx",
      "args": ["-y", "@anthropic/email-mcp-server"],
      "env_keys": {
        "EMAIL_PROVIDER": "gmail",
        "EMAIL_CREDENTIALS_PATH": "ghost-query:mcp:email-credentials"
      }
    },
    "brave-search": {
      "command": "npx",
      "args": ["-y", "@anthropic/brave-search-mcp-server"],
      "env_keys": {
        "BRAVE_API_KEY": "ghost-query:mcp:brave-api-key"
      }
    }
  }
}
```

The `env_keys` field maps environment variable names to keychain entry identifiers. At spawn time, the Rust backend resolves each keychain identifier to its actual value and injects it as an env var:

```rust
fn spawn_mcp_server(config: &McpServerConfig) -> Result<Child> {
    let mut envs = std::env::vars().collect::<HashMap<_, _>>();
    
    // Resolve keychain references to actual secrets
    for (env_var, keychain_id) in &config.env_keys {
        let entry = Entry::new("ghost-query", keychain_id)?;
        let secret = entry.get_password()?;  // from OS keychain
        envs.insert(env_var.clone(), secret);
    }
    
    // Spawn with secrets as env vars (never written to disk)
    Command::new(&config.command)
        .args(&config.args)
        .envs(&envs)
        .spawn()
}
```

**Why this is different from provider keys:** MCP server secrets (API keys for email, search, etc.) are configured by the user for their own MCP servers. They're user-owned secrets, not your app's secrets. The keychain is still the right storage — the config file just references where to find them, not the values themselves. This prevents accidental leakage via config file sharing, git commits, or backup dumps.

### Tool Calling Flow

```
1. User: "Send an email to john@example.com about the meeting tomorrow"
2. App sends message + available tools to AI model
3. AI model returns: tool_call { name: "send_email", args: { to, subject, body } }
4. App routes tool_call to the email MCP server
5. MCP server executes the action (sends email via API)
6. App returns result to AI model
7. AI model: "I've sent the email to John about tomorrow's meeting."
```

### Tool Categories

| Category | Examples | MCP Servers |
|----------|----------|-------------|
| **Communication** | Send email, Slack message, Telegram | email-mcp, slack-mcp |
| **Calendar** | Create event, check schedule, invite attendees | calendar-mcp |
| **Files** | Read/write/organize files, convert formats | filesystem-mcp |
| **Web** | Search, scrape, fetch content | brave-search-mcp, fetch-mcp |
| **Development** | Run commands, git operations, code search | filesystem-mcp, git-mcp |
| **Productivity** | Notes, tasks, bookmarks | obsidian-mcp, todoist-mcp |

### MCP is the Plugin System

Instead of building every integration yourself, you provide a clean MCP client and let the community build servers. Users can even write their own MCP servers for custom workflows.

---

## 5. Simplified Architecture — Two Components, Not Three

### v1: Three Separate Components

```
React App (port 5173)
    ↕ Tauri IPC
Rust Backend
    ↕ HTTP
Express Proxy (Vercel)
```

### v2: Two Components

```
React Frontend (embedded in Tauri webview)
    ↕ Tauri IPC
Rust Backend (everything else)
```

**The proxy server is gone.** The Rust binary handles everything:
- API communication (direct to providers)
- Key management (OS keychain)
- Conversation state (in-memory + SQLite)
- MCP server management (spawning, communicating)
- Window management (hotkeys, visibility, screen capture hiding)

### What Moves to Rust

| v1 Location | v2 Location | Why |
|-------------|-------------|-----|
| Proxy server (Express) | Rust `reqwest` | No server needed |
| API key in `.env` | OS keychain via `keyring` crate | Secure storage |
| Streaming parsing in proxy | Rust SSE parser | Simpler, no network hop |
| Conversation in Rust globals | Rust `AppState` + SQLite | Persistence |
| Model list hardcoded | Fetched from providers at runtime | Dynamic |

### What Stays in React

- UI rendering
- Markdown parsing
- Code block rendering
- Input handling
- State display (messages, loading, errors)

### React ↔ Rust Interface

```rust
// Tauri commands exposed to frontend
#[tauri::command] async fn send_message(content: String, model_id: String) -> Result<()>
#[tauri::command] async fn stop_streaming() -> Result<()>
#[tauri::command] fn get_conversation_history() -> Result<Vec<Message>>
#[tauri::command] fn clear_conversation() -> Result<()>
#[tauri::command] async fn list_providers() -> Result<Vec<ProviderInfo>>
#[tauri::command] async fn add_provider(provider_type: String, api_key: String) -> Result<()>
#[tauri::command] async fn remove_provider(provider_id: String) -> Result<()>
#[tauri::command] async fn list_models() -> Result<Vec<ModelInfo>>
#[tauri::command] async fn list_mcp_tools() -> Result<Vec<Tool>>
#[tauri::command] async fn call_mcp_tool(server: String, tool: String, args: Value) -> Result<Value>
#[tauri::command] fn get_settings() -> Result<Settings>
#[tauri::command] fn update_settings(settings: Settings) -> Result<()>
```

**20 commands.** Clean, well-defined interface. The frontend never touches APIs directly.

---

## 6. OS-Level Privacy — Screen Share & Recording Hiding

This is the killer feature that makes Ghost Query unique. No other AI chat app does this.

### The Problem

When you're in a video call and share your screen, every open window is visible. Ghost Query should be invisible unless you explicitly want it shown.

### Platform Implementation

#### macOS

```rust
// 1. Hide from screen capture (macOS 13+)
// NSWindow.sharingType controls screen capture visibility
use cocoa::appkit::NSWindow;

unsafe {
    let ns_window = window.ns_window().unwrap() as cocoa::base::id;
    // NSWindowSharingTypeNone = 0 — invisible to screen capture
    ns_window.setSharingType_(0); // NSWindowSharingTypeNone
}

// 2. Hide from dock (already done in v1)
app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);

// 3. Hide from Mission Control
unsafe {
    ns_window.setCollectionBehavior_(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary
        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorIgnoredForCycle
    );
}

// 4. Hide from Cmd+Tab (app switcher)
// Already achieved by Accessory activation policy
```

#### Windows

```rust
// 1. Hide from screen capture
// Windows doesn't have a direct API for this, but we can use:
// - SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE) — Windows 10 2004+
use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};

unsafe {
    let hwnd = window.hwnd().unwrap();
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).ok();
}

// 2. Hide from taskbar + Alt+Tab (already done in v1)
let style = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
SetWindowLongPtrA(hwnd as isize, GWL_EXSTYLE, style as isize);

// 3. Hide from screen recording (partial)
// WDA_EXCLUDEFROMCAPTURE also excludes from most recording software
```

#### Linux

Linux screen capture exclusion is fundamentally limited. Here's the honest assessment:

**X11:**
- `_NET_WM_STATE_SKIP_TASKBAR` — hides from taskbar. Does NOT exclude from screen capture.
- `_NET_WM_STATE_SKIP_PAGER` — hides from pager/workspace switcher. Does NOT exclude from screen capture.
- There is no X11 equivalent of macOS `setSharingType_(0)` or Windows `WDA_EXCLUDEFROMCAPTURE`. X11 treats all windows as capturable by design — any X client with access to the display can capture any window.
- Possible: Set the window to use an undetectable override-redirect pattern, but this breaks window management entirely and is not practical.

**Wayland:**
- Wayland's security model is stricter — clients can't capture each other's windows. But this means the screen capture exclusion API is limited to what the compositor exposes.
- Some compositors (Sway, Hyprland) support `xdg-shell` protocols for window identification, but there's no standard "exclude from capture" protocol.
- Portal-based screen sharing (via `xdg-desktop-portal`) respects user consent prompts but doesn't give apps programmatic exclusion control.

**What this means for Ghost Query on Linux:**
- **Full privacy mode is not achievable.** The settings UI should not offer "Invisible during screen share" as an option on Linux.
- **What IS achievable:** Hide from taskbar, hide from Alt+Tab, hide from dock (if any). These are compositor-dependent but generally work.
- **Recommendation:** On Linux, offer only system visibility controls (taskbar, Alt+Tab). Don't promise screen capture exclusion. If the user needs true screen capture privacy, Ollama + the app running on a different virtual desktop is the practical workaround.

```rust
// Linux: system visibility only
#[cfg(target_os = "linux")]
{
    // X11: Set _NET_WM_STATE_SKIP_TASKBAR
    // This hides from taskbar/Alt+Tab but NOT from screen capture
    // Wayland: Depends on compositor, may not be possible
    
    // No equivalent to macOS setSharingType_(0) or Windows WDA_EXCLUDEFROMCAPTURE
    // Screen capture exclusion is not achievable on Linux
}
```

### User-Configurable Privacy Modes

```rust
enum PrivacyMode {
    Full,           // Hidden from screen share + recording + taskbar + dock (macOS/Windows only)
    ShareVisible,   // Visible on screen share, hidden from dock/taskbar
    AlwaysVisible,  // Normal window behavior
}

// Linux only supports system visibility (taskbar, Alt+Tab)
// Screen capture exclusion is not available on Linux
```

Settings UI:

```
Privacy Settings
├── Screen Sharing (macOS/Windows only — not available on Linux)
│   ├── ○ Invisible during screen share (recommended)
│   ├── ○ Visible during screen share
│   └── ○ Ask each time
├── Screen Recording (macOS only)
│   ├── ○ Invisible during recording
│   ├── ○ Visible during recording
│   └── ○ Follow screen share setting
├── System Visibility (all platforms)
│   ├── ☑ Hide from dock/taskbar
│   ├── ☑ Hide from Cmd+Tab/App Switcher
│   └── ☑ Hide from Mission Control/Alt+Tab
└── Hotkey to toggle visibility: Ctrl+Shift+Space
```

**Platform capability matrix:**

| Feature | macOS | Windows | Linux |
|---------|-------|---------|-------|
| Hide from dock/taskbar | Yes | Yes | Yes (compositor-dependent) |
| Hide from app switcher | Yes | Yes | Partial (X11: yes, Wayland: compositor-dependent) |
| Hide from screen capture | Yes (`setSharingType_(0)`) | Yes (`WDA_EXCLUDEFROMCAPTURE`) | **No** |
| Hide from screen recording | Yes (`setSharingType_(0)`) | Partial (`WDA_EXCLUDEFROMCAPTURE`) | **No** |

### "Panic" Hotkey

A global hotkey that **instantly** hides the window regardless of current state. Like a boss key.

```rust
// Ctrl+Shift+Escape — immediate hide
// Works even if the app isn't focused
// Shows a fake screen (optional: settings page, or blank)
```

---

## 7. Conversation Management

### SQLite Storage

```sql
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    model_id TEXT,
    provider_id TEXT,
    created_at DATETIME,
    updated_at DATETIME
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT REFERENCES conversations(id),
    role TEXT CHECK(role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT,
    tool_calls TEXT,  -- JSON array of tool calls
    tool_result TEXT, -- JSON result from tool execution
    created_at DATETIME
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT
);
```

### Multi-Conversation Support

```
┌─ Sidebar ─────────────────┐  ┌─ Chat Area ──────────────────────┐
│                             │  │                                    │
│  🔍 Search conversations    │  │  [Message 1 - User]               │
│                             │  │  [Message 1 - Assistant]          │
│  Today                       │  │  [Message 2 - User]               │
│  ├─ Meeting prep            │  │  [Message 2 - Assistant + tool]   │
│  ├─ Code review             │  │  [Message 3 - User]               │
│  └─ Travel planning         │  │  [Message 3 - Assistant]          │
│                             │  │                                    │
│  Yesterday                   │  │                                    │
│  ├─ API debugging           │  │                                    │
│  └─ Recipe ideas            │  │                                    │
│                             │  │                                    │
│  [+ New Chat]               │  │  [ChatInput________________] [▶]  │
└─────────────────────────────┘  └────────────────────────────────────┘
```

### Conversation Features

- Auto-title conversations (AI generates a title from first message)
- Export as Markdown
- Import from JSON
- Delete with confirmation
- Pin important conversations

---

## 8. Settings System

### Settings Structure

```rust
struct Settings {
    // Providers
    providers: Vec<ProviderConfig>,
    
    // Defaults
    default_provider: String,
    default_model: String,
    
    // Privacy
    privacy_mode: PrivacyMode,
    hide_from_screen_share: bool,
    hide_from_screen_recording: bool,
    hide_from_dock: bool,
    hide_from_app_switcher: bool,
    panic_hotkey: String,
    
    // Behavior
    toggle_hotkey: String,
    auto_title_conversations: bool,
    max_context_messages: usize,
    temperature: f32,
    
    // MCP
    mcp_servers: Vec<McpServerConfig>,
    
    // Appearance
    theme: Theme,
    font_size: u32,
}
```

### Settings UI

```
Settings
├── Providers
│   ├── [OpenRouter] [Connected] [Remove]
│   ├── [Ollama] [Connected: localhost:11434] [Remove]
│   ├── [+ Add Provider]
│   └── Default: OpenRouter
├── Privacy
│   ├── Screen sharing: Invisible
│   ├── Screen recording: Invisible
│   ├── Dock visibility: Hidden
│   └── Panic hotkey: Ctrl+Shift+Escape
├── Behavior
│   ├── Toggle hotkey: Ctrl+Shift+Space
│   ├── Auto-title conversations: On
│   ├── Max context messages: 20
│   └── Temperature: 0.7
├── MCP Servers
│   ├── [Filesystem] [Connected] [Configure] [Remove]
│   ├── [Email] [Disconnected] [Configure] [Remove]
│   └── [+ Add MCP Server]
└── Appearance
    ├── Theme: System / Light / Dark
    └── Font size: 14px
```

---

## 9. MCP Server Management

### Auto-Discovery

```rust
// Check for common MCP servers
fn discover_mcp_servers() -> Vec<McpServerConfig> {
    let mut servers = Vec::new();
    
    // Check if Ollama is running
    if is_port_open(11434) {
        servers.push(McpServerConfig {
            name: "Ollama".into(),
            transport: McpTransport::Http { url: "http://localhost:11434".into() },
        });
    }
    
    // Check for node/npx availability for stdio servers
    if command_exists("npx") {
        // Can spawn stdio MCP servers
    }
    
    servers
}
```

### MCP Transport Types

```rust
enum McpTransport {
    // Stdio — spawn a process, communicate via stdin/stdout
    Stdio { command: String, args: Vec<String> },
    
    // HTTP — connect to an HTTP endpoint
    Http { url: String },
    
    // SSE — server-sent events endpoint
    Sse { url: String },
}
```

### Built-in MCP Servers

Ship with these MCP servers bundled or auto-installable:

| Server | Purpose | Transport |
|--------|---------|-----------|
| `@anthropic/filesystem` | Read/write files | Stdio |
| `@anthropic/fetch` | HTTP requests, web scraping | Stdio |
| `@anthropic/brave-search` | Web search | Stdio |

---

## 10. Complete File Structure (v2)

```
ghost-query/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json
│   ├── src/
│   │   ├── main.rs                    # App entry, window setup
│   │   ├── lib.rs                     # Tauri builder, command registration
│   │   ├── state.rs                   # AppState (providers, conversation, settings)
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── chat.rs                # send_message, stop_streaming
│   │   │   ├── conversation.rs        # history, clear, new, delete
│   │   │   ├── provider.rs            # add/remove/list providers
│   │   │   ├── model.rs               # list models
│   │   │   ├── mcp.rs                 # list tools, call tool
│   │   │   ├── settings.rs            # get/update settings
│   │   │   └── privacy.rs             # privacy mode, screen capture
│   │   ├── providers/
│   │   │   ├── mod.rs                 # Provider trait
│   │   │   ├── openrouter.rs          # OpenRouter (OpenAI-compatible)
│   │   │   ├── ollama.rs              # Ollama local
│   │   │   ├── openai.rs              # Direct OpenAI
│   │   │   ├── anthropic.rs           # Direct Anthropic
│   │   │   └── google.rs              # Direct Google/Gemini
│   │   ├── mcp/
│   │   │   ├── mod.rs                 # MCP client
│   │   │   ├── client.rs              # MCP protocol implementation
│   │   │   ├── manager.rs             # Server lifecycle management
│   │   │   └── transport.rs           # Stdio/HTTP/SSE transports
│   │   ├── storage/
│   │   │   ├── mod.rs
│   │   │   ├── db.rs                  # SQLite operations
│   │   │   └── keychain.rs            # OS keychain (keyring crate)
│   │   ├── hotkey.rs                  # Global hotkey registration
│   │   ├── privacy.rs                 # Screen capture hiding
│   │   └── streaming.rs              # SSE parser, stream management
│   └── migrations/
│       └── 001_init.sql
├── src/                               # React frontend
│   ├── package.json
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   ├── src/
│   │   ├── main.tsx
│   │   ├── App.tsx
│   │   ├── components/
│   │   │   ├── chat/
│   │   │   │   ├── chat-view.tsx
│   │   │   │   ├── message-list.tsx
│   │   │   │   ├── message-bubble.tsx
│   │   │   │   ├── chat-input.tsx
│   │   │   │   └── code-block.tsx
│   │   │   ├── sidebar/
│   │   │   │   ├── sidebar.tsx
│   │   │   │   ├── conversation-list.tsx
│   │   │   │   └── search-bar.tsx
│   │   │   ├── settings/
│   │   │   │   ├── settings-view.tsx
│   │   │   │   ├── provider-config.tsx
│   │   │   │   ├── privacy-config.tsx
│   │   │   │   ├── mcp-config.tsx
│   │   │   │   └── appearance-config.tsx
│   │   │   ├── model-selector/
│   │   │   │   └── model-selector.tsx
│   │   │   └── ui/                    # Primitives (button, input, dialog, etc.)
│   │   ├── hooks/
│   │   │   ├── use-chat.ts            # Chat state machine
│   │   │   ├── use-stream.ts          # SSE streaming listener
│   │   │   ├── use-conversation.ts    # Conversation CRUD
│   │   │   ├── use-providers.ts       # Provider management
│   │   │   ├── use-settings.ts        # Settings
│   │   │   └── use-mcp.ts            # MCP tool listing/calling
│   │   ├── lib/
│   │   │   ├── tauri.ts               # Typed invoke wrappers
│   │   │   └── utils.ts
│   │   └── types/
│   │       └── index.ts
│   └── index.html
├── ARCHITECTURE.md                    # v1 documentation (existing)
├── V2_DESIGN.md                       # This document
└── Cargo.toml                         # Workspace
```

---

## 11. Key Rust Dependencies (v2)

```toml
[dependencies]
# Tauri
tauri = { version = "2", features = ["devtools"] }
tauri-plugin-shell = "2"          # Spawn MCP server processes
tauri-plugin-dialog = "2"         # File picker, confirmations
tauri-plugin-fs = "2"             # File system access
tauri-plugin-sql = "2"            # SQLite support

# HTTP + SSE streaming
reqwest = { version = "0.12", features = ["json", "stream"] }
eventsource-stream = "0.2"        # SSE frame parser for reqwest streams
futures-util = "0.3"
tokio = { version = "1", features = ["full"] }

# MCP (official SDK — modelcontextprotocol/rust-sdk, 17M+ downloads)
rmcp = { version = "3.0", features = ["client"] }

# Keychain (23M+ downloads — macOS Keychain, Windows Credential Manager, Linux Secret Service)
keyring = { version = "4.2", features = [
    "apple-native-keyring-store",
    "windows-native-keyring-store",
    "zbus-secret-service-keyring-store"
]}

# Database
rusqlite = { version = "0.31", features = ["bundled"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Hotkey
global-hotkey = "0.4"

# macOS specifics
[target.'cfg(target_os = "macos")'.dependencies]
cocoa = "0.25"
objc = "0.2"

# Windows specifics
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.56", features = [
    "Win32_UI_WindowsAndMessaging",
    "Win32_Foundation"
]}

# Utilities
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
log = "0.4"
env_logger = "0.11"
```

---

## 12. Feasibility Notes

### "Can Rust handle all API calling without a proxy?"

Yes. `reqwest` + `eventsource-stream` is proven for streaming OpenRouter/OpenAI-compatible APIs:

```rust
use eventsource_stream::{Eventsource, Event};
use futures_util::StreamExt;

let mut stream = client
    .post("https://openrouter.ai/api/v1/chat/completions")
    .header("Authorization", format!("Bearer {}", api_key))
    .json(&body)
    .send().await?
    .bytes_stream()
    .eventsource();

while let Some(event) = stream.next().await {
    let event = event?;
    if event.data == "[DONE]" { break; }
    let chunk: Value = serde_json::from_str(&event.data)?;
    let token = chunk["choices"][0]["delta"]["content"]
        .as_str().unwrap_or("");
    app.emit("chat://token", token).unwrap();
}
```

**Gotcha:** Don't use `reqwest::ClientBuilder::timeout()` — it kills SSE connections. Use `tcp_keepalive` instead. Also, OpenRouter sends `: keep-alive` comment lines mid-stream; `eventsource-stream` handles these correctly (manual parsers must skip `:`-prefixed lines).

### "How secure are API keys in the OS keychain?"

`keyring` v4.2.0 wraps native OS encryption:

| Platform | Storage | Encryption |
|----------|---------|------------|
| macOS | Keychain Services | User login password |
| Windows | Credential Manager | DPAPI (user scope) |
| Linux | Secret Service (GNOME Keyring) | User password (daemon-managed) |

Keys never touch plaintext files. Any process running as the same user can read them — same threat model as a proxy server (which also runs as a single user). Protection is against disk theft, other users, and accidental leakage — not against malware already on the machine.

**Linux caveat:** Secret Service requires a running daemon (GNOME Keyring, KWallet). Headless environments get `NoPlatformStore`. Fine for desktop use.

### "Is there an MCP library for Rust?"

Yes — `rmcp` v3.0.0 (official, 17M+ downloads, from `modelcontextprotocol/rust-sdk`):

```rust
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;

// Connect to MCP server via stdio (spawn subprocess)
let server = Client::new()
    .transport(TokioChildProcess::new(
        Command::new("npx")
            .arg("-y").arg("@anthropic/filesystem-mcp-server")
            .arg("/Users/prem/Documents")
    )?)
    .await?;

// List tools, call tools
let tools = server.list_tools().await?;
let result = server.call_tool("read_file", json!({ "path": "/file" })).await?;
```

Supports both stdio (spawn subprocess) and Streamable HTTP transports. Handles the full lifecycle (initialize → operate → shutdown). No need to write a custom MCP client.

### "What about a free tier — can't you just embed the key in the binary?"

No. **Any secret embedded in a distributed binary can be extracted.** This is not a theoretical concern:

- **Decompilation:** Rust binaries can be reverse-engineered with Ghidra, IDA Pro, or `strings`. Obfuscation slows this down but doesn't prevent it.
- **Network sniffing:** Even if the key is hidden in code, the outgoing HTTPS request contains it. Mitmproxy, Charles, or Wireshark on the same network will capture it.
- **Memory inspection:** The key exists in plaintext in process memory during API calls. A debugger or memory dump extracts it trivially.

Once extracted, the key is your key — with your money behind it. The attacker can burn through your API quota, and you foot the bill.

**The only way to offer "free cloud inference" without user keys is a server-side component that:**
1. Holds your API key
2. Authenticates requests (even anonymously, via rate limiting by device fingerprint or IP)
3. Never exposes the key to the client

This is the v1 proxy approach, done correctly. The alternatives (OpenRouter free-tier models, Ollama) avoid the problem rather than solving it — they're valid strategies but serve different use cases.

| Strategy | Cost to you | User experience | Key security |
|----------|------------|-----------------|--------------|
| Ollama (local) | $0 | Requires decent hardware + model download | N/A — no key |
| BYOK (user's key) | $0 | User must have API key | User's key, user's risk |
| Free tier via proxy | Server hosting + API costs | Instant, no setup | Server-held, never in binary |
| OpenRouter free models via proxy | Minimal (free-tier limits) | Instant, limited model selection | Server-held, bounded blast radius |

---

## 13. Migration Path from v1

### What to Keep

- React components (chat-input, button, textarea, code-block) — minor refactoring
- Markdown rendering logic
- Hotkey concept (implementation changes)
- Conversation message structure

### What to Rewrite

- `main.rs` — Complete rewrite with modular structure
- Provider logic — Unified `Provider` trait instead of per-provider functions
- Proxy server — Delete entirely
- State management — Replace `lazy_static` with Tauri managed state
- Model selector — Dynamic models from providers

### What to Add

- SQLite storage layer
- OS keychain integration
- MCP client
- Provider management UI
- Settings system
- Screen capture hiding
- Multi-conversation support
- Privacy modes

---

## 14. Summary of Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| API routing (BYOK) | OpenRouter + direct providers + Ollama | One function, many providers, no proxy needed |
| API routing (free tier) | Hardened proxy (optional) | Server-held key, never in binary, rate-limited |
| Key storage | OS keychain (keyring v4.2) | Secure, cross-platform, no config files |
| MCP secrets | Keychain-injected env vars at spawn time | Secrets never in config files |
| Agentic features | MCP protocol via rmcp v3.0 | Official Rust SDK, open standard, extensible |
| Architecture | 2 components (React + Rust) + optional proxy | Minimal deployment for BYOK/Ollama |
| Conversation storage | SQLite (rusqlite, bundled) | Persistent, queryable, no external DB |
| Screen hiding (macOS) | `setSharingType_(0)` + Accessory policy | Full capture exclusion + dock hiding |
| Screen hiding (Windows) | `WDA_EXCLUDEFROMCAPTURE` + `WS_EX_TOOLWINDOW` | Capture exclusion + taskbar hiding |
| Screen hiding (Linux) | System visibility only (taskbar, Alt+Tab) | Capture exclusion not achievable on Linux |
| Privacy modes | Configurable per-feature, per-platform | Honest about platform capabilities |
| Model listing | Dynamic from providers at runtime | No hardcoded models, auto-update |
| MCP transport | Stdio (primary) + Streamable HTTP | Compatible with existing MCP ecosystem |

---

*This document is the blueprint for Ghost Query v2. Start here, build from the ground up, don't port v1 cruft.*
