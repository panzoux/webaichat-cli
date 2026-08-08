// Pi extension: register chatto as a custom OpenAI-compatible provider.
//
// Install (pick one):
//   Global (all projects):  copy/symlink this file to ~/.pi/agent/extensions/chatto.ts
//   Project-local:          copy/symlink this file to .pi/extensions/chatto.ts
//   Ad-hoc test:             pi -e ./integrations/pi/chatto.ts
//
// Prerequisites: browser-bridge, the Chrome extension, and chatto must all be
// running first (see docs/chatto.md). Default chatto port is 11434.

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

export default function (pi: ExtensionAPI) {
  pi.registerProvider("chatto", {
    baseUrl: "http://127.0.0.1:11434/v1",
    apiKey: "chatto", // chatto does not validate the key — any string works
    api: "openai-completions",
    models: [
      {
        id: "chatgpt",
        name: "ChatGPT (via chatto)",
        reasoning: false,
        input: ["text"],
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        // Estimates only — chatto proxies a browser chat UI, not a metered API,
        // so nothing here is actually enforced.
        contextWindow: 128000,
        maxTokens: 8192,
        // chatto's tool_calls responses are structurally valid OpenAI, but
        // the underlying "provider" is a browser scrape, not a real API —
        // pi's own tool-result continuation logic apparently doesn't
        // recognize the turn as continuable without these, and falls back
        // to re-injecting the original user message as a new turn instead
        // of just letting the model keep going. requiresToolResultName also
        // lets chatto label results as "[Tool Result: <name>]" instead of
        // just "[Tool Result]".
        compat: {
          requiresToolResultName: true,
          requiresAssistantAfterToolResult: true,
        },
      },
      {
        id: "gemini",
        name: "Gemini (via chatto)",
        reasoning: false,
        input: ["text"],
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
        contextWindow: 128000,
        maxTokens: 8192,
        compat: {
          requiresToolResultName: true,
          requiresAssistantAfterToolResult: true,
        },
      },
    ],
  });
}
