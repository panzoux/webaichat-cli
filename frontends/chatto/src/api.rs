use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use axum::response::sse::{Event as SseEvent, KeepAlive};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::stitcher::{stitch, ChatMessage};
use crate::tools::{
    build_repair_prompt, extract_marker_body, marker_appears_early, new_tool_call, parse_repaired_json,
    parse_tool_marker, tools_addendum, ToolCall, ToolCallDelta, ToolDef, PEEK_WINDOW, TOOL_CALL_START,
};
use web_llm_runtime::bridge_client::BridgeClient;
use web_llm_runtime::transport::Event as BridgeEvent;

// ─── OpenAI wire types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub tools: Vec<ToolDef>,
}

#[derive(Debug, Serialize)]
pub struct ModelObject {
    pub id: &'static str,
    pub object: &'static str,
    pub owned_by: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: &'static str,
    pub data: Vec<ModelObject>,
}

#[derive(Debug, Serialize)]
pub struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Serialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChunkDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompletionChunk {
    pub id: String,
    pub object: &'static str,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize)]
pub struct CompletionMessage {
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Serialize)]
pub struct CompletionChoice {
    pub index: u32,
    pub message: CompletionMessage,
    pub finish_reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ApiError,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Build the non-streaming OpenAI `chat.completion` response body for a plain
/// text reply.
fn build_completion_response(id: String, model: String, content: String, created: u64) -> ChatCompletionResponse {
    ChatCompletionResponse {
        id,
        object: "chat.completion",
        created,
        model,
        choices: vec![CompletionChoice {
            index: 0,
            message: CompletionMessage { role: "assistant", content: Some(content), tool_calls: None },
            finish_reason: "stop",
        }],
    }
}

/// Build the non-streaming OpenAI `chat.completion` response body for a
/// detected tool call.
fn build_tool_call_response(id: String, model: String, created: u64, call: ToolCall) -> ChatCompletionResponse {
    ChatCompletionResponse {
        id,
        object: "chat.completion",
        created,
        model,
        choices: vec![CompletionChoice {
            index: 0,
            message: CompletionMessage { role: "assistant", content: None, tool_calls: Some(vec![call]) },
            finish_reason: "tool_calls",
        }],
    }
}

/// SSE data payload for a content-delta chunk.
fn content_chunk_json(id: &str, model: &str, content: String) -> String {
    let chunk = CompletionChunk {
        id: id.to_string(),
        object: "chat.completion.chunk",
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta { content: Some(content), tool_calls: None },
            finish_reason: None,
        }],
    };
    serde_json::to_string(&chunk).unwrap_or_default()
}

/// SSE data payload for the terminal chunk of a plain text reply. OpenAI
/// clients (pi included) treat a stream that ends without a `finish_reason`
/// chunk as an error and retry — this must always be sent once, right before
/// `[DONE]`.
fn finish_chunk_json(id: &str, model: &str) -> String {
    let chunk = CompletionChunk {
        id: id.to_string(),
        object: "chat.completion.chunk",
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta { content: None, tool_calls: None },
            finish_reason: Some("stop".to_string()),
        }],
    };
    serde_json::to_string(&chunk).unwrap_or_default()
}

/// SSE data payload carrying a detected tool call.
fn tool_call_chunk_json(id: &str, model: &str, call: ToolCall) -> String {
    let chunk = CompletionChunk {
        id: id.to_string(),
        object: "chat.completion.chunk",
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta { content: None, tool_calls: Some(vec![ToolCallDelta { index: 0, call }]) },
            finish_reason: None,
        }],
    };
    serde_json::to_string(&chunk).unwrap_or_default()
}

/// SSE data payload for the terminal chunk of a tool-call reply.
fn tool_calls_finish_chunk_json(id: &str, model: &str) -> String {
    let chunk = CompletionChunk {
        id: id.to_string(),
        object: "chat.completion.chunk",
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta { content: None, tool_calls: None },
            finish_reason: Some("tool_calls".to_string()),
        }],
    };
    serde_json::to_string(&chunk).unwrap_or_default()
}

