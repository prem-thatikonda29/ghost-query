# Ghost Query: Phase Plan

Working checklist. Tick items as they land; one commit per phase; stop for review between phases.
Verified facts (checked against live docs/API on 2026-10-21) are in "Reference facts" at the bottom.

## Phase 0: Bug fixes and structure  (no new features; app keeps using the proxy)
- [x] Move all logic from `src-tauri/src/main.rs` into `lib.rs`; `main.rs` only calls `app_lib::run()`
- [x] Replace `lazy_static` globals with Tauri managed state
- [x] Working Stop: per-request `CancellationToken`, stream aborted with `select!`, `ai-response-cancelled` emitted
- [x] Buffered SSE parser (handles lines split across chunks, ignores `:` comments, stops at `[DONE]`, surfaces mid-stream `error` chunks)
- [x] History bugs: user message only committed with its reply; no dup on retry; partial reply kept on cancel
- [x] Cargo: drop `dotenv`, `tokio-stream`, `url`, `lazy_static`, `cocoa`; add `tokio-util`
- [x] `.gitignore` already ignores `.env` (earlier "bug" was a misread; nothing to fix)
- [x] Rust unit tests for SSE parser
- [x] Verify: `cargo check`/`cargo test` (11 pass), frontend `vite build`

## Phase 1: Provider, keys, proxy removal  (moved ahead of history so the app can make real requests)
- [x] One OpenAI-compatible streaming client (base URL up to `/v1`, append `/chat/completions`)
- [x] OpenRouter default; any OpenAI-compatible base URL (Ollama, Groq...) via Settings; model list fetched from `{base}/models`
- [x] Settings view: base URL, model ID, key; key in OS keychain via `keyring` (frontend can write / check-exists, never read)
- [x] Map errors: 401 bad key, 402/credits, 429 limits, mid-stream `error.metadata.error_type`
- [x] Delete `proxy-server/` and the hardcoded Vercel URL

- [x] Structured `[system, ...history, user]` messages instead of pasted text (partial Phase 2 history work)
- [x] Removed dead frontend formatters (they flattened all Markdown); README rewritten
- [x] Live test with a real key (user confirmed working with OpenRouter)
- [x] Separate Settings page with Back button; per-provider keys + presets (OpenAI, Claude, Gemini, Groq, NVIDIA, Mistral, Together, DeepSeek, Ollama) and a Custom URL+key slot
- [ ] User to test multi-provider Settings, then start history phase

## Phase 2: Proper history (SQLite)
- [x] SQLite (`rusqlite`, bundled) in the app data dir; versioned migrations (`PRAGMA user_version`); tables `conversations`, `messages` (role, content, status complete/cancelled, timestamps), WAL, foreign keys with cascade delete
- [x] Commands: list/create (reuses an empty one)/rename/delete conversation, get messages
- [x] Structured messages `[system, ...history, user]`; [ ] editable system prompt
- [x] Token-budget trimming (approx chars/4, budget 6000, always keeps the new prompt, never starts on an assistant turn); [ ] per-model context from `/models`
- [x] Regenerate replaces the last exchange atomically (failed regenerate keeps the old answer)
- [x] Stopped replies are saved with `cancelled` status; nothing is written for failed/empty requests
- [x] Basic frontend: message thread, conversation switcher, New chat, Delete, Regenerate; old in-memory history and regex hacks removed
- [ ] Rename UI (backend command exists)
- [ ] User to test, then Phase 3

## Phase 3: Ghost window
- [x] Frameless (`decorations: false`), always-on-top, skip taskbar, all workspaces (macOS/Linux; unsupported on Windows), starts hidden; config guarded by a unit test
- [x] `contentProtected` on by default plus a Settings toggle (macOS 15+ ScreenCaptureKit may ignore it; UI says "asks the OS", never "guarantees")
- [x] `tauri-plugin-global-shortcut` (Ctrl+Shift+Space, debounced) replaces the hand-rolled hotkey thread and the `global-hotkey` crate
- [x] No window-style hacks; Windows needs no custom code (Tauri handles skipTaskbar/content protection)
- [x] Frameless extras: draggable header (`core:window:allow-start-dragging`), Esc hides, Ctrl/Cmd+Q and a Settings button quit, hotkey-registration failure shows the window plus a notice
- [ ] User to test: hotkey, Esc, drag, quit, behaviour over fullscreen apps (plain Tauri windows may not float over fullscreen Spaces; `tauri-nspanel` is the fallback), capture hiding in a real screen share
- [ ] Windows: not built or tested (compile check planned in CI phase)

## Phase 4+: Later
- [ ] FTS5 search over history
- [ ] Tool calling + MCP (`rmcp`), approval prompt before any tool runs; evaluate `rig-core` behind the Provider trait
- [ ] CI: macOS + `windows-latest` compile check, release on tag

## Reference facts
- OpenRouter: base `https://openrouter.ai/api/v1`, path `/chat/completions` (live POST: 401 on that path, 404 on `/api/v1`)
- Optional headers: `HTTP-Referer`, `X-OpenRouter-Title`
- SSE may contain comment lines (ignore); usage arrives once in the final chunk before `[DONE]`
- Pre-stream errors = HTTP status; mid-stream errors = chunk with top-level `error` + `finish_reason: "error"`; stable field `error.metadata.error_type`; non-streaming can be 200 with only `error`
- `:free` models: 20 req/min; 50/day if <10 credits bought, 1000/day otherwise; negative balance blocks even free models
- Crates (2026-10-21): tauri 2.12.5, tauri-plugin-global-shortcut 2.4.1, keyring 4.2.0 (API changed since 3.x; read docs first), rmcp 3.6.0, rusqlite 0.40.2, tauri-nspanel 2.1.1, rig-core 0.45.0. `langchain-rust` is stale (2024), avoid.
- Unverified: what "FreeLLMApi" is (treat as a preset if OpenAI-compatible); Tauri window API docs
