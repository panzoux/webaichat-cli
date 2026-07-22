# Architecture

## Overview

The Web LLM Runtime is a local runtime that allows any frontend to communicate with AI chat websites through a browser the user has already logged into.

## Components

### Runtime (Rust)

The main executable that handles:
- Session management
- Conversation history
- Tool execution
- Provider selection
- Prompt assembly
- Streaming
- Communication with frontends
- Communication with Browser Bridge

The Runtime MUST NOT know:
- DOM selectors
- HTML structure
- Browser APIs
- ChatGPT UI implementation

### Browser Bridge (Rust)

Separate executable that handles:
- WebSocket server
- Browser connection management
- Browser heartbeat
- Reconnection
- Version negotiation
- Message validation
- Provider routing

The Browser Bridge contains NO planner logic. It should remain thin and stable.

### Browser Extension (TypeScript)

Chrome Extension that runs in a separate context (bypassing page CSP):
- Background script handles WebSocket connection to Bridge
- Content script handles DOM operations
- Observe streaming
- Upload files
- Download files
- Report progress
- Return structured events

## Data Flow

```
1. User types command in CLI
2. CLI sends message to Runtime
3. Runtime selects provider
4. Runtime sends message to Browser Bridge
5. Browser Bridge routes to Chrome Extension (background script)
6. Background script forwards to content script
7. Content script executes DOM operations
8. Content script observes streaming response
9. Content script sends chunks back through Bridge
10. Runtime displays chunks to CLI
```

## Protocol

All communication is event-based via WebSocket:

### Runtime → Bridge → Browser
- `SendMessage` - Send a chat message
- `Cancel` - Cancel ongoing generation

### Browser → Bridge → Runtime
- `MessageStart` - Generation started
- `MessageChunk` - Streaming chunk
- `MessageEnd` - Generation complete
- `Cancelled` - Cancellation acknowledged

### Any → Any
- `Error` - Error occurred
- `Ping` - Heartbeat ping
- `Pong` - Heartbeat pong
