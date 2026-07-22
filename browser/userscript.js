// ==UserScript==
// @name         AI Browser Runtime
// @namespace    http://tampermonkey.net/
// @version      0.1.0
// @description  Browser runtime for web-llm-runtime
// @author       You
// @match        https://chatgpt.com/*
// @match        https://gemini.google.com/*
// @grant        none
// @run-at       document-idle
// ==/UserScript==

(function() {
    'use strict';

    const BRIDGE_URL = 'ws://127.0.0.1:9527';
    const RECONNECT_DELAY = 3000;
    const HEARTBEAT_INTERVAL = 30000;

    let ws = null;
    let currentProvider = null;
    let currentMessageId = null;
    let heartbeatTimer = null;

    function generateId() {
        return 'msg_' + Date.now() + '_' + Math.random().toString(36).substr(2, 9);
    }

    function sendMessage(event) {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify(event));
        }
    }

    function connect() {
        console.log('[AI Runtime] Connecting to bridge...');
        ws = new WebSocket(BRIDGE_URL);

        ws.onopen = function() {
            console.log('[AI Runtime] Connected to bridge');
            startHeartbeat();
        };

        ws.onmessage = function(event) {
            try {
                const data = JSON.parse(event.data);
                handleEvent(data);
            } catch (e) {
                console.error('[AI Runtime] Error parsing message:', e);
            }
        };

        ws.onclose = function() {
            console.log('[AI Runtime] Disconnected from bridge');
            stopHeartbeat();
            setTimeout(connect, RECONNECT_DELAY);
        };

        ws.onerror = function(error) {
            console.error('[AI Runtime] WebSocket error:', error);
        };
    }

    function startHeartbeat() {
        stopHeartbeat();
        heartbeatTimer = setInterval(function() {
            sendMessage({
                type: 'Ping',
                timestamp: Date.now()
            });
        }, HEARTBEAT_INTERVAL);
    }

    function stopHeartbeat() {
        if (heartbeatTimer) {
            clearInterval(heartbeatTimer);
            heartbeatTimer = null;
        }
    }

    function handleEvent(event) {
        console.log('[AI Runtime] Received event:', event.type);

        switch (event.type) {
            case 'Ready':
                console.log('[AI Runtime] Bridge ready, version:', event.version);
                break;

            case 'SendMessage':
                handleSendMessage(event);
                break;

            case 'Cancel':
                handleCancel(event);
                break;

            case 'Pong':
                // Heartbeat response
                break;

            default:
                console.log('[AI Runtime] Unknown event type:', event.type);
        }
    }

    function handleSendMessage(event) {
        currentProvider = detectProvider();
        currentMessageId = generateId();

        if (!currentProvider) {
            sendMessage({
                type: 'Error',
                provider: event.provider,
                message: 'No supported AI provider detected on this page'
            });
            return;
        }

        sendMessage({
            type: 'MessageStart',
            provider: event.provider,
            message_id: currentMessageId
        });

        currentProvider.sendMessage(event.message, currentMessageId);
    }

    function handleCancel(event) {
        if (currentProvider) {
            currentProvider.cancel();
        }
        sendMessage({
            type: 'Cancelled',
            provider: event.provider,
            message_id: event.message_id
        });
    }

    function detectProvider() {
        const url = window.location.hostname;

        if (url.includes('chatgpt.com')) {
            return new ChatGptProvider();
        } else if (url.includes('gemini.google.com')) {
            return new GeminiProvider();
        }

        return null;
    }

    class BaseProvider {
        sendMessage(message, messageId) {
            throw new Error('Not implemented');
        }

        cancel() {
            throw new Error('Not implemented');
        }

        sendChunk(content) {
            sendMessage({
                type: 'MessageChunk',
                provider: this.getName(),
                message_id: currentMessageId,
                index: 0,
                content: content
            });
        }

        sendEnd() {
            sendMessage({
                type: 'MessageEnd',
                provider: this.getName(),
                message_id: currentMessageId
            });
        }

        sendError(message) {
            sendMessage({
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
        }

        getName() {
            return 'chatgpt';
        }

        async sendMessage(message, messageId) {
            try {
                // Find the textarea
                const textarea = document.querySelector('#prompt-textarea') ||
                                 document.querySelector('textarea[placeholder*="Message"]') ||
                                 document.querySelector('textarea');

                if (!textarea) {
                    this.sendError('Could not find input textarea');
                    return;
                }

                // Set the message
                textarea.focus();
                textarea.value = message;
                textarea.dispatchEvent(new Event('input', { bubbles: true }));

                // Wait a bit for the UI to update
                await new Promise(r => setTimeout(r, 100));

                // Find and click the send button
                const sendButton = document.querySelector('button[data-testid="send-button"]') ||
                                   document.querySelector('button[aria-label="Send prompt"]') ||
                                   document.querySelector('button:has(svg[data-testid="send-button"])');

                if (!sendButton) {
                    this.sendError('Could not find send button');
                    return;
                }

                sendButton.click();

                // Start observing the response
                this.startObserving();

            } catch (e) {
                this.sendError(e.message);
            }
        }

        startObserving() {
            this.lastContent = '';
            
            const responseContainer = document.querySelector('.markdown') ||
                                      document.querySelector('[data-message-author-role="assistant"]') ||
                                      document.querySelector('.agent-turn');

            if (!responseContainer) {
                // Wait for response container to appear
                setTimeout(() => this.startObserving(), 500);
                return;
            }

            this.observer = new MutationObserver((mutations) => {
                const currentContent = responseContainer.innerText || responseContainer.textContent;
                
                if (currentContent !== this.lastContent) {
                    const newContent = currentContent.slice(this.lastContent.length);
                    this.lastContent = currentContent;
                    
                    if (newContent) {
                        this.sendChunk(newContent);
                    }
                }
            });

            this.observer.observe(responseContainer, {
                childList: true,
                subtree: true,
                characterData: true
            });

            // Check for completion
            this.checkCompletion();
        }

        checkCompletion() {
            const checkInterval = setInterval(() => {
                const sendButton = document.querySelector('button[data-testid="send-button"]');
                const stopButton = document.querySelector('button[aria-label="Stop generating"]');
                
                if (sendButton && !stopButton) {
                    clearInterval(checkInterval);
                    this.stopObserving();
                    this.sendEnd();
                }
            }, 500);
        }

        stopObserving() {
            if (this.observer) {
                this.observer.disconnect();
                this.observer = null;
            }
        }

        cancel() {
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
        }

        getName() {
            return 'gemini';
        }

        async sendMessage(message, messageId) {
            try {
                // Find the input area
                const inputArea = document.querySelector('.ql-editor') ||
                                  document.querySelector('[contenteditable="true"]') ||
                                  document.querySelector('rich-textarea');

                if (!inputArea) {
                    this.sendError('Could not find input area');
                    return;
                }

                // Set the message
                inputArea.focus();
                inputArea.textContent = message;
                inputArea.dispatchEvent(new Event('input', { bubbles: true }));

                // Wait a bit for the UI to update
                await new Promise(r => setTimeout(r, 100));

                // Find and click the send button
                const sendButton = document.querySelector('button[aria-label="Send message"]') ||
                                   document.querySelector('.send-button');

                if (!sendButton) {
                    this.sendError('Could not find send button');
                    return;
                }

                sendButton.click();

                // Start observing the response
                this.startObserving();

            } catch (e) {
                this.sendError(e.message);
            }
        }

        startObserving() {
            this.lastContent = '';
            
            const responseContainer = document.querySelector('.model-response-text') ||
                                      document.querySelector('.response-container') ||
                                      document.querySelector('model-response');

            if (!responseContainer) {
                setTimeout(() => this.startObserving(), 500);
                return;
            }

            this.observer = new MutationObserver((mutations) => {
                const currentContent = responseContainer.innerText || responseContainer.textContent;
                
                if (currentContent !== this.lastContent) {
                    const newContent = currentContent.slice(this.lastContent.length);
                    this.lastContent = currentContent;
                    
                    if (newContent) {
                        this.sendChunk(newContent);
                    }
                }
            });

            this.observer.observe(responseContainer, {
                childList: true,
                subtree: true,
                characterData: true
            });

            this.checkCompletion();
        }

        checkCompletion() {
            const checkInterval = setInterval(() => {
                const stopButton = document.querySelector('button[aria-label="Stop generating"]');
                const inputArea = document.querySelector('.ql-editor');
                
                if (!stopButton && inputArea) {
                    clearInterval(checkInterval);
                    this.stopObserving();
                    this.sendEnd();
                }
            }, 500);
        }

        stopObserving() {
            if (this.observer) {
                this.observer.disconnect();
                this.observer = null;
            }
        }

        cancel() {
            this.stopObserving();
            const stopButton = document.querySelector('button[aria-label="Stop generating"]');
            if (stopButton) {
                stopButton.click();
            }
        }
    }

    // Connect when the script loads
    connect();
})();