/// Send `message` to `provider` over a fresh bridge connection and collect
/// the full (non-streamed) response. Used for the tool-call JSON repair
/// pass, where we just need the final text, not live delivery.
async fn send_and_collect(bridge_url: &str, provider: &str, message: String) -> Result<String, String> {
    let mut bridge = BridgeClient::new(bridge_url);
    bridge.connect().await.map_err(|e| format!("Bridge connection failed: {}", e))?;
    bridge
        .send_event(&BridgeEvent::SendMessage { provider: provider.to_string(), message })
        .await
        .map_err(|e| format!("SendMessage failed: {}", e))?;

    let mut full = String::new();
    loop {
        match bridge.receive_event().await {
            Ok(Some(BridgeEvent::MessageChunk { provider: p, content, .. })) if p == provider => {
                full.push_str(&content);
            }
            Ok(Some(BridgeEvent::MessageEnd { provider: p, .. })) if p == provider => break,
            Ok(Some(BridgeEvent::Error { provider: p, message })) if p == provider => {
                let _ = bridge.disconnect().await;
                return Err(message);
            }
            Ok(None) => break,
            Err(e) => {
                let _ = bridge.disconnect().await;
                return Err(e.to_string());
            }
            _ => {}
        }
    }

    let _ = bridge.disconnect().await;
    Ok(full)
}

fn bridge_error_response(message: String) -> Response {
    (
        StatusCode::BAD_GATEWAY,
        Json(ErrorResponse {
            error: ApiError { message, error_type: "bridge_error" },
        }),
    )
        .into_response()
}

// ─── App state ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub bridge_url: String,
}

// ─── Model → provider routing ─────────────────────────────────────────────────

fn model_to_provider(model: &str) -> &'static str {
    let m = model.to_lowercase();
    if m.starts_with("gemini") {
        "gemini"
    } else {
        // chatgpt, gpt-*, or anything else → ChatGPT as default
        "chatgpt"
    }
}

// ─── Routes ───────────────────────────────────────────────────────────────────

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(Arc::new(state))
}

