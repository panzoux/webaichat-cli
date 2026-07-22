const statusEl = document.getElementById('status');
const connectBtn = document.getElementById('connect');
const disconnectBtn = document.getElementById('disconnect');

function updateStatus() {
  chrome.runtime.sendMessage({ type: 'GetStatus' }, (response) => {
    if (chrome.runtime.lastError) {
      statusEl.textContent = 'Extension error';
      statusEl.className = 'status disconnected';
      return;
    }

    if (response && response.connected) {
      statusEl.textContent = 'Connected to bridge';
      statusEl.className = 'status connected';
    } else {
      statusEl.textContent = 'Disconnected from bridge';
      statusEl.className = 'status disconnected';
    }
  });
}

connectBtn.addEventListener('click', () => {
  chrome.runtime.sendMessage({ type: 'Connect' }, () => {
    updateStatus();
  });
});

disconnectBtn.addEventListener('click', () => {
  chrome.runtime.sendMessage({ type: 'Disconnect' }, () => {
    updateStatus();
  });
});

// Initial status check
updateStatus();

// Periodic status updates
setInterval(updateStatus, 5000);
