/// Stitch an OpenAI messages array into a single plain-text prompt.
///
/// Chatto does NOT manage memory or decide what to include — that is opencode's job.
/// This function is purely a format translator: JSON messages array → plain text string
/// suitable for pasting into a browser chat UI.
///
/// Rules:
/// - Single user message with no system/assistant turns → content sent as-is (cleanest)
/// - Multi-turn → each message prefixed with its role label, separated by blank lines
/// - Empty content is skipped

use crate::tools::{ToolCallIn, TOOL_CALL_END, TOOL_CALL_START};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, deserialize_with = "deserialize_content")]
    pub content: String,
    /// Present on an assistant message that called a tool (via our
    /// plain-text marker convention) and got it echoed back by the client.
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallIn>>,
    /// Present on a `role: "tool"` message; which tool this result came from.
    #[serde(default)]
    pub name: Option<String>,
}

/// OpenAI clients send `content` either as a plain string, or — for
/// multi-part / vision-capable payloads — as an array of `{"type", "text", ...}`
/// parts (pi does this even for plain text). Accept both and flatten to text,
/// dropping non-text parts (e.g. image_url) since the browser only takes text.
fn deserialize_content<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    })
}

pub fn stitch(messages: &[ChatMessage]) -> String {
    let non_empty: Vec<&ChatMessage> = messages
        .iter()
        .filter(|m| !m.content.trim().is_empty() || m.tool_calls.is_some())
        .collect();

    // Simple case: single user message — send as-is, no label clutter
    if non_empty.len() == 1 && non_empty[0].role == "user" && non_empty[0].tool_calls.is_none() {
        return non_empty[0].content.trim().to_string();
    }

    // Multi-turn: prefix each role
    non_empty
        .iter()
        .map(|m| format_message(m))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_message(m: &ChatMessage) -> String {
    // An assistant message that called a tool — reconstruct it in the same
    // marker form we ask the model to use, so its own history reads
    // consistently if it looks back at what it did.
    if let Some(calls) = &m.tool_calls {
        let mut parts = Vec::new();
        if !m.content.trim().is_empty() {
            parts.push(m.content.trim().to_string());
        }
        for call in calls {
            parts.push(format!(
                "{}\n{{\"name\": \"{}\", \"arguments\": {}}}\n{}",
                TOOL_CALL_START,
                call.function.name,
                if call.function.arguments.trim().is_empty() { "{}" } else { call.function.arguments.trim() },
                TOOL_CALL_END,
            ));
        }
        return format!("[Assistant]: {}", parts.join("\n"));
    }

    match m.role.as_str() {
        "system" => format!("[System]: {}", m.content.trim()),
        "assistant" => format!("[Assistant]: {}", m.content.trim()),
        "tool" => {
            let label = match &m.name {
                Some(name) => format!("[Tool Result: {}]", name),
                None => "[Tool Result]".to_string(),
            };
            format!("{}: {}", label, m.content.trim())
        }
        _ => format!("[User]: {}", m.content.trim()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.to_string(), content: content.to_string(), tool_calls: None, name: None }
    }

    #[test]
    fn single_user_message_is_passthrough() {
        let msgs = vec![msg("user", "Hello!")];
        assert_eq!(stitch(&msgs), "Hello!");
    }

    #[test]
    fn multi_turn_gets_labels() {
        let msgs = vec![
            msg("system", "You are helpful."),
            msg("user", "What is 2+2?"),
            msg("assistant", "4."),
            msg("user", "And 3+3?"),
        ];
        let result = stitch(&msgs);
        assert!(result.contains("[System]: You are helpful."));
        assert!(result.contains("[User]: What is 2+2?"));
        assert!(result.contains("[Assistant]: 4."));
        assert!(result.contains("[User]: And 3+3?"));
    }

    #[test]
    fn empty_messages_are_skipped() {
        let msgs = vec![
            msg("system", ""),
            msg("user", "Hello!"),
        ];
        // Only one non-empty non-system message — but system is empty so filtered
        // Result: single user message → passthrough
        assert_eq!(stitch(&msgs), "Hello!");
    }

    #[test]
    fn deserializes_plain_string_content() {
        let m: ChatMessage = serde_json::from_str(r#"{"role":"user","content":"hi there"}"#).unwrap();
        assert_eq!(m.content, "hi there");
    }

    #[test]
    fn deserializes_array_content_parts() {
        // This is the shape pi sends for plain text messages.
        let m: ChatMessage = serde_json::from_str(
            r#"{"role":"user","content":[{"type":"text","text":"hello! "},{"type":"text","text":"give me short greetings"}]}"#,
        )
        .unwrap();
        assert_eq!(m.content, "hello! give me short greetings");
    }

    #[test]
    fn deserializes_array_content_ignoring_non_text_parts() {
        let m: ChatMessage = serde_json::from_str(
            r#"{"role":"user","content":[{"type":"image_url","image_url":{"url":"http://x"}},{"type":"text","text":"describe this"}]}"#,
        )
        .unwrap();
        assert_eq!(m.content, "describe this");
    }

    #[test]
    fn deserializes_null_content_as_empty() {
        let m: ChatMessage = serde_json::from_str(r#"{"role":"assistant","content":null}"#).unwrap();
        assert_eq!(m.content, "");
    }

    #[test]
    fn deserializes_missing_content_as_empty() {
        let m: ChatMessage = serde_json::from_str(r#"{"role":"assistant"}"#).unwrap();
        assert_eq!(m.content, "");
    }

    #[test]
    fn deserializes_assistant_tool_call_history() {
        let m: ChatMessage = serde_json::from_str(
            r#"{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","function":{"name":"write","arguments":"{\"path\":\"a.txt\"}"}}]}"#,
        )
        .unwrap();
        let calls = m.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "write");
    }

    #[test]
    fn stitches_assistant_tool_call_as_marker_block() {
        let msgs = vec![
            msg("user", "create a.txt"),
            ChatMessage {
                role: "assistant".to_string(),
                content: String::new(),
                tool_calls: Some(vec![crate::tools::ToolCallIn {
                    id: "call_1".to_string(),
                    function: crate::tools::ToolCallFunctionIn {
                        name: "write".to_string(),
                        arguments: r#"{"path":"a.txt","content":"hi"}"#.to_string(),
                    },
                }]),
                name: None,
            },
        ];
        let result = stitch(&msgs);
        assert!(result.contains(&format!("[Assistant]: {}", crate::tools::TOOL_CALL_START)));
        assert!(result.contains(r#"{"name": "write", "arguments": {"path":"a.txt","content":"hi"}}"#));
        assert!(result.contains(crate::tools::TOOL_CALL_END));
    }

    #[test]
    fn stitches_tool_result_with_name_label() {
        let msgs = vec![
            msg("user", "create a.txt"),
            ChatMessage {
                role: "tool".to_string(),
                content: "File written successfully.".to_string(),
                tool_calls: None,
                name: Some("write".to_string()),
            },
        ];
        let result = stitch(&msgs);
        assert!(result.contains("[Tool Result: write]: File written successfully."));
    }

    #[test]
    fn stitches_tool_result_without_name_label() {
        let msgs = vec![
            msg("user", "create a.txt"),
            ChatMessage {
                role: "tool".to_string(),
                content: "done".to_string(),
                tool_calls: None,
                name: None,
            },
        ];
        let result = stitch(&msgs);
        assert!(result.contains("[Tool Result]: done"));
    }
}
