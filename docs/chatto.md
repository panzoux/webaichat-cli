# Chatto — OpenAI-Compatible API Frontend

Chatto is a local HTTP server that exposes your browser-connected AI providers as an OpenAI-compatible REST API.
Any tool that speaks OpenAI (opencode, Cursor, Continue.dev, etc.) can point at it with zero code changes.

## Start chatto

```bash
# Make sure browser-bridge is already running first
cargo run -p browser-bridge

# Then start chatto (in a second terminal)
cargo run -p chatto

# Custom port
cargo run -p chatto -- --port 8080

# Custom bridge URL
cargo run -p chatto -- --bridge-url ws://127.0.0.1:9527
```

Default port: **11434** (same as Ollama — opencode knows it automatically).

## Configure opencode

```bash
# In your shell or .env
export OPENAI_BASE_URL=http://127.0.0.1:11434
export OPENAI_API_KEY=chatto        # any string, not validated

opencode
```

## Configure Cursor / Continue.dev

Point the "OpenAI base URL" setting at `http://127.0.0.1:11434`.
Set the API key to any string (e.g. `chatto`).

## Configure Pi

Pi (the terminal coding agent, `@earendil-works/pi-coding-agent`) doesn't read
`OPENAI_BASE_URL` — custom providers are registered via a small extension
file instead. Use [`integrations/pi/chatto.ts`](../integrations/pi/chatto.ts):

```bash
# Global — available in every project
cp integrations/pi/chatto.ts ~/.pi/agent/extensions/chatto.ts

# Or project-local
cp integrations/pi/chatto.ts .pi/extensions/chatto.ts

pi
# then select the "chatto" provider / chatgpt or gemini model
```

The registered models set `compat: { requiresToolResultName: true, requiresAssistantAfterToolResult: true }`. Without these, pi's own tool-result continuation logic didn't reliably recognize a tool call against chatto as continuable — observed in practice as pi re-injecting the original user message as a brand-new turn immediately after a tool call succeeded, instead of just letting the model carry on, producing runaway duplicate `write`/`edit` calls. If you copy this file manually rather than symlinking, make sure you pick up this block too.

## Available Models

| Model name | Routes to |
|---|---|
| `chatgpt` | ChatGPT (`chatgpt.com`) |
| `gpt-4o`, `gpt-4`, `gpt-3.5-turbo` | ChatGPT (aliases) |
| `gemini`, `gemini-pro`, `gemini-flash` | Gemini (`gemini.google.com`) |

Anything not matched → ChatGPT as default fallback.

## API Endpoints

### `GET /v1/models`

Returns available providers in OpenAI model list format.

```bash
curl http://127.0.0.1:11434/v1/models
```

### `POST /v1/chat/completions`

Streams a response from the browser-connected AI.

```bash
curl http://127.0.0.1:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer chatto" \
  -d '{
    "model": "chatgpt",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

## How Multi-Turn Works

Chatto is a **dumb format translator** — it does not manage memory or context.

opencode (or any client) is responsible for deciding what messages to include in the `messages` array.
Chatto simply converts the array to a plain-text string and sends it to the browser:

```
[System]: You are a coding assistant.

[User]: Here is my code: ...

[Assistant]: I see the issue...

[User]: Now fix it.
```

Single-turn (one user message, no prior context) is sent as-is with no label.

This means opencode's memory plugins, context compression, and tool results all work normally — chatto is invisible to them.

## Tool Calling

ChatGPT/Gemini's web UI has no native function-calling channel — it only ever exchanges plain text. When a request includes a `tools` array (pi's `read`/`bash`/`edit`/`write`, for example), chatto:

1. Appends a description of each tool plus a calling convention to the stitched prompt: *your entire reply must be exactly `@@chatto-tool-call@@`, then `{"name": ..., "arguments": {...}}`, then `@@end-chatto-tool-call@@` — not one word before it.* This is deliberately plain text, not a markdown code fence — ChatGPT's renderer has been observed stripping fence backticks from the DOM entirely (syntax highlighting swaps them for a language-label UI badge), which made an earlier fence-based marker undetectable. Plain delimiter lines with no markdown-special characters can't be rewritten that way.
2. Watches the first ~200 characters of the model's reply. If `@@chatto-tool-call@@` doesn't show up there, it never will, and the reply streams live exactly as a normal answer would — most responses take this path. The window is generous because the model doesn't always obey "nothing else" and can add a short lead-in sentence before the marker anyway.
3. If it does, chatto buffers the (short) JSON reply fully and tries to parse it (tolerating a missing end marker or an incidental code fence around the JSON), returning a real OpenAI `tool_calls` response (`finish_reason: "tool_calls"`) instead of plain content, so the agent harness actually executes the tool.
4. **If the JSON fails to parse**, chatto tries to fix it itself before asking the model, in two layers:
   - **Deterministic repair** (instant, no round-trip): the most common failure is an unescaped `"` inside a string value — e.g. a batch script's own `if /i "..."=="..."` comparison syntax. Chatto re-scans the text as a JSON string would be parsed and escapes any quote that isn't actually closing the string, judged by what follows it (a real terminator is always followed by `,`, `:`, `}`, `]`, or end of input). It also fixes a backslash followed by a character that's never legal after `\` in JSON (e.g. `\System32`, `\cmd.exe`) — unambiguous, since there's no valid reading where that's a real escape. It deliberately leaves `\` followed by `b`/`f`/`n`/`r`/`t`/`u` alone, since that's genuinely ambiguous (a real escape vs. a Windows path segment like `\test` or `\file`) and guessing wrong risks silently corrupting otherwise-correct content — worse than leaving a real problem for the next step to catch.
   - **LLM repair round-trip**: if the deterministic pass still doesn't produce parseable JSON, chatto sends the malformed JSON back to the model with an explicit "fix this" prompt and tries to parse the corrected reply. Only if that also fails does chatto fall back to returning the original reply as plain text.
5. On the next turn, the client sends back the tool call + its `role: "tool"` result in history. Chatto reconstructs the same marker form for the prior call and labels the result `[Tool Result: <name>]:` so the model sees a coherent transcript of what it did and what happened.

This is prompt compliance, not a protocol guarantee — the model can still just answer in prose instead of calling a tool, same as any other instruction you give it. The deterministic repair is free; the LLM repair round-trip adds one extra full request (and its own web-UI latency), but only on the rare path where the first two layers didn't already fix it.

## Prerequisites

1. Browser bridge running: `cargo run -p browser-bridge`
2. Chrome extension loaded and connected (see QUICKSTART.md)
3. Target AI tab open and logged in (e.g. chatgpt.com, gemini.google.com)

## Limitations

Chatto is backed by a real browser tab driving DOM automation, not a metered API, so:

- **Latency** is whatever the web UI takes to respond — there's no low-latency path.
- **No concurrency within a provider.** Each provider (ChatGPT, Gemini) maps to a single browser tab with one in-flight conversation. If a second request hits the *same* provider while the first is still streaming, the extension's observer for the first request is torn down and replaced — the first response is silently cut short rather than queued or rejected. Different providers in different tabs (e.g. ChatGPT + Gemini at once) are independent and fine.

This app is built for one client driving one conversation per provider at a time — not a multi-tenant or high-throughput backend.
