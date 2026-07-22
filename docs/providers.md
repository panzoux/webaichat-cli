# Providers

## Overview

Providers are website-specific implementations that handle DOM operations for each AI chat website.

## Provider Interface

Each provider implements:

```typescript
interface Provider {
    getName(): string;
    sendMessage(message: string, messageId: string): Promise<void>;
    cancel(): void;
}
```

## Built-in Providers

### ChatGPT Provider

**File:** `browser-extension/content.js` (ChatGptProvider class)

**Selectors:**
- Input textarea: `#prompt-textarea`, `textarea[placeholder*="Message"]`
- Send button: `button[data-testid="send-button"]`
- Response container: `.markdown`, `[data-message-author-role="assistant"]`
- Stop button: `button[aria-label="Stop generating"]`

**Behavior:**
1. Focus textarea
2. Set message value
3. Dispatch input event
4. Click send button
5. Observe response with MutationObserver
6. Send chunks as they appear
7. Detect completion when send button re-enables

### Gemini Provider

**File:** `browser-extension/content.js` (GeminiProvider class)

**Selectors:**
- Input area: `.ql-editor`, `[contenteditable="true"]`
- Send button: `button[aria-label="Send message"]`
- Response container: `.model-response-text`, `.response-container`
- Stop button: `button[aria-label="Stop generating"]`

**Behavior:**
1. Focus input area
2. Set text content
3. Dispatch input event
4. Click send button
5. Observe response with MutationObserver
6. Send chunks as they appear
7. Detect completion when stop button disappears

## Adding a New Provider

1. Add a new class in `browser-extension/content.js`
2. Extend `BaseProvider` class
3. Implement required methods
4. Add detection logic in `detectProvider()`
5. Update `manifest.json` to include the new URL pattern

### Example

```javascript
class ClaudeProvider extends BaseProvider {
    constructor() {
        super();
        this.observer = null;
        this.lastContent = '';
    }

    getName() {
        return 'claude';
    }

    async sendMessage(message, messageId) {
        // Implement DOM operations
    }

    cancel() {
        // Stop generation
    }
}
```

## Base Provider

All providers extend `BaseProvider` which provides:

- `sendChunk(content)` - Send a streaming chunk
- `sendEnd()` - Signal generation complete
- `sendError(message)` - Send error message

## Provider Detection

The runtime detects providers based on the current URL:

- `chatgpt.com` → ChatGPT Provider
- `gemini.google.com` → Gemini Provider

Add new URL patterns in `detectProvider()` function.

## DOM Selector Updates

When a website updates its UI, only the provider file needs to be updated. The Runtime and Browser Bridge remain unchanged.

## Streaming Observation

Providers use `MutationObserver` to watch for response changes:

1. Find response container
2. Create MutationObserver
3. Watch for childList, subtree, and characterData changes
4. Extract new content
5. Send chunks via `sendChunk()`
6. Stop observing when generation completes
