# Ghost Query

A Tauri desktop app for chatting with any OpenAI-compatible model provider (OpenRouter by default) — streaming responses, a global hotkey, and conversation history. No server: the app talks to the provider directly and keeps your API key in the OS keychain.

## Features

- **Bring your own key, any provider** — A separate Settings page lets you pick OpenRouter (including free `:free` models), OpenAI, Claude (Anthropic), Google Gemini, Groq, NVIDIA NIM, Mistral, Together AI, DeepSeek or local Ollama; the base URL is set for you. Choose "Custom" to enter your own base URL and key for any other OpenAI-compatible endpoint (AgentRouter, a self-hosted gateway, ...). Each provider keeps its own key and model.
- **Keys stay in the keychain** — One entry per provider in the macOS Keychain / Windows Credential Manager. The UI can save or remove a key but never reads it back, and the app never writes it to disk.
- **Streaming responses** — Real-time token-by-token output, with a Stop button that cancels the request.
- **Global hotkey** — Ctrl+Shift+Space shows or hides the window; no Dock icon on macOS.
- **Conversation history** — Browse, copy, or clear the current conversation.
- **Markdown rendering** — Syntax-highlighted code blocks with copy buttons.

## Tech Stack

- **Desktop:** Tauri 2 (Rust + React)
- **Frontend:** React, Tailwind CSS, Framer Motion, React Markdown
- **Backend:** Rust (`reqwest` streaming, `keyring`)

## Project Structure

```
├── src/                  # React frontend (Vite)
│   └── src/
│       ├── App.tsx       # Main chat UI
│       ├── components/   # UI primitives (chat input, settings page, etc.)
│       └── lib/          # Utilities
├── src-tauri/            # Tauri/Rust backend
│   └── src/
│       ├── lib.rs        # App setup, commands, hotkey
│       ├── provider.rs   # OpenAI-compatible streaming client
│       ├── providers.rs  # Built-in provider presets
│       ├── keystore.rs   # Per-provider API keys in the OS keychain
│       ├── settings.rs   # Active provider, models, custom URL (non-secret)
│       ├── conversation.rs
│       └── sse.rs        # Server-sent-events parser
├── PLAN.md               # Phase checklist
└── Cargo.toml            # Rust workspace
```

## Prerequisites

- [Rust](https://rustup.rs/) (1.77.2+)
- Node.js 18+
- An [OpenRouter](https://openrouter.ai) API key (free to create). Free models are limited to 20 requests/minute and 50/day (1000/day after buying 10+ credits).

## Getting Started

```sh
# From the project root
npm --prefix src install
npm --prefix src run tauri dev
```

On first launch the Settings page opens: pick a provider, paste its API key, choose a model (OpenRouter defaults to `google/gemma-4-31b-it:free`), and press "Save and use". Use "Back to chat" to return.

## Tests

```sh
cargo test -p app
```

## License

MIT
