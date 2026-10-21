# Ghost Query — Architecture Document

> Reference for v2 rebuild. Covers current state, known issues, and design recommendations.

---

## Table of Contents

1. [Current State](#current-state)
2. [Architecture Overview](#architecture-overview)
3. [Component Deep Dive](#component-deep-dive)
4. [Data Flow](#data-flow)
5. [Known Issues & Bugs](#known-issues--bugs)
6. [Security Concerns](#security-concerns)
7. [Code Quality Issues](#code-quality-issues)
8. [v2 Design Recommendations](#v2-design-recommendations)
9. [Feature Roadmap Suggestions](#feature-roadmap-suggestions)

---

## Current State

**Version:** 0.2.1  
**Stack:** Tauri 2.8.5 + React 18 + Vercel (Express proxy)  
**Status:** Functional prototype with significant technical debt.

### What Works

- Desktop app launches, toggles via global hotkey (Ctrl+Shift+Space)
- Sends prompts to Gemini 2.0 Flash through proxy server
- Streams responses token-by-token to the UI
- Renders markdown with syntax-highlighted code blocks
- Conversation history (in-memory, last 20 messages)
- Copy code blocks and full responses
- Rate limiting on proxy (100 req/15min general, 50 Gemini, 30 Perplexity)

### What's Broken or Missing

- Stop streaming button does nothing (flag set but never checked)
- Gemini streaming is simulated (non-streaming API + artificial chunking)
- Perplexity model is commented out in the frontend
- No conversation persistence across restarts
- No authentication on proxy server
- CSP disabled entirely

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    DESKTOP APP                          │
│                                                         │
│  ┌───────────────┐    Tauri IPC    ┌────────────────┐  │
│  │ React (Vite)  │ ◄─────────────► │ Rust Backend   │  │
│  │ localhost:5173 │  invoke/listen  │ (main.rs)      │  │
│  └───────────────┘                  └───────┬────────┘  │
│                                             │            │
└─────────────────────────────────────────────┼────────────┘
                                              │ HTTP POST
                                              ▼
                                    ┌──────────────────┐
                                    │  Proxy Server     │
                                    │  (Express/Vercel) │
                                    └────────┬─────────┘
                                             │ HTTPS
                              ┌──────────────┴──────────────┐
                              ▼                              ▼
                    ┌──────────────┐              ┌──────────────┐
                    │ Gemini API   │              │ Perplexity   │
                    │ (Google)     │              │ API          │
                    └──────────────┘              └──────────────┘
```

### Why This Architecture

The proxy server exists to **keep API keys off the client**. In a desktop app, the binary is extractable — embedding API keys in the Rust/JS bundle would expose them. The proxy is the only entity that holds `GEMINI_API_KEY` and `PERPLEXITY_API_KEY`.

---

## Component Deep Dive

### 1. React Frontend (`src/src/`)

**Entry:** `main.jsx` → `App.tsx`

| File | Purpose |
|------|---------|
| `App.tsx` | Main chat UI. Manages all state, event listeners, markdown rendering. |
| `components/ui/chat-input.tsx` | Composable chat input (context-based). Handles Enter-to-send, auto-resize. |
| `components/ui/model-selector.tsx` | Dropdown for model selection. Currently only 1 model active. |
| `components/ui/button.tsx` | shadcn/ui-style button with variants. |
| `components/ui/textarea.tsx` | Styled textarea primitive. |
| `hooks/use-textarea-resize.tsx` | Auto-resizes textarea to fit content. |
| `lib/utils.ts` | `cn()` utility (clsx + tailwind-merge). |

**State Management:** Plain `useState` hooks. No state library. All state lives in `App.tsx`:

```tsx
message          // Current input text
isLoading        // Waiting for AI response
response         // Accumulated streaming response
error            // Last error message
conversationHistory  // Full message list from backend
showHistory      // Toggle history panel
selectedModel    // Currently selected model ID
copiedStates     // Per-code-block copy feedback
fullResponseCopied  // Full response copy feedback
```

**Event System:** Tauri events via `@tauri-apps/api/event`:

| Event | Direction | Payload |
|-------|-----------|---------|
| `ai-response-chunk` | Rust → React | `string` (token) |
| `ai-response-done` | Rust → React | `string` (full response) |
| `ai-response-error` | Rust → React | `string` (error message) |
| `ai-response-cancelled` | Rust → React | `string` (unused) |

### 2. Rust Backend (`src-tauri/src/main.rs`)

**Core structures:**

```rust
ConversationMessage { id, role, content, timestamp }
Conversation { messages: VecDeque<ConversationMessage>, max_messages: 20 }
```

**Global state (lazy_static):**

```rust
static ref CONVERSATION: Arc<Mutex<Conversation>>
static ref STREAM_CANCELLED: Arc<AtomicBool>
```

**Tauri commands exposed to frontend:**

| Command | Args | Returns | Purpose |
|---------|------|---------|---------|
| `ask_ai_stream` | `prompt: String, model: String` | `Result<(), String>` | Send prompt, stream response |
| `get_conversation_history` | none | `Vec<ConversationMessage>` | Fetch all messages |
| `clear_conversation` | none | `Result<(), String>` | Wipe conversation |
| `stop_streaming` | none | `Result<(), String>` | Set cancel flag (BROKEN) |

**Platform-specific behavior:**

- **macOS:** Hides dock icon (`ActivationPolicy::Accessory`), sets window as utility panel (can join all spaces)
- **Windows:** Applies `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` styles
- **Global hotkey:** `Ctrl+Shift+Space` toggles window visibility with 200ms debounce

### 3. Proxy Server (`proxy-server/api/index.js`)

**Framework:** Express 4, deployed as Vercel serverless function.

**Endpoints:**

| Method | Path | Rate Limit | Description |
|--------|------|------------|-------------|
| `GET` | `/health` | — | Health check |
| `POST` | `/api/gemini` | 100/15min + 50/15min | Chat with Gemini |
| `POST` | `/api/perplexity` | 100/15min + 30/15min | Chat with Perplexity |
| `GET` | `/api/models` | — | List available models |

**Request format (both providers):**

```json
{
  "model": "gemini-2.0-flash-exp",
  "prompt": "user message (includes full conversation context)",
  "temperature": 0.7,
  "maxTokens": 2048,
  "stream": true
}
```

**Streaming implementation:**

- **Perplexity:** True SSE passthrough. The proxy reads Perplexity's streaming response and forwards chunks.
- **Gemini:** **Simulated.** Calls non-streaming `generateContent`, then chunks the response word-by-word with 50ms delay between words.

**Rate limiting strategy:** `express-rate-limit` with `keyGenerator: (req) => \`${req.ip}-${req.get("User-Agent")}\``.

---

## Data Flow

### Message Send Flow

```
1. User types message, presses Enter
2. App.tsx: handleSubmit() → setIsLoading(true), setResponse("")
3. App.tsx: invoke("ask_ai_stream", { prompt, model })
4. main.rs:148: ask_ai_stream() called
5. main.rs:153-164: Lock CONVERSATION mutex, add user message, build contextual prompt
   contextual_prompt = "Previous conversation:\nuser: ...\nassistant: ...\n\nUser: {prompt}"
6. main.rs:169-177: Route to call_gemini_api() or call_perplexity_api() based on model prefix
7. main.rs:187-188: Build proxy URL (env PROXY_URL or hardcoded Vercel URL)
8. main.rs:200: POST to proxy with JSON body
9. proxy/index.js:68: Receive request, validate, apply rate limits
10. proxy/index.js:90-102: Build Gemini API request, call Google API
11. proxy/index.js:129-148: Simulate streaming — split response into words, send each as SSE chunk
12. main.rs:208-234: Read SSE stream, parse "data: " lines
13. main.rs:227: For each chunk: emit("ai-response-chunk", content)
14. main.rs:219-220: On "[DONE]": add assistant message to CONVERSATION, emit("ai-response-done")
15. App.tsx:40-41: listen("ai-response-chunk") → setResponse(prev => prev + chunk)
16. App.tsx:44-48: listen("ai-response-done") → setIsLoading(false), loadConversationHistory()
```

### Conversation Context Build

```
Frontend sends: "What's the capital of France?"

If CONVERSATION has:
  - user: "Hello"
  - assistant: "Hi! How can I help?"

Contextual prompt sent to API:
  "Previous conversation:
  user: Hello
  assistant: Hi! How can I help?

  User: What's the capital of France?"
```

### Stream Parsing (Rust side)

```rust
// SSE line format from proxy:
// data: {"content":"The ","model":"gemini-2.0-flash-exp","provider":"gemini"}
// data: {"content":"capital ","model":"gemini-2.0-flash-exp","provider":"gemini"}
// data: [DONE]

// Rust parses each line:
for line in chunk_str.lines() {
    if line.starts_with("data: ") {
        let data = &line[6..];
        if data == "[DONE]" { /* finalize */ }
        // Parse JSON, extract "content" field, emit event
    }
}
```

---

## Known Issues & Bugs

### Critical

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| 1 | **Stop streaming is non-functional** | `main.rs:348-351` | `STREAM_CANCELLED` is set but never checked in streaming loops. The stop button does nothing. |
| 2 | **No proxy authentication** | `proxy-server/api/index.js:11-17` | CORS `origin: "*"` + no auth = anyone can use your API keys by calling the Vercel URL directly. |
| 3 | **Hardcoded proxy URL** | `main.rs:188,266` | If the Vercel deployment URL changes, the app breaks. No fallback or configuration UI. |

### Functional

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| 4 | **Gemini streaming is simulated** | `proxy/index.js:104-158` | Calls non-streaming API, chunks words with 50ms delay. Adds ~2-5s latency vs real streaming. |
| 5 | **Perplexity model disabled** | `model-selector.tsx:28-40` | `sonar` is commented out. Only Gemini 2.0 Flash is selectable. |
| 6 | **No conversation persistence** | `main.rs:142-145` | `CONVERSATION` is in-memory. All history lost on app restart. |
| 7 | **Duplicate context in requests** | `main.rs:153-163` + `App.tsx:122` | Backend builds contextual prompt with full history, but the prompt itself is the latest user message. The context prepended is redundant if the API also receives conversation history (which it doesn't — so this is the only context path, but the frontend doesn't know). |
| 8 | **`always_on_top` race** | `main.rs:421-424` | Sets `always_on_top(true)` then immediately `false` — causes window flicker. |

### Code Quality

| # | Issue | Location | Impact |
|---|-------|----------|--------|
| 9 | **Massive code duplication** | `main.rs:180-332` | `call_gemini_api` and `call_perplexity_api` are ~75 lines each, nearly identical. |
| 10 | **Single model hardcoded** | `model-selector.tsx:21-40` | Only one model in the array. Dropdown UI exists but only shows one option. |
| 11 | **Inconsistent output format** | `proxy/index.js:245` vs Gemini endpoint | Perplexity forces JSON output (`summary`, `details`, `key_points`), Gemini does not. Frontend tries to parse JSON from both. |
| 12 | **`formatResponse` complexity** | `App.tsx:201-277` | Tries JSON parsing, regex extraction, then raw text. Brittle. |

---

## Security Concerns

### Current Threats

1. **API key extraction** — The proxy is publicly accessible. Anyone can POST to `/api/gemini` with their own prompts and consume your quota.
2. **No CSP** — `tauri.conf.json` has `"csp": null`. The webview can load any script, open any URL.
3. **CORS wide open** — `origin: "*"` on the proxy means any website can call your proxy.
4. **Rate limiting bypass** — `keyGenerator` uses IP + User-Agent. Both are trivially spoofable.
5. **Input injection** — User prompts are passed directly to API without sanitization. A malicious prompt could extract system prompts or manipulate API behavior.
6. **No HTTPS enforcement** — The proxy URL defaults to HTTPS but the env var `PROXY_URL` could be set to HTTP.

### Recommendations for v2

- Add API key authentication to the proxy (client generates a session key, proxy validates)
- Set a real CSP in Tauri config
- Implement CORS allowlist instead of wildcard
- Add request signing or token-based auth
- Sanitize/validate prompts before forwarding to APIs

---

## v2 Design Recommendations

### Architecture

**Keep the three-tier split.** It's sound. The proxy server correctly keeps API keys off the client. But improve each layer:

#### Frontend

```
src/
├── app/
│   ├── App.tsx                 # Root layout
│   ├── routes/
│   │   ├── chat/               # Main chat view
│   │   └── settings/           # API config, proxy URL
│   └── providers/
│       └── chat-provider.tsx    # Centralized chat state (useReducer or zustand)
├── components/
│   ├── chat/
│   │   ├── message-list.tsx
│   │   ├── message-bubble.tsx
│   │   ├── chat-input.tsx
│   │   └── code-block.tsx
│   ├── model-selector/
│   │   └── model-selector.tsx
│   └── ui/                     # Primitive components
├── hooks/
│   ├── use-stream.ts           # Encapsulate SSE streaming logic
│   ├── use-conversation.ts     # Conversation CRUD
│   └── use-hotkey.ts           # Global hotkey binding
├── lib/
│   ├── api.ts                  # Tauri invoke wrappers
│   ├── stream-parser.ts        # SSE line parser
│   └── utils.ts
└── types/
    └── index.ts
```

**Key changes:**
- Move chat state to `useReducer` or `zustand` (no more prop drilling)
- Extract streaming logic into a `useStream` hook
- Separate `stream-parser.ts` for SSE parsing (testable independently)
- Add a settings route for proxy URL configuration

#### Rust Backend

```
src-tauri/src/
├── main.rs                    # App entry, window setup, hotkey
├── commands/
│   ├── chat.rs                # ask_ai_stream, stop_streaming
│   └── conversation.rs        # get_history, clear, etc.
├── providers/
│   ├── mod.rs
│   ├── gemini.rs              # Gemini-specific request/response
│   └── perplexity.rs          # Perplexity-specific request/response
├── proxy.rs                   # HTTP client, SSE streaming
├── conversation.rs            # Conversation struct + persistence
├── state.rs                   # AppState (replaces lazy_static)
└── config.rs                  # Configurable proxy URL, etc.
```

**Key changes:**
- Replace `lazy_static` globals with Tauri's managed `AppState`
- Split providers into separate modules
- Implement actual stream cancellation (check `STREAM_CANCELLED` in the loop)
- Add conversation persistence (SQLite via `tauri-plugin-sql` or file-based)
- Make proxy URL configurable via `tauri.conf.json` or a config file

#### Proxy Server

```
proxy-server/
├── api/
│   ├── index.js               # Express app setup
│   ├── routes/
│   │   ├── gemini.js
│   │   ├── perplexity.js
│   │   └── models.js
│   ├── middleware/
│   │   ├── auth.js            # API key validation
│   │   ├── rate-limit.js
│   │   └── validate.js        # Request schema validation
│   └── services/
│       ├── gemini.js           # Actual Gemini API calls
│       └── perplexity.js       # Actual Perplexity API calls
├── package.json
└── vercel.json
```

**Key changes:**
- Add authentication middleware (client sends a session token)
- Add request validation (joi/zod)
- Use Gemini's native streaming (`streamGenerateContent`) instead of simulated
- Structured logging
- Health check should report rate limit status

### State Management

**Current:** Global `Mutex<Conversation>` in Rust + `useState` in React.

**Recommended v2:**

```tsx
// React side: zustand or useReducer
interface ChatState {
  messages: Message[];
  streamingMessage: string | null;
  isLoading: boolean;
  error: string | null;
  selectedModel: string;
  proxyUrl: string;
}

// Rust side: Tauri managed state
struct AppState {
  conversation: Mutex<Conversation>,
  config: Mutex<AppConfig>,
}
```

### Streaming Fix

**Current problem:** `STREAM_CANCELLED` is set but never read.

**Fix for v2:**

```rust
// In the streaming loop:
while let Some(chunk) = stream.next().await {
    if STREAM_CANCELLED.load(Ordering::Relaxed) {
        let _ = app_handle.emit("ai-response-cancelled", "Stream stopped by user");
        return Ok(());
    }
    // ... process chunk
}
```

### Conversation Persistence

**Current:** In-memory only. Lost on restart.

**Options for v2:**

1. **SQLite via `tauri-plugin-sql`** — Best for desktop. Supports queries, migrations.
2. **JSON file** — Simple. Store in app data directory (`tauri::api::path::app_data_dir`).
3. **Both** — SQLite for active conversation, JSON export for backup.

Recommended: SQLite for conversations + settings, JSON export for sharing.

### Model Management

**Current:** Hardcoded array with 1 active model.

**v2 approach:** Fetch models from proxy server's `/api/models` endpoint at startup. This allows:
- Adding models without app updates
- Different model lists per deployment
- Dynamic pricing/availability info

---

## Feature Roadmap Suggestions

### Phase 1: Stability (v2.0)

- [ ] Fix stop streaming (check cancellation flag in loop)
- [ ] Fix Gemini real streaming (use `streamGenerateContent`)
- [ ] Add proxy authentication (API key per client)
- [ ] Set proper CSP in Tauri config
- [ ] Persist conversations to SQLite
- [ ] Configurable proxy URL in settings
- [ ] Uncomment and enable Perplexity models
- [ ] Fix `always_on_top` race condition

### Phase 2: Polish (v2.1)

- [ ] Multi-conversation support (tabs or sidebar)
- [ ] Message editing and regeneration
- [ ] Conversation export (Markdown, JSON)
- [ ] Keyboard shortcuts (Cmd+K for model switch, Cmd+N for new chat)
- [ ] Dark/light theme toggle
- [ ] Streaming indicator animation
- [ ] Error retry with exponential backoff

### Phase 3: Power Features (v2.2+)

- [ ] File/image upload (Vision models)
- [ ] Tool use / function calling
- [ ] System prompt customization
- [ ] Token counting and cost estimation
- [ ] Conversation search
- [ ] Plugin system for custom providers (OpenAI, Anthropic, local Ollama)
- [ ] Multi-window support (split conversations)
- [ ] Auto-update via Tauri updater plugin

---

## File Reference (Current)

| Path | Lines | Purpose |
|------|-------|---------|
| `src-tauri/src/main.rs` | 434 | Core Rust backend — commands, hotkey, streaming, platform setup |
| `src-tauri/src/lib.rs` | 16 | Tauri entry point (unused in current setup — `main.rs` runs directly) |
| `src/src/App.tsx` | 465 | Entire chat UI — state, events, rendering, formatting |
| `proxy-server/api/index.js` | 440 | Express proxy — Gemini/Perplexity endpoints, rate limiting, SSE |
| `src/src/components/ui/chat-input.tsx` | 204 | Composable chat input with context pattern |
| `src/src/components/ui/model-selector.tsx` | 113 | Model dropdown (1 active model) |
| `src-tauri/tauri.conf.json` | 32 | Tauri config — window, build, security |
| `src-tauri/Cargo.toml` | 35 | Rust dependencies |
| `src/package.json` | 40 | Frontend dependencies |
| `proxy-server/package.json` | 25 | Proxy dependencies |

---

## Environment Variables

| Variable | Where | Required | Description |
|----------|-------|----------|-------------|
| `GEMINI_API_KEY` | Proxy `.env` | Yes | Google Gemini API key |
| `PERPLEXITY_API_KEY` | Proxy `.env` | Yes | Perplexity API key |
| `PROXY_URL` | Rust `.env` (optional) | No | Proxy server URL (defaults to hardcoded Vercel URL) |

---

## Dependency Versions (Current)

| Package | Version | Notes |
|---------|---------|-------|
| Tauri | 2.8.5 | Desktop framework |
| React | 18.2.0 | UI library |
| Vite | 7.1.2 | Build tool |
| Tailwind CSS | 4.0.0-alpha.25 | Styling (alpha!) |
| reqwest | 0.12 | Rust HTTP client |
| tokio | 1.0 | Async runtime |
| Express | 4.18.2 | Proxy server |
| axios | 1.6.2 | Proxy HTTP client |

**Warning:** Tailwind CSS is on an alpha version. Pin to stable for v2.

---

*Document generated from codebase analysis. Refer to source files for implementation details.*