async fn list_models() -> Json<ModelsResponse> {
    Json(ModelsResponse {
        object: "list",
        data: vec![
            ModelObject { id: "chatgpt",    object: "model", owned_by: "chatto" },
            ModelObject { id: "gemini",     object: "model", owned_by: "chatto" },
            // Aliases for opencode / Cursor compatibility
            ModelObject { id: "gpt-4o",     object: "model", owned_by: "chatto" },
            ModelObject { id: "gemini-pro", object: "model", owned_by: "chatto" },
        ],
    })
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Response {
    let provider = model_to_provider(&req.model).to_string();
    let mut prompt = stitch(&req.messages);
    if !req.tools.is_empty() {
        prompt.push_str(&tools_addendum(&req.tools));
    }
    let bridge_url = state.bridge_url.clone();
    let model = req.model.clone();
    let stream = req.stream;

    tracing::info!(
        "chat_completions: provider={} prompt_len={} tools={}",
        provider,
        prompt.len(),
        req.tools.len()
    );

    // Connect to bridge and run the request, collecting chunks
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<String, String>>(256);

    let spawn_bridge_url = bridge_url.clone();
    let spawn_provider = provider.clone();
    tokio::spawn(async move {
        let bridge_url = spawn_bridge_url;
        let provider = spawn_provider;
        let mut bridge = BridgeClient::new(&bridge_url);
        if let Err(e) = bridge.connect().await {
            let _ = tx.send(Err(format!("Bridge connection failed: {}", e))).await;
            return;
        }

        if let Err(e) = bridge.send_event(&BridgeEvent::SendMessage {
            provider: provider.clone(),
            message: prompt,
        }).await {
            let _ = tx.send(Err(format!("SendMessage failed: {}", e))).await;
            return;
        }

        loop {
            match bridge.receive_event().await {
                Ok(Some(BridgeEvent::MessageChunk { provider: p, content, .. })) if p == provider => {
                    if tx.send(Ok(content)).await.is_err() { break; }
                }
                Ok(Some(BridgeEvent::MessageEnd { provider: p, .. })) if p == provider => {
                    break;
                }
                Ok(Some(BridgeEvent::Error { provider: p, message })) if p == provider => {
                    let _ = tx.send(Err(message)).await;
                    break;
                }
                Ok(None) => break,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string())).await;
                    break;
                }
                _ => {}
            }
        }

        // Close cleanly — otherwise the bridge server logs a "Connection reset
        // without closing handshake" error every time this task's connection
        // is dropped at the end of scope.
        let _ = bridge.disconnect().await;
    });

    let completion_id = format!("chatcmpl-{}", Uuid::new_v4().simple());

    if !stream {
        // Non-streaming: drain everything up front (there's no SSE keep-alive
        // concept for a single JSON response either way, so nothing is lost
        // by fully buffering here — this path only differs from streaming in
        // that it always did).
        let mut full = String::new();
        while let Some(msg) = rx.recv().await {
            match msg {
                Ok(content) => full.push_str(&content),
                Err(e) => return bridge_error_response(e),
            }
        }

        if marker_appears_early(&full, PEEK_WINDOW) {
            if let Some(parsed) = parse_tool_marker(&full) {
                let call = new_tool_call(format!("call_{}", Uuid::new_v4().simple()), parsed.name, parsed.arguments);
                return Json(build_tool_call_response(completion_id, model, unix_now(), call)).into_response();
            }

            tracing::warn!("chatto-tool marker found but failed to parse; asking model to repair the JSON");
            if let Some(body) = extract_marker_body(&full) {
                match send_and_collect(&bridge_url, &provider, build_repair_prompt(&body)).await {
                    Ok(repaired) => {
                        if let Some(parsed) = parse_repaired_json(&repaired) {
                            tracing::info!("Repair pass succeeded");
                            let call = new_tool_call(format!("call_{}", Uuid::new_v4().simple()), parsed.name, parsed.arguments);
                            return Json(build_tool_call_response(completion_id, model, unix_now(), call)).into_response();
                        }
                        tracing::warn!("Repair pass reply still didn't parse; returning original reply as plain text");
                    }
                    Err(e) => tracing::warn!("Repair pass request failed: {}; returning original reply as plain text", e),
                }
            }
        }

        return Json(build_completion_response(completion_id, model, full, unix_now())).into_response();
    }

    // Streaming: the whole peek → buffer → parse → (repair round-trip) →
    // emit pipeline runs *inside* stream_chat_completions, not here, and
    // specifically not before returning this Response. Buffering a tool call
    // or the repair pass can mean 15-45+ seconds with zero bytes from
    // ChatGPT — if we made the client wait that long before we even started
    // the HTTP response, its own timeout would fire and it would abort and
    // retry the whole request (observed in practice: the same prompt sent
    // over and over, each retry taking just as long, forever). Returning the
    // SSE response immediately lets axum's keep_alive() emit periodic pings
    // while we're still working, so the connection reads as alive the whole
    // time.
    stream_chat_completions(completion_id, model, bridge_url, provider, rx)
}

fn build_sse_response(
    s: impl futures_util::Stream<Item = Result<SseEvent, std::convert::Infallible>> + Send + 'static,
) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    (headers, Sse::new(s).keep_alive(KeepAlive::default())).into_response()
}

