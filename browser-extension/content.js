// Content script - runs in the page context
// Handles DOM operations for AI providers

(function() {
  'use strict';

  console.log('[AI Runtime] Content script initialized');

  let currentProvider = null;
  let currentMessageId = null;

  function generateId() {
    return 'msg_' + Date.now() + '_' + Math.random().toString(36).substr(2, 9);
  }

  function sendToBackground(event) {
    console.log('[AI Runtime] Sending to background:', event.type);
    chrome.runtime.sendMessage(event, (response) => {
      console.log('[AI Runtime] Background response:', response);
    });
  }

  function detectProvider() {
    const url = window.location.hostname;
    console.log('[AI Runtime] Detecting provider for URL:', url);

    if (url.includes('chatgpt.com')) {
      console.log('[AI Runtime] Detected ChatGPT');
      return new ChatGptProvider();
    } else if (url.includes('gemini.google.com')) {
      console.log('[AI Runtime] Detected Gemini');
      return new GeminiProvider();
    }

    console.log('[AI Runtime] No provider detected');
    return null;
  }

  // Listen for messages from background script (register listener exactly once per window)
  if (!window.__aiRuntimeListenerRegistered) {
    window.__aiRuntimeListenerRegistered = true;

    chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
      console.log('[AI Runtime] Received message:', message.type, message);

      if (message.type === 'SendMessage') {
        console.log('[AI Runtime] Handling SendMessage');
        handleSendMessage(message);
      } else if (message.type === 'Cancel') {
        console.log('[AI Runtime] Handling Cancel');
        handleCancel(message);
      }

      sendResponse({ received: true });
      return true;
    });
  }

  function handleSendMessage(event) {
    console.log('[AI Runtime] handleSendMessage called with:', event);

    // Stop active provider observer if one is running
    if (currentProvider && typeof currentProvider.stopObserving === 'function') {
      console.log('[AI Runtime] Cleaning up active provider observer');
      currentProvider.stopObserving();
    }

    currentProvider = detectProvider();
    currentMessageId = generateId();

    if (!currentProvider) {
      console.log('[AI Runtime] No provider detected, sending error');
      sendToBackground({
        type: 'Error',
        provider: event.provider,
        message: 'No supported AI provider detected on this page'
      });
      return;
    }

    console.log('[AI Runtime] Sending MessageStart');
    sendToBackground({
      type: 'MessageStart',
      provider: event.provider,
      message_id: currentMessageId
    });

    console.log('[AI Runtime] Calling provider.sendMessage');
    currentProvider.sendMessage(event.message, currentMessageId);
  }

  function handleCancel(event) {
    console.log('[AI Runtime] handleCancel called');
    if (currentProvider) {
      currentProvider.cancel();
    }
    sendToBackground({
      type: 'Cancelled',
      provider: event.provider,
      message_id: event.message_id
    });
  }

  // Never let a diffed chunk end with an unpaired UTF-16 high surrogate (the
  // first half of an emoji/astral character). JSON.stringify escapes a lone
  // surrogate as literal "\uXXXX" text, which serde_json on the bridge server
  // rejects as invalid ("unexpected end of hex escape" / "lone leading
  // surrogate"). Clamping here holds the surrogate back so it's included
  // whole with its pair on the next diff, a poll tick (200ms) later.
  function clampTrailingSurrogate(text) {
    if (text.length > 0) {
      const code = text.charCodeAt(text.length - 1);
      if (code >= 0xd800 && code <= 0xdbff) {
        return text.slice(0, -1);
      }
    }
    return text;
  }

  // How long to wait for the DOM to go quiet before reading it. Markdown/code
  // renderers (syntax highlighting, fence-marker cleanup) rewrite their output
  // across several back-to-back mutations as a message streams in; reading
  // immediately on every mutation risks catching a transient, not-yet-final
  // state (e.g. a code fence's raw ```lang marker before it's swapped for the
  // highlighted block).
  const SETTLE_DELAY_MS = 150;

  class BaseProvider {
    sendChunk(content, index) {
      console.log('[AI Runtime] sendChunk:', content.substring(0, 50));
      sendToBackground({
        type: 'MessageChunk',
        provider: this.getName(),
        message_id: currentMessageId,
        index: index || 0,
        content: content
      });
    }

    sendEnd() {
      console.log('[AI Runtime] sendEnd');
      sendToBackground({
        type: 'MessageEnd',
        provider: this.getName(),
        message_id: currentMessageId
      });
    }

    sendError(message) {
      console.log('[AI Runtime] sendError:', message);
      sendToBackground({
        type: 'Error',
        provider: this.getName(),
        message: message
      });
    }
  }

  class ChatGptProvider extends BaseProvider {
    constructor() {
      super();
      this.observer = null;
      this.pollingInterval = null;
      this.lastContent = '';
      this.chunkIndex = 0;
      this.initialAssistantCount = 0;
    }

    getName() {
      return 'chatgpt';
    }

    getLatestAssistantContainer() {
      // Modern ChatGPT selectors
      const containers = document.querySelectorAll('[data-message-author-role="assistant"]');
      if (containers.length > 0) {
        return containers[containers.length - 1];
      }

      const markdowns = document.querySelectorAll('.markdown');
      if (markdowns.length > 0) {
        return markdowns[markdowns.length - 1];
      }

      const articles = document.querySelectorAll('article');
      if (articles.length > 0) {
        return articles[articles.length - 1];
      }

      return null;
    }

    async sendMessage(message, messageId) {
      console.log('[AI Runtime] ChatGPT sendMessage:', message);

      try {
        // Record initial count of assistant messages
        const initialContainers = document.querySelectorAll('[data-message-author-role="assistant"]');
        this.initialAssistantCount = initialContainers.length;

        // Find the input element
        console.log('[AI Runtime] Waiting for input textarea...');
        let textarea = await this.waitForElement('#prompt-textarea', 8000) ||
                       document.querySelector('textarea') ||
                       document.querySelector('div[contenteditable="true"]');

        if (!textarea) {
          console.log('[AI Runtime] Textarea not found');
          this.sendError('Could not find input element (#prompt-textarea)');
          return;
        }

        console.log('[AI Runtime] Found input element');

        // Focus & clear/type
        textarea.focus();
        textarea.click();
        await new Promise(r => setTimeout(r, 150));

        // Clear existing ProseMirror/textarea content
        document.execCommand('selectAll', false, null);
        document.execCommand('delete', false, null);
        await new Promise(r => setTimeout(r, 50));

        // Use execCommand for ProseMirror/contenteditable compatibility
        document.execCommand('insertText', false, message);

        // Fallback for native textarea value
        if (textarea.tagName === 'TEXTAREA' && textarea.value !== message) {
          textarea.value = message;
        }

        textarea.dispatchEvent(new Event('input', { bubbles: true }));
        textarea.dispatchEvent(new Event('change', { bubbles: true }));

        await new Promise(r => setTimeout(r, 300));

        // Locate send button
        console.log('[AI Runtime] Looking for send button...');
        let sendButton = null;
        const startTime = Date.now();

        while (Date.now() - startTime < 8000) {
          sendButton = document.querySelector('button[data-testid="send-button"]') ||
                       document.querySelector('button[data-testid="fruitjuice-send-button"]') ||
                       document.querySelector('button[aria-label*="Send"]');

          if (sendButton && !sendButton.disabled) break;
          await new Promise(r => setTimeout(r, 200));
        }

        if (sendButton && !sendButton.disabled) {
          console.log('[AI Runtime] Clicking send button');
          sendButton.click();
        } else {
          console.log('[AI Runtime] Send button disabled/not found, dispatching Enter key');
          const enterEvent = new KeyboardEvent('keydown', {
            key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true
          });
          textarea.dispatchEvent(enterEvent);
        }

        // Start observing response
        console.log('[AI Runtime] Starting response observer');
        this.startObserving();

      } catch (e) {
        console.error('[AI Runtime] Error in sendMessage:', e);
        this.sendError(e.message);
      }
    }

    async waitForElement(selector, timeout) {
      const start = Date.now();
      while (Date.now() - start < timeout) {
        const el = document.querySelector(selector);
        if (el) return el;
        await new Promise(r => setTimeout(r, 100));
      }
      return null;
    }

    startObserving() {
      this.lastContent = '';
      this.chunkIndex = 0;
      this.debounceTimer = null;
      let hasStartedStream = false;

      const mainContainer = document.querySelector('main') || document.body;

      const processContentUpdate = () => {
        const currentContainer = this.getLatestAssistantContainer();
        if (!currentContainer) return;

        const currentContainersCount = document.querySelectorAll('[data-message-author-role="assistant"]').length;

        // If a new message node appeared, reset lastContent baseline for the new container
        if (!hasStartedStream && currentContainersCount > this.initialAssistantCount) {
          hasStartedStream = true;
          this.lastContent = '';
        }

        const currentContent = clampTrailingSurrogate(currentContainer.innerText || currentContainer.textContent || '');
        if (currentContent === this.lastContent) return;

        if (currentContent.startsWith(this.lastContent)) {
          const newContent = currentContent.slice(this.lastContent.length);
          this.lastContent = currentContent;

          if (newContent) {
            hasStartedStream = true;
            this.chunkIndex++;
            console.log('[AI Runtime] Chunk #', this.chunkIndex, ':', newContent.substring(0, 40));
            this.sendChunk(newContent, this.chunkIndex);
          }
        } else {
          // The DOM was rewritten, not just appended to (e.g. a code block
          // just got swapped from raw markdown to syntax-highlighted HTML).
          // We can't retract chunks already streamed, so resync silently
          // instead of sending a duplicate/garbled one.
          console.log('[AI Runtime] Non-append DOM change detected, resyncing without emitting a chunk');
          this.lastContent = currentContent;
        }
      };

      const scheduleProcess = () => {
        if (this.debounceTimer) clearTimeout(this.debounceTimer);
        this.debounceTimer = setTimeout(() => {
          this.debounceTimer = null;
          processContentUpdate();
        }, SETTLE_DELAY_MS);
      };

      // MutationObserver on full main chat container
      this.observer = new MutationObserver(() => {
        scheduleProcess();
      });

      this.observer.observe(mainContainer, {
        childList: true,
        subtree: true,
        characterData: true
      });

      // Polling backup every 200ms, debounced through the same settle window
      this.pollingInterval = setInterval(() => {
        scheduleProcess();
      }, 200);

      // Completion check
      this.checkCompletion(hasStartedStream);
    }

    checkCompletion() {
      let completionChecked = false;
      let noChunkCount = 0;
      let lastChunkIndex = 0;
      const startTime = Date.now();

      const checkInterval = setInterval(() => {
        const elapsed = Date.now() - startTime;

        if (this.chunkIndex > 0) {
          if (this.chunkIndex === lastChunkIndex) {
            noChunkCount++;
          } else {
            lastChunkIndex = this.chunkIndex;
            noChunkCount = 0;
          }

          // If no new chunks received for 2.5 seconds (5 consecutive checks), stream has completed!
          if (noChunkCount >= 5 && !completionChecked) {
            console.log('[AI Runtime] Stream completed (inactivity detection)');
            completionChecked = true;
            clearInterval(checkInterval);
            this.stopObserving();
            this.sendEnd();
            return;
          }
        }

        if (elapsed > 35000 && !completionChecked) {
          console.log('[AI Runtime] Stream timeout reached');
          completionChecked = true;
          clearInterval(checkInterval);
          this.stopObserving();
          if (this.chunkIndex > 0) {
            this.sendEnd();
          } else {
            this.sendError('Timed out waiting for ChatGPT response.');
          }
        }
      }, 500);
    }

    stopObserving() {
      if (this.observer) {
        this.observer.disconnect();
        this.observer = null;
      }
      if (this.pollingInterval) {
        clearInterval(this.pollingInterval);
        this.pollingInterval = null;
      }
      if (this.debounceTimer) {
        clearTimeout(this.debounceTimer);
        this.debounceTimer = null;
      }
    }

    cancel() {
      console.log('[AI Runtime] Cancelling');
      this.stopObserving();
      const stopButton = document.querySelector('button[aria-label="Stop generating"]');
      if (stopButton) {
        stopButton.click();
      }
    }
  }

  class GeminiProvider extends BaseProvider {
    constructor() {
      super();
      this.observer = null;
      this.lastContent = '';
      this.chunkIndex = 0;
      this.debounceTimer = null;
    }

    getName() {
      return 'gemini';
    }

    async sendMessage(message, messageId) {
      console.log('[AI Runtime] Gemini sendMessage:', message);

      try {
        // Wait for input area
        console.log('[AI Runtime] Waiting for input area...');
        await this.waitForElement('.ql-editor, [contenteditable="true"]', 10000);

        const inputArea = document.querySelector('.ql-editor') ||
                          document.querySelector('[contenteditable="true"]');

        if (!inputArea) {
          console.log('[AI Runtime] Input area not found');
          this.sendError('Could not find input area');
          return;
        }

        console.log('[AI Runtime] Found input area');

        // Set the message
        inputArea.focus();
        inputArea.textContent = message;
        inputArea.dispatchEvent(new Event('input', { bubbles: true }));

        // Wait for UI to update
        await new Promise(r => setTimeout(r, 500));

        // Find and click send button
        console.log('[AI Runtime] Waiting for send button...');
        const sendButton = await this.waitForElement('button[aria-label="Send message"]', 5000);

        if (!sendButton) {
          console.log('[AI Runtime] Send button not found');
          this.sendError('Could not find send button');
          return;
        }

        console.log('[AI Runtime] Found send button, clicking');

        sendButton.click();

        // Start observing
        console.log('[AI Runtime] Starting to observe response');
        this.startObserving();

      } catch (e) {
        console.error('[AI Runtime] Error in sendMessage:', e);
        this.sendError(e.message);
      }
    }

    async waitForElement(selector, timeout) {
      const start = Date.now();
      while (Date.now() - start < timeout) {
        const el = document.querySelector(selector);
        if (el) return el;
        await new Promise(r => setTimeout(r, 100));
      }
      return null;
    }

    startObserving() {
      this.lastContent = '';
      this.chunkIndex = 0;

      this.waitForResponse().then(container => {
        if (!container) {
          console.log('[AI Runtime] Response container not found');
          this.sendError('Response container not found');
          return;
        }

        console.log('[AI Runtime] Found response container, starting observer');

        const processContentUpdate = () => {
          const currentContent = clampTrailingSurrogate(container.innerText || container.textContent || '');
          if (currentContent === this.lastContent) return;

          if (currentContent.startsWith(this.lastContent)) {
            const newContent = currentContent.slice(this.lastContent.length);
            this.lastContent = currentContent;

            if (newContent) {
              this.chunkIndex++;
              this.sendChunk(newContent, this.chunkIndex);
            }
          } else {
            // Non-append DOM rewrite (e.g. markdown/code re-render). Can't
            // retract already-streamed chunks — resync without emitting.
            console.log('[AI Runtime] Non-append DOM change detected, resyncing without emitting a chunk');
            this.lastContent = currentContent;
          }
        };

        this.observer = new MutationObserver(() => {
          if (this.debounceTimer) clearTimeout(this.debounceTimer);
          this.debounceTimer = setTimeout(() => {
            this.debounceTimer = null;
            processContentUpdate();
          }, SETTLE_DELAY_MS);
        });

        this.observer.observe(container, {
          childList: true,
          subtree: true,
          characterData: true
        });

        this.checkCompletion();
      });
    }

    async waitForResponse(timeout = 10000) {
      const start = Date.now();
      while (Date.now() - start < timeout) {
        const container = document.querySelector('.model-response-text') ||
                          document.querySelector('.response-container');
        if (container) {
          console.log('[AI Runtime] Found response container');
          return container;
        }
        await new Promise(r => setTimeout(r, 500));
      }
      return null;
    }

    checkCompletion() {
      console.log('[AI Runtime] Checking for completion');
      const checkInterval = setInterval(() => {
        const stopButton = document.querySelector('button[aria-label="Stop generating"]');
        const inputArea = document.querySelector('.ql-editor');

        if (!stopButton && inputArea) {
          console.log('[AI Runtime] Generation complete');
          clearInterval(checkInterval);
          this.stopObserving();
          this.sendEnd();
        }
      }, 500);

      setTimeout(() => {
        clearInterval(checkInterval);
        this.stopObserving();
        this.sendEnd();
      }, 60000);
    }

    stopObserving() {
      if (this.observer) {
        this.observer.disconnect();
        this.observer = null;
      }
      if (this.debounceTimer) {
        clearTimeout(this.debounceTimer);
        this.debounceTimer = null;
      }
    }

    cancel() {
      console.log('[AI Runtime] Cancelling');
      this.stopObserving();
      const stopButton = document.querySelector('button[aria-label="Stop generating"]');
      if (stopButton) {
        stopButton.click();
      }
    }
  }

  // Auto-detect and initialize
  console.log('[AI Runtime] Content script initializing');
  currentProvider = detectProvider();
  if (currentProvider) {
    console.log('[AI Runtime] Provider detected:', currentProvider.getName());
  }

})();
