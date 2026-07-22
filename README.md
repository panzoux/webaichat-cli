# Web LLM Runtime

A browser-backed multi-provider LLM runtime that allows any frontend (CLI, VSCode, Pi, etc.) to communicate with AI chat websites (ChatGPT, Gemini, Claude, DuckDuckGo AI) through a browser that the user has already logged into.

## Architecture

```
User → Frontend (CLI/VSCode/Pi) → Runtime (Rust) → Browser Bridge (Rust) → Chrome Extension → AI Websites
```

## Key Principles

- **No credential storage** - Uses existing browser sessions
- **No login automation** - User logs in manually
- **Modular** - Browser implementation swappable (Chrome Extension, Playwright, CDP)
- **Provider-agnostic** - Runtime doesn't know about DOM selectors

## Components

### Runtime (Rust)
- Session management
- Conversation history
- Tool execution
- Provider selection
- Streaming

### Browser Bridge (Rust)
- WebSocket server
- Browser connection management
- Heartbeat and reconnection
- Message routing

### Browser Extension (TypeScript)
- DOM operations
- Streaming observation
- File upload/download
- Provider-specific UI operations
- Bypasses page CSP restrictions

## Quick Start

### 1. Build the Runtime

```bash
cargo build --release
```

### 2. Start the Browser Bridge

```bash
cargo run -p browser-bridge
```

### 3. Install the Browser Extension

1. Open Chrome and navigate to `chrome://extensions/`
2. Enable "Developer mode" (top right toggle)
3. Click "Load unpacked"
4. Select the `browser-extension` folder
5. The extension icon should appear in your toolbar

### 4. Open an AI Website

Navigate to chatgpt.com or gemini.google.com and log in normally.

### 5. Send a Message

```bash
cargo run -p web-llm-cli -- send --provider chatgpt --message "Hello, how are you?"
```

## Development

### Phase 1: Minimal POC
- [x] Project structure
- [ ] WebSocket communication
- [ ] ChatGPT provider
- [ ] CLI interface

### Phase 2: Provider Abstraction
- [ ] Multiple providers
- [ ] Provider registry
- [ ] Session management

### Phase 3: Streaming & Reconnect
- [ ] Heartbeat system
- [ ] Reconnection logic
- [ ] Cancel/interrupt support

### Phase 4: Tool Registry
- [ ] Filesystem tool
- [ ] Shell tool
- [ ] Python tool

### Phase 5: Planning
- [ ] Planner module
- [ ] Autonomous execution

## Configuration

### runtime.toml
```toml
[general]
default_provider = "chatgpt"

[session]
storage = "memory"
max_history = 100
```

### bridge.toml
```toml
[server]
host = "127.0.0.1"
port = 9527

[heartbeat]
interval_secs = 30
timeout_secs = 10
```

## License

MIT
