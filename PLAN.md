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

## Phase 1: Proper history
- [ ] SQLite (`rusqlite`) in the app data dir; tables `conversations`, `messages` (role, content, status, timestamps)
- [ ] Commands: list/create/rename/delete conversation, load messages
- [ ] Send structured `[{role, content}]` + optional system prompt (no pasted text)
- [ ] Token-budget trimming (keep system prompt + latest turns)
- [ ] Retry/regenerate replaces the reply
- [ ] Frontend: conversation list; delete `formatResponse` / `cleanCurrencyAndPercent` regex hacks

## Phase 2: Ghost window
- [ ] Frameless, always-on-top, skip taskbar/Dock, all workspaces
- [ ] `set_content_protected(true)` (macOS 15+ ScreenCaptureKit may ignore it; do not promise more than "requests exclusion")
- [ ] `tauri-plugin-global-shortcut` replaces the hand-rolled hotkey thread
- [ ] Windows: OR in `WS_EX_TOOLWINDOW` instead of overwriting styles
- [ ] Decide focus model (must accept typing); `tauri-nspanel` only if plain Tauri settings misbehave

## Phase 3: Provider, keys, proxy removal
- [ ] `Provider` trait; one OpenAI-compatible streaming client (base URL up to `/v1`, append `/chat/completions`)
- [ ] Presets: OpenRouter (default), Ollama (local), custom base URL
- [ ] Settings view: base URL, model ID, key; key in OS keychain via `keyring` (frontend can write / check-exists, never read)
- [ ] Map errors: 401 bad key, 402/credits, 429 limits, mid-stream `error.metadata.error_type`
- [ ] Delete `proxy-server/` and the hardcoded Vercel URL

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
