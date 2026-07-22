# Quick Start Guide

## Prerequisites

1. Rust installed (https://rustup.rs/)
2. Google Chrome or Chromium-based browser
3. Logged into ChatGPT or Gemini in your browser

## Step 1: Build the Project

```bash
cargo build --release
```

## Step 2: Start the Browser Bridge

Open a terminal and run:

```bash
cargo run -p browser-bridge
```

This starts the WebSocket server on port 9527.

## Step 3: Install the Browser Extension

1. Open Chrome and navigate to `chrome://extensions/`
2. Enable "Developer mode" (top right toggle)
3. Click "Load unpacked"
4. Select the `browser-extension` folder
5. The extension icon should appear in your toolbar

## Step 4: Open an AI Website

Navigate to one of the supported websites:
- https://chatgpt.com
- https://gemini.google.com

Log in normally with your credentials.

## Step 5: Send a Message

Open another terminal and run:

```bash
# For ChatGPT
cargo run -p web-llm-cli -- send --provider chatgpt --message "Hello, how are you?"

# For Gemini
cargo run -p web-llm-cli -- send --provider gemini --message "Hello, how are you?"
```

## Expected Output

The response will stream to your terminal as the AI generates it.

## Troubleshooting

### "No supported AI provider detected"

Make sure you have the correct website open and the Chrome Extension is installed.

### Connection refused

Make sure the Browser Bridge is running on port 9527.

### Extension not working

1. Check the extension icon for connection status
2. Open DevTools (F12) and check Console for `[AI Runtime]` messages
3. Make sure the extension is enabled in `chrome://extensions/`
4. Try refreshing the page

## Development

### Project Structure

```
web-llm-runtime/
├── runtime/
│   ├── core/           # Main runtime library
│   └── browser-bridge/ # WebSocket bridge
├── browser-extension/  # Chrome Extension
│   ├── manifest.json
│   ├── background.js
│   ├── content.js
│   └── popup.html
├── frontends/
│   └── cli/            # CLI frontend
└── docs/               # Documentation
```

### Adding a New Provider

1. Add a new class in `browser-extension/content.js`
2. Extend the `BaseProvider` class
3. Implement the required methods
4. Add detection logic in `detectProvider()`
5. Update the extension's `manifest.json` to include the new URL pattern

### Running Tests

```bash
cargo test
```

### Building Documentation

The documentation is in the `docs/` folder. You can view it with any Markdown viewer.
