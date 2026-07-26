# Ghost Query

A Tauri desktop app for chatting with Gemini and Perplexity — streaming responses, model selection, and conversation history.

## Features

- **Multi-model support** — Switch between Gemini and Perplexity models on the fly.
- **Streaming responses** — Real-time token-by-token output as the AI thinks.
- **Conversation history** — Browse, copy, or clear past conversations.
- **Markdown rendering** — Responses render with syntax-highlighted code blocks and copy buttons.
- **Proxy server** — API keys stay server-side with rate limiting per IP.

## Tech Stack

- **Desktop:** Tauri 2 (Rust + React)
- **Frontend:** React, Tailwind CSS, Framer Motion, React Markdown
- **Backend proxy:** Node.js, Express, Helmet, rate limiting
- **APIs:** Google Gemini, Perplexity

## Project Structure

```
├── src/                  # React frontend (Vite)
│   └── src/
│       ├── App.tsx       # Main chat UI
│       ├── components/   # UI primitives (chat input, model selector, etc.)
│       ├── hooks/        # Custom React hooks
│       └── lib/          # Utilities
├── src-tauri/            # Tauri/Rust backend
│   └── src/
│       └── lib.rs        # Tauri entry point
├── proxy-server/         # Express proxy for API keys + rate limiting
│   └── api/              # Serverless functions
└── Cargo.toml            # Rust workspace
```

## Prerequisites

- [Rust](https://rustup.rs/) (1.77.2+)
- Node.js 18+
- API keys for Gemini and Perplexity

## Getting Started

### 1. Set up the proxy server

```sh
cd proxy-server
npm install
cp env.example .env   # Add your GEMINI_API_KEY and PERPLEXITY_API_KEY
npm run dev
```

### 2. Run the desktop app

```sh
# From the project root
npm --prefix src install
cargo tauri dev
```

The Vite dev server starts on `localhost:5173` and Tauri loads it in a native window.

## Proxy Server

The Express proxy (`proxy-server/`) keeps API keys server-side and enforces rate limits:

| Endpoint | Method | Description |
|---|---|---|
| `/api/gemini` | POST | Chat with Gemini models |
| `/api/perplexity` | POST | Chat with Perplexity models |
| `/health` | GET | Health check |
| `/api/models` | GET | List available models |

Rate limits: 100 req/15min general, 50 req/15min Gemini, 30 req/15min Perplexity.

## Environment Variables

| Variable | Description |
|---|---|
| `GEMINI_API_KEY` | Google Gemini API key |
| `PERPLEXITY_API_KEY` | Perplexity API key |

## License

MIT
