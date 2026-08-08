/**
 * WebAIChat E2E Test Suite
 *
 * Modes:
 *   node tests/run-e2e.js          -- Mock mode (fully automated, no browser needed)
 *   node tests/run-e2e.js --live   -- Live mode (requires Chrome + extension + ChatGPT tab open)
 */

import { spawn } from 'child_process';
import path from 'path';
import net from 'net';
import WebSocket from 'ws';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const ROOT_DIR = path.resolve(__dirname, '..');
const LIVE_MODE = process.argv.includes('--live');
const BRIDGE_PORT = 9527;

// ─── Utilities ───────────────────────────────────────────────────────────────

function checkPort(port) {
  return new Promise((resolve) => {
    const socket = new net.Socket();
    socket.setTimeout(1000);
    socket.on('connect', () => { socket.destroy(); resolve(true); });
    socket.on('timeout', () => { socket.destroy(); resolve(false); });
    socket.on('error', () => { socket.destroy(); resolve(false); });
    socket.connect(port, '127.0.0.1');
  });
}

async function ensureBridgeServer() {
  const isRunning = await checkPort(BRIDGE_PORT);
  if (isRunning) {
    console.log('[E2E] Browser bridge already running on port', BRIDGE_PORT);
    return null;
  }

  console.log('[E2E] Starting browser-bridge...');
  const cargoCmd = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
  const bridgeProcess = spawn(cargoCmd, ['run', '-p', 'browser-bridge'], {
    cwd: ROOT_DIR,
    stdio: 'pipe',
  });

  bridgeProcess.stderr.on('data', (d) => {
    const line = d.toString().trim();
    if (line) console.log(`[Bridge] ${line}`);
  });

  // Wait up to 30 seconds for bridge to start
  for (let i = 0; i < 60; i++) {
    await new Promise((r) => setTimeout(r, 500));
    if (await checkPort(BRIDGE_PORT)) {
      console.log('[E2E] Bridge server started.');
      return bridgeProcess;
    }
  }
  throw new Error('Failed to start browser-bridge on port ' + BRIDGE_PORT);
}

function runCliCommand(provider, message, timeoutMs = 60000) {
  return new Promise((resolve, reject) => {
    console.log(`[E2E] CLI: send --provider ${provider} --message "${message}"`);
    const cargoCmd = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
    const proc = spawn(
      cargoCmd,
      ['run', '-p', 'web-llm-cli', '--', 'send', '--provider', provider, '--message', message],
      { cwd: ROOT_DIR }
    );

    let stdout = '';
    proc.stdout.on('data', (chunk) => {
      const text = chunk.toString();
      stdout += text;
      process.stdout.write(text);
    });
    proc.stderr.on('data', (chunk) => {
      // cargo build output goes to stderr — only log on failure
    });

    const timer = setTimeout(() => {
      proc.kill();
      reject(new Error(`CLI timed out after ${timeoutMs / 1000}s`));
    }, timeoutMs);

    proc.on('close', (code) => {
      clearTimeout(timer);
      console.log(`\n[E2E] CLI exited with code ${code}`);
      if (code === 0) resolve(stdout.trim());
      else reject(new Error(`CLI failed with exit code ${code}`));
    });
  });
}

// ─── Mock Browser Client ─────────────────────────────────────────────────────
// Simulates the browser extension over WebSocket so we can test the full
// CLI → bridge → (fake browser) → bridge → CLI pipeline without a real browser.

