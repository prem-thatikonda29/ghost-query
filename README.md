# Ghost Query

A Tauri desktop app for chatting with any OpenAI-compatible model provider (OpenRouter by default) — streaming responses, a global hotkey, and conversation history. No server: the app talks to the provider directly and keeps your API key in the OS keychain.

## Features

- **Bring your own key** — Works with OpenRouter (including its free `:free` models), or any OpenAI-compatible endpoint such as Ollama, Groq or OpenAI. Set the base URL, model and key in the in-app Settings.
- **Key stays in the keychain** — Stored in the macOS Keychain / Windows Credential Manager. The UI can save or remove it but never reads it back, and it is never written to disk by the app.
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
│       ├── components/   # UI primitives (chat input, settings panel, etc.)
│       └── lib/          # Utilities
├── src-tauri/            # Tauri/Rust backend
│   └── src/
│       ├── lib.rs        # App setup, commands, hotkey
│       ├── provider.rs   # OpenAI-compatible streaming client
│       ├── keystore.rs   # API key in the OS keychain
│       ├── settings.rs   # Base URL / model (non-secret) settings
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

On first launch the Settings panel opens: paste your API key, pick a model (the default is `google/gemma-4-31b-it:free`), and save.

## Tests

```sh
cargo test -p app
```

## License

MIT
