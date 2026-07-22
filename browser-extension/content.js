// Content script - runs in the page context
// Handles DOM operations for AI providers

(function() {
  'use strict';

  // Prevent multiple injections
  if (window.__aiRuntimeLoaded) {
    console.log('[AI Runtime] Content script already loaded, skipping');
    return;
  }
  window.__aiRuntimeLoaded = true;

  console.log('[AI Runtime] Content script loaded');

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

  // Listen for messages from background script
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

  function handleSendMessage(event) {
    console.log('[AI Runtime] handleSendMessage called with:', event);

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
      this.lastContent = '';
      this.chunkIndex = 0;
    }

    getName() {
      return 'chatgpt';
    }

    async sendMessage(message, messageId) {
      console.log('[AI Runtime] ChatGPT sendMessage:', message);

      try {
        // Wait for page to be ready
        console.log('[AI Runtime] Waiting for textarea...');
        await this.waitForElement('#prompt-textarea', 10000);

        // Find the textarea - ChatGPT uses a ProseMirror editor
        const textarea = document.querySelector('#prompt-textarea');

        if (!textarea) {
          console.log('[AI Runtime] Textarea not found');
          this.sendError('Could not find input textarea');
          return;
        }

        console.log('[AI Runtime] Found textarea');

        // Focus the textarea
        textarea.focus();
        textarea.click();
        await new Promise(r => setTimeout(r, 200));

        // Clear existing content and type message using keyboard events
        console.log('[AI Runtime] Typing message...');

        // Select all existing text
        document.execCommand('selectAll', false, null);
        await new Promise(r => setTimeout(r, 50));

        // Delete selected text
        document.execCommand('delete', false, null);
        await new Promise(r => setTimeout(r, 50));

        // Type the message using insertText
        document.execCommand('insertText', false, message);

        await new Promise(r => setTimeout(r, 500));

        // Verify content was set
        const currentContent = textarea.textContent || textarea.innerText;
        console.log('[AI Runtime] Content after input:', currentContent.substring(0, 50));

        // Wait for send button to appear and become enabled
        console.log('[AI Runtime] Waiting for send button...');

        let sendButton = null;
        const startTime = Date.now();
        const timeout = 10000;

        while (Date.now() - startTime < timeout) {
          // Try multiple selectors
          sendButton = document.querySelector('button[data-testid="send-button"]') ||
                       document.querySelector('button[aria-label="Send prompt"]') ||
                       document.querySelector('button[aria-label="Send"]');

          if (sendButton) {
            console.log('[AI Runtime] Found send button');
            break;
          }

          // Debug: log what we see
          const buttons = document.querySelectorAll('button');
          const buttonInfo = Array.from(buttons).map(b => ({
            testid: b.getAttribute('data-testid'),
            label: b.getAttribute('aria-label'),
            disabled: b.disabled,
            text: b.textContent?.substring(0, 20)
          }));
          console.log('[AI Runtime] Available buttons:', JSON.stringify(buttonInfo.slice(0, 10)));

          await new Promise(r => setTimeout(r, 500));
        }

        if (!sendButton) {
          console.log('[AI Runtime] Send button not found after waiting');
          this.sendError('Could not find send button');
          return;
        }

        // Wait for button to be enabled
        console.log('[AI Runtime] Waiting for send button to be enabled...');
        while (sendButton.disabled && Date.now() - startTime < timeout) {
          await new Promise(r => setTimeout(r, 100));
        }

        if (sendButton.disabled) {
          console.log('[AI Runtime] Send button is disabled');
          this.sendError('Send button is disabled');
          return;
        }

        console.log('[AI Runtime] Clicking send button');

        // Click the send button
        sendButton.click();

        // Also try keyboard shortcut as backup
        await new Promise(r => setTimeout(r, 200));
        console.log('[AI Runtime] Also trying Enter key...');
        textarea.focus();
        const enterEvent = new KeyboardEvent('keydown', {
          key: 'Enter',
          code: 'Enter',
          keyCode: 13,
          which: 13,
          bubbles: true
        });
        textarea.dispatchEvent(enterEvent);

        // Start observing the response
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
      this.responseContainer = null;

      // Wait for response container to appear
      this.waitForResponse().then(container => {
        if (!container) {
          console.log('[AI Runtime] Response container not found');
          this.sendError('Response container not found');
          return;
        }

        console.log('[AI Runtime] Found response container, starting observer');
        console.log('[AI Runtime] Container tag:', container.tagName, 'class:', container.className);
        console.log('[AI Runtime] Initial content:', (container.innerText || '').substring(0, 100));

        // Store reference to container
        this.responseContainer = container;

        // Set initial content as baseline
        this.lastContent = container.innerText || container.textContent || '';

        this.observer = new MutationObserver((mutations) => {
          console.log('[AI Runtime] Mutation detected, mutations:', mutations.length);
          const currentContent = container.innerText || container.textContent || '';
          console.log('[AI Runtime] Current content length:', currentContent.length, 'Last length:', this.lastContent.length);

          if (currentContent !== this.lastContent) {
            const newContent = currentContent.slice(this.lastContent.length);
            this.lastContent = currentContent;

            if (newContent) {
              this.chunkIndex++;
              console.log('[AI Runtime] New chunk:', newContent.substring(0, 50));
              this.sendChunk(newContent, this.chunkIndex);
            }
          }
        });

        this.observer.observe(container, {
          childList: true,
          subtree: true,
          characterData: true
        });

        console.log('[AI Runtime] Observer started');

        // Also use polling as backup
        this.pollingInterval = setInterval(() => {
          if (this.responseContainer) {
            const currentContent = this.responseContainer.innerText || this.responseContainer.textContent || '';
            if (currentContent !== this.lastContent) {
              const newContent = currentContent.slice(this.lastContent.length);
              this.lastContent = currentContent;
              if (newContent) {
                this.chunkIndex++;
                console.log('[AI Runtime] Polling chunk:', newContent.substring(0, 50));
                this.sendChunk(newContent, this.chunkIndex);
              }
            }
          }
        }, 500);

        // Check for completion
        this.checkCompletion();
      });
    }

    async waitForResponse(timeout = 10000) {
      const start = Date.now();
      while (Date.now() - start < timeout) {
        // Look for assistant response containers
        const containers = document.querySelectorAll('[data-message-author-role="assistant"]');
        if (containers.length > 0) {
          // Get the last (newest) response
          console.log('[AI Runtime] Found assistant response container');
          return containers[containers.length - 1];
        }

        // Also try markdown containers
        const markdown = document.querySelector('.markdown');
        if (markdown) {
          console.log('[AI Runtime] Found markdown container');
          return markdown;
        }

        await new Promise(r => setTimeout(r, 500));
      }
      return null;
    }

    checkCompletion() {
      console.log('[AI Runtime] Checking for completion');
      let completionChecked = false;

      const checkInterval = setInterval(() => {
        const sendButton = document.querySelector('button[data-testid="send-button"]');
        const stopButton = document.querySelector('button[aria-label="Stop generating"]');

        console.log('[AI Runtime] Completion check - sendButton:', !!sendButton, 'stopButton:', !!stopButton);

        if (sendButton && !stopButton && !completionChecked) {
          // Check if we have any content
          const currentContent = this.responseContainer?.innerText || '';
          console.log('[AI Runtime] Current response content length:', currentContent.length);

          if (currentContent.length > 0) {
            // Generation complete
            console.log('[AI Runtime] Generation complete, clearing polling');
            completionChecked = true;
            clearInterval(checkInterval);
            if (this.pollingInterval) {
              clearInterval(this.pollingInterval);
            }
            this.stopObserving();
            this.sendEnd();
          }
        }
      }, 1000);

      // Safety timeout
      setTimeout(() => {
        if (!completionChecked) {
          console.log('[AI Runtime] Safety timeout reached');
          clearInterval(checkInterval);
          if (this.pollingInterval) {
            clearInterval(this.pollingInterval);
          }
          this.stopObserving();
          this.sendEnd();
        }
      }, 60000);
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

        this.observer = new MutationObserver(() => {
          const currentContent = container.innerText || container.textContent;

          if (currentContent !== this.lastContent) {
            const newContent = currentContent.slice(this.lastContent.length);
            this.lastContent = currentContent;

            if (newContent) {
              this.chunkIndex++;
              this.sendChunk(newContent, this.chunkIndex);
            }
          }
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
