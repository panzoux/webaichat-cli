# Quick Start Guide

## Prerequisites

1. Rust installed (https://rustup.rs/)
2. A web browser with Tampermonkey extension
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

## Step 3: Install the Tampermonkey Script

1. Open your browser
2. Click on the Tampermonkey icon
3. Select "Create a new script"
4. Delete the default content
5. Copy the contents of `browser/userscript.js`
6. Click File → Save
7. Make sure the script is enabled

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

Make sure you have the correct website open and the Tampermonkey script is running.

### Connection refused

Make sure the Browser Bridge is running on port 9527.

### Script not working

1. Check the browser console for errors
2. Make sure the Tampermonkey script is enabled
3. Try refreshing the page

## Development

### Project Structure

```
web-llm-runtime/
├── runtime/
│   ├── core/           # Main runtime library
│   └── browser-bridge/ # WebSocket bridge
├── browser/
│   └── userscript.js   # Tampermonkey script
├── frontends/
│   └── cli/            # CLI frontend
└── docs/               # Documentation
```

### Adding a New Provider

1. Create a new file in `browser/providers/`
2. Extend the `BaseProvider` class
3. Implement the required methods
4. Add detection logic in `detectProvider()`
5. Update the userscript metadata block

### Running Tests

```bash
cargo test
```

### Building Documentation

The documentation is in the `docs/` folder. You can view it with any Markdown viewer.
