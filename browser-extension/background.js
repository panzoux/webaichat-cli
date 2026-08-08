// Background service worker - handles WebSocket connection to bridge
// This runs in a separate context and is NOT restricted by page CSP

const BRIDGE_URL = 'ws://127.0.0.1:9527';
const RECONNECT_DELAY = 3000;
const HEARTBEAT_INTERVAL = 30000;

let ws = null;
let heartbeatTimer = null;
let connectedTabs = new Map(); // tabId -> provider name

// Connect to bridge
function connectToBridge() {
  console.log('[AI Runtime BG] Connecting to bridge...');
  ws = new WebSocket(BRIDGE_URL);

  ws.onopen = function() {
    console.log('[AI Runtime BG] Connected to bridge');
    // Send Connect event to register with bridge
    sendToBridge({
      type: 'Connect',
      provider: 'chatgpt',
      version: '0.1.0'
    });
    startHeartbeat();
  };

  ws.onmessage = function(event) {
    console.log('[AI Runtime BG] Raw message received:', event.data);
    try {
      const data = JSON.parse(event.data);
      console.log('[AI Runtime BG] Parsed event:', data.type);
      handleEvent(data);
    } catch (e) {
      console.error('[AI Runtime BG] Error parsing message:', e);
    }
  };

  ws.onclose = function() {
    console.log('[AI Runtime BG] Disconnected from bridge');
    stopHeartbeat();
    setTimeout(connectToBridge, RECONNECT_DELAY);
  };

  ws.onerror = function(error) {
    console.error('[AI Runtime BG] WebSocket error:', error);
  };
}

function startHeartbeat() {
  stopHeartbeat();
  heartbeatTimer = setInterval(function() {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({
        type: 'Ping',
        timestamp: Date.now()
      }));
    }
  }, HEARTBEAT_INTERVAL);
}

function stopHeartbeat() {
  if (heartbeatTimer) {
    clearInterval(heartbeatTimer);
    heartbeatTimer = null;
  }
}

function sendToBridge(event) {
  console.log('[AI Runtime BG] Sending to bridge:', event.type);
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(event));
  } else {
    console.error('[AI Runtime BG] WebSocket not open, state:', ws ? ws.readyState : 'null');
  }
}

function handleEvent(event) {
  console.log('[AI Runtime BG] Handling event:', event.type);

  switch (event.type) {
    case 'Ready':
      console.log('[AI Runtime BG] Bridge ready, version:', event.version);
      break;

    case 'SendMessage':
      console.log('[AI Runtime BG] Received SendMessage for provider:', event.provider);
      handleSendMessage(event);
      break;

    case 'Cancel':
      console.log('[AI Runtime BG] Received Cancel');
      handleCancel(event);
      break;

    case 'Pong':
      console.log('[AI Runtime BG] Received Pong');
      break;

    default:
      console.log('[AI Runtime BG] Unknown event type:', event.type);
  }
}

async function handleSendMessage(event) {
  console.log('[AI Runtime BG] handleSendMessage called');

  // Find the tab with the matching provider
  const tabs = await chrome.tabs.query({});
  console.log('[AI Runtime BG] Found tabs:', tabs.length);

  let targetTab = null;

  for (const tab of tabs) {
    console.log('[AI Runtime BG] Tab:', tab.url);
    if (event.provider === 'chatgpt' && tab.url && tab.url.includes('chatgpt.com')) {
      targetTab = tab;
      console.log('[AI Runtime BG] Found ChatGPT tab:', tab.id);
      break;
    }
    if (event.provider === 'gemini' && tab.url && tab.url.includes('gemini.google.com')) {
      targetTab = tab;
      console.log('[AI Runtime BG] Found Gemini tab:', tab.id);
      break;
    }
  }

  if (!targetTab) {
    console.log('[AI Runtime BG] No tab found for provider:', event.provider);
    sendToBridge({
      type: 'Error',
      provider: event.provider,
      message: `No ${event.provider} tab found`
    });
    return;
  }

  // Track tab, un-minimize window if minimized, and focus tab to prevent throttling
  connectedTabs.set(targetTab.id, event.provider);
  try {
    if (targetTab.windowId !== undefined) {
      // Request window restore
      await chrome.windows.update(targetTab.windowId, { state: 'normal', focused: true });

      // Poll until window is actually in 'normal' state (not still 'minimized').
      // Chrome's update() resolves before the OS finishes restoring the window,
      // so content script timers would still be throttled if we send immediately.
      for (let attempts = 0; attempts < 20; attempts++) {
        await new Promise((r) => setTimeout(r, 150));
        try {
          const win = await chrome.windows.get(targetTab.windowId);
          if (win.state === 'normal' || win.state === 'maximized') {
            console.log('[AI Runtime BG] Window restored to state:', win.state);
            break;
          }
          console.log('[AI Runtime BG] Waiting for window restore, state:', win.state);
        } catch (_) {
          break;
        }
      }

      // Extra settle time so Chrome fully lifts throttling on the tab's timers
      await new Promise((r) => setTimeout(r, 300));
    }
    await chrome.tabs.update(targetTab.id, { active: true });
  } catch (e) {
    console.log('[AI Runtime BG] Could not un-minimize/activate window:', e);
  }

  // Now the window is restored — safe to send to content script
  async function sendToContentScript() {
    try {
      console.log('[AI Runtime BG] Sending message to content script in tab:', targetTab.id);
      chrome.tabs.sendMessage(targetTab.id, {
        type: 'SendMessage',
        provider: event.provider,
        message: event.message
      }, (response) => {
        if (chrome.runtime.lastError) {
          console.log('[AI Runtime BG] Script fallback executing script:', chrome.runtime.lastError.message);
          chrome.scripting.executeScript({
            target: { tabId: targetTab.id },
            files: ['content.js']
          }).then(() => {
            chrome.tabs.sendMessage(targetTab.id, {
              type: 'SendMessage',
              provider: event.provider,
              message: event.message
            });
          });
        }
      });
    } catch (e) {
      console.error('[AI Runtime BG] Error sending message to tab:', e);
      sendToBridge({
        type: 'Error',
        provider: event.provider,
        message: e.message
      });
    }
  }

  await sendToContentScript();
}

function handleCancel(event) {
  console.log('[AI Runtime BG] handleCancel called');
  // Find tab and send cancel
  for (const [tabId, provider] of connectedTabs) {
    if (provider === event.provider) {
      chrome.tabs.sendMessage(tabId, {
        type: 'Cancel',
        provider: event.provider,
        message_id: event.message_id
      });
      break;
    }
  }
}

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log('[AI Runtime BG] Received from content script:', message.type);

  // Forward to bridge
  sendToBridge(message);

  // Show notification when response is complete
  if (message.type === 'MessageEnd') {
    const provider = message.provider || 'AI';
    chrome.notifications.create({
      type: 'basic',
      title: `${provider} Response Ready`,
      message: 'Check the CLI for the response',
      priority: 2
    }).catch(() => {
      // Notifications might not be available
    });
  }

  sendResponse({ received: true });
  return true;
});

// Start connection
console.log('[AI Runtime BG] Extension loaded, connecting to bridge...');
connectToBridge();
