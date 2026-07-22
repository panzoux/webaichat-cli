# Browser Extension

Chrome Extension for AI Browser Runtime.

## Why Extension Instead of Tampermonkey?

ChatGPT has a strict Content Security Policy (CSP) that blocks WebSocket connections to `ws://127.0.0.1:9527/`. Tampermonkey userscripts run in the page context and are subject to these restrictions.

Chrome Extensions run in a separate context and can bypass page CSP restrictions.

## Installation

### Developer Mode (Recommended)

1. Open Chrome and navigate to `chrome://extensions/`
2. Enable "Developer mode" (top right toggle)
3. Click "Load unpacked"
4. Select the `browser-extension` folder
5. The extension icon should appear in your toolbar

### Usage

1. Start the Browser Bridge:
   ```bash
   cargo run -p browser-bridge
   ```

2. Open ChatGPT or Gemini in your browser

3. The extension automatically connects to the bridge

4. You can click the extension icon to see connection status

## How It Works

1. **Background Script** (`background.js`)
   - Runs in a separate context (not affected by page CSP)
   - Handles WebSocket connection to the bridge
   - Routes messages between bridge and content scripts

2. **Content Script** (`content.js`)
   - Injected into ChatGPT/Gemini pages
   - Handles DOM operations
   - Observes streaming responses
   - Communicates with background script via Chrome APIs

3. **Popup** (`popup.html`)
   - Shows connection status
   - Allows manual connect/disconnect

## Permissions

- `activeTab` - Access to the current tab
- `scripting` - Inject content scripts
- `tabs` - Query open tabs
- `host_permissions` - Access to ChatGPT/Gemini URLs

## Troubleshooting

### Extension not connecting

1. Check if the Browser Bridge is running on port 9527
2. Click the extension icon and check status
3. Check Chrome DevTools console for errors

### Content script not working

1. Open DevTools (F12) on the ChatGPT/Gemini page
2. Check Console for `[AI Runtime]` messages
3. Look for specific error messages

### CSP errors in console

This should NOT happen with the extension. If you see CSP errors, the extension may not be properly loaded. Try:
1. Disable and re-enable the extension
2. Reload the ChatGPT/Gemini page
3. Check that you're using the extension, not Tampermonkey