/// Drives the whole SSE reply: peek → (buffer + parse + optional repair
/// round-trip) for a tool call, or replay + live-drain for plain content.
///
/// Everything here runs as part of the stream itself — the caller has
/// already returned this as the HTTP response by the time any of this runs.
/// That's the point: axum's `keep_alive()` only emits pings while the SSE
/// response is open and being polled, so a long wait (buffering a tool call,
/// or the LLM repair pass) needs to happen *inside* the stream, not before
/// it, or the client sees dead air and times out.
fn stream_chat_completions(
    id: String,
    model: String,
    bridge_url: String,
    provider: String,
    mut rx: tokio::sync::mpsc::Receiver<Result<String, String>>,
) -> Response {
    let s = async_stream::stream! {
        let mut prefix = String::new();
        let mut replay: Vec<String> = Vec::new();

        while !prefix.contains(TOOL_CALL_START) && prefix.len() < PEEK_WINDOW {
            match rx.recv().await {
                Some(Ok(chunk)) => {
                    prefix.push_str(&chunk);
                    replay.push(chunk);
                }
                Some(Err(e)) => {
                    tracing::error!("Stream error: {}", e);
                    yield Ok::<SseEvent, std::convert::Infallible>(SseEvent::default().data("[DONE]"));
                    return;
                }
                None => break,
            }
        }

        if prefix.contains(TOOL_CALL_START) {
            let mut full = prefix;
            loop {
                match rx.recv().await {
                    Some(Ok(chunk)) => full.push_str(&chunk),
                    Some(Err(e)) => {
                        tracing::error!("Stream error: {}", e);
                        yield Ok(SseEvent::default().data("[DONE]"));
                        return;
                    }
                    None => break,
                }
            }

            let mut parsed = parse_tool_marker(&full);

            if parsed.is_none() {
                tracing::warn!("chatto-tool marker found but failed to parse; asking model to repair the JSON");
                if let Some(body) = extract_marker_body(&full) {
                    match send_and_collect(&bridge_url, &provider, build_repair_prompt(&body)).await {
                        Ok(repaired) => {
                            parsed = parse_repaired_json(&repaired);
                            if parsed.is_some() {
                                tracing::info!("Repair pass succeeded");
                            } else {
                                tracing::warn!("Repair pass reply still didn't parse; returning original reply as plain text");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Repair pass request failed: {}; returning original reply as plain text", e);
                        }
                    }
                }
            }

            match parsed {
                Some(p) => {
                    let call = new_tool_call(format!("call_{}", Uuid::new_v4().simple()), p.name, p.arguments);
                    yield Ok(SseEvent::default().data(tool_call_chunk_json(&id, &model, call)));
                    yield Ok(SseEvent::default().data(tool_calls_finish_chunk_json(&id, &model)));
                    yield Ok(SseEvent::default().data("[DONE]"));
                }
                None => {
                    yield Ok(SseEvent::default().data(content_chunk_json(&id, &model, full)));
                    yield Ok(SseEvent::default().data(finish_chunk_json(&id, &model)));
                    yield Ok(SseEvent::default().data("[DONE]"));
                }
            }
            return;
        }

        // Not a tool call — replay what we peeked, then keep draining live.
        for chunk in replay {
            yield Ok(SseEvent::default().data(content_chunk_json(&id, &model, chunk)));
        }

        loop {
            match rx.recv().await {
                Some(Ok(content)) => {
                    yield Ok(SseEvent::default().data(content_chunk_json(&id, &model, content)));
                }
                Some(Err(e)) => {
                    tracing::error!("Stream error: {}", e);
                    yield Ok(SseEvent::default().data("[DONE]"));
                    return;
                }
                None => {
                    yield Ok(SseEvent::default().data(finish_chunk_json(&id, &model)));
                    yield Ok(SseEvent::default().data("[DONE]"));
                    return;
                }
            }
        }
    };

    build_sse_response(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_chatgpt_by_default() {
        assert_eq!(model_to_provider("anything-unrecognized"), "chatgpt");
        assert_eq!(model_to_provider("chatgpt"), "chatgpt");
        assert_eq!(model_to_provider("gpt-4o"), "chatgpt");
        assert_eq!(model_to_provider("GPT-4"), "chatgpt");
    }

    #[test]
    fn routes_gemini_variants() {
        assert_eq!(model_to_provider("gemini"), "gemini");
        assert_eq!(model_to_provider("gemini-pro"), "gemini");
        assert_eq!(model_to_provider("Gemini-Flash"), "gemini");
    }

    #[test]
    fn content_chunk_has_delta_and_no_finish_reason() {
        let json = content_chunk_json("chatcmpl-1", "chatgpt", "hi".to_string());
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["object"], "chat.completion.chunk");
        assert_eq!(v["choices"][0]["delta"]["content"], "hi");
        assert!(v["choices"][0]["finish_reason"].is_null());
    }

    #[test]
    fn finish_chunk_has_stop_reason_and_empty_delta() {
        // Regression test: without this chunk, OpenAI-compatible clients like
        // pi see a stream that ends with no finish_reason and retry/abort.
        let json = finish_chunk_json("chatcmpl-1", "chatgpt");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["choices"][0]["finish_reason"], "stop");
        assert!(v["choices"][0]["delta"].get("content").is_none());
    }

    #[test]
    fn non_stream_response_has_openai_shape() {
        let resp = build_completion_response(
            "chatcmpl-abc".to_string(),
            "chatgpt".to_string(),
            "Hello there".to_string(),
            1_700_000_000,
        );

        assert_eq!(resp.id, "chatcmpl-abc");
        assert_eq!(resp.object, "chat.completion");
        assert_eq!(resp.model, "chatgpt");
        assert_eq!(resp.created, 1_700_000_000);
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.role, "assistant");
        assert_eq!(resp.choices[0].message.content, Some("Hello there".to_string()));
        assert_eq!(resp.choices[0].finish_reason, "stop");

        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["object"], "chat.completion");
        assert_eq!(json["choices"][0]["message"]["content"], "Hello there");
        assert!(json["choices"][0]["message"].get("tool_calls").is_none());
    }

    #[test]
    fn non_stream_tool_call_response_has_openai_shape() {
        let call = new_tool_call("call_1".to_string(), "write".to_string(), r#"{"path":"a.txt"}"#.to_string());
        let resp = build_tool_call_response("chatcmpl-abc".to_string(), "chatgpt".to_string(), 1_700_000_000, call);

        assert_eq!(resp.choices[0].finish_reason, "tool_calls");
        assert_eq!(resp.choices[0].message.content, None);

        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["choices"][0]["message"].get("content").is_none());
        assert_eq!(json["choices"][0]["message"]["tool_calls"][0]["id"], "call_1");
        assert_eq!(json["choices"][0]["message"]["tool_calls"][0]["type"], "function");
        assert_eq!(json["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "write");
        assert_eq!(
            json["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            r#"{"path":"a.txt"}"#
        );
    }

    #[test]
    fn tool_call_chunk_has_delta_tool_calls_and_no_finish_reason() {
        let call = new_tool_call("call_1".to_string(), "write".to_string(), "{}".to_string());
        let json = tool_call_chunk_json("chatcmpl-1", "chatgpt", call);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["choices"][0]["delta"].get("content").is_none());
        assert_eq!(v["choices"][0]["delta"]["tool_calls"][0]["index"], 0);
        assert_eq!(v["choices"][0]["delta"]["tool_calls"][0]["function"]["name"], "write");
        assert!(v["choices"][0]["finish_reason"].is_null());
    }

    #[test]
    fn tool_calls_finish_chunk_has_tool_calls_reason() {
        let json = tool_calls_finish_chunk_json("chatcmpl-1", "chatgpt");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["choices"][0]["finish_reason"], "tool_calls");
        assert!(v["choices"][0]["delta"].get("tool_calls").is_none());
    }
}