function startMockBrowser(provider = 'chatgpt') {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${BRIDGE_PORT}`);

    ws.on('open', () => {
      console.log('[MockBrowser] Connected to bridge');
      // Register as a browser client
      ws.send(JSON.stringify({ type: 'Connect', provider, version: '0.1.0' }));
    });

    ws.on('message', (raw) => {
      let event;
      try {
        event = JSON.parse(raw.toString());
      } catch (e) {
        console.error('[MockBrowser] JSON parse error:', e.message);
        return;
      }
      console.log('[MockBrowser] Received:', event.type);

      if (event.type === 'Ready') {
        console.log('[MockBrowser] Bridge ready — waiting for SendMessage...');
        resolve(ws); // Signal that mock browser is ready
      }

      if (event.type === 'SendMessage') {
        console.log(`[MockBrowser] Got SendMessage: "${event.message}"`);
        const msgId = 'mock_msg_' + Date.now();
        const responseText = `MOCK_RESPONSE_OK for: ${event.message}`;

        // Simulate streaming response
        setTimeout(() => {
          ws.send(JSON.stringify({ type: 'MessageStart', provider, message_id: msgId }));
        }, 100);

        // Send in chunks
        const words = responseText.split(' ');
        words.forEach((word, i) => {
          setTimeout(() => {
            ws.send(JSON.stringify({
              type: 'MessageChunk',
              provider,
              message_id: msgId,
              index: i,
              content: (i === 0 ? '' : ' ') + word,
            }));
          }, 200 + i * 80);
        });

        // End the message
        setTimeout(() => {
          ws.send(JSON.stringify({ type: 'MessageEnd', provider, message_id: msgId }));
          console.log('[MockBrowser] Sent MessageEnd');
        }, 200 + words.length * 80 + 200);
      }
    });

    ws.on('error', (e) => reject(e));
    ws.on('close', () => console.log('[MockBrowser] Connection closed'));
  });
}

// ─── Test Definitions ────────────────────────────────────────────────────────

async function runMockTests() {
  console.log('\n📋 MODE: MOCK (no real browser required)\n');

  let mockWs = null;

  try {
    mockWs = await startMockBrowser('chatgpt');
    console.log('[E2E] Mock browser ready.\n');

    // Small delay to ensure bridge has registered the mock browser
    await new Promise((r) => setTimeout(r, 500));

    // Test 1: Basic message round-trip
    console.log('─── Test 1: Basic message round-trip ───');
    const response1 = await runCliCommand('chatgpt', 'Automated E2E Test 1: Output the word E2E_VERIFIED_SUCCESS.');
    if (!response1 || response1.length === 0) throw new Error('Test 1: Empty response');
    console.log('[E2E] ✅ Test 1 PASSED — got response:', response1.substring(0, 80));

    await new Promise((r) => setTimeout(r, 500));

    // Test 2: Back-to-back + special characters (LaTeX escape)
    console.log('\n─── Test 2: Back-to-back & escape character test ───');
    const response2 = await runCliCommand('chatgpt', 'Test 2: LaTeX \\frac{a}{b} and backslash test.');
    if (!response2 || response2.length === 0) throw new Error('Test 2: Empty response');
    console.log('[E2E] ✅ Test 2 PASSED — got response:', response2.substring(0, 80));

    console.log('\n🎉 ✅ ALL MOCK TESTS PASSED!\n');
    return true;
  } finally {
    if (mockWs) {
      mockWs.close();
      console.log('[E2E] Mock browser disconnected.');
    }
  }
}

async function runLiveTests() {
  console.log('\n📋 MODE: LIVE (requires Chrome + extension + ChatGPT tab)\n');
  console.log('[E2E] Verifying bridge is reachable...');

  const isRunning = await checkPort(BRIDGE_PORT);
  if (!isRunning) {
    throw new Error('Bridge server not running on port ' + BRIDGE_PORT + '. Start it with: cargo run -p browser-bridge');
  }

  console.log('[E2E] Bridge is running. Sending CLI commands to real ChatGPT...\n');

  // Give the extension a moment
  await new Promise((r) => setTimeout(r, 2000));

  // Test 1
  console.log('─── Test 1: Basic message ───');
  const response1 = await runCliCommand('chatgpt', 'Automated E2E Live Test: Reply with the word LIVE_TEST_OK.', 90000);
  if (!response1 || response1.length === 0) throw new Error('Test 1: Empty response');
  console.log('[E2E] ✅ Test 1 PASSED');

  await new Promise((r) => setTimeout(r, 2000));

  // Test 2
  console.log('\n─── Test 2: LaTeX / special chars ───');
  const response2 = await runCliCommand('chatgpt', 'Test 2: Write LaTeX \\frac{a}{b}.', 90000);
  if (!response2 || response2.length === 0) throw new Error('Test 2: Empty response');
  console.log('[E2E] ✅ Test 2 PASSED');

  console.log('\n🎉 ✅ ALL LIVE TESTS PASSED!\n');
  return true;
}

// ─── Main ────────────────────────────────────────────────────────────────────

async function main() {
  console.log('====================================================');
  console.log('🤖 WEBAICHAT E2E TEST SUITE');
  console.log('====================================================');

  let bridgeProcess = null;

  try {
    bridgeProcess = await ensureBridgeServer();

    if (LIVE_MODE) {
      await runLiveTests();
    } else {
      await runMockTests();
    }

    process.exitCode = 0;
  } catch (err) {
    console.error('\n❌ 💥 E2E TEST FAILED:');
    console.error(err.message);
    process.exitCode = 1;
  } finally {
    if (bridgeProcess) {
      console.log('[E2E] Stopping bridge server...');
      bridgeProcess.kill();
    }
    console.log('[E2E] Test run finished.');
  }
}

main();
