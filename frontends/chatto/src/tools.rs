/// Tool-calling support for chatto.
///
/// ChatGPT's/Gemini's web UI has no native function-calling channel — it only
/// ever sees and produces plain text. To let a client's `tools` (e.g. pi's
/// read/bash/edit/write) still work, chatto describes them in the stitched
/// prompt and asks the model to reply with a specific plain-text-delimited
/// JSON block when it wants to call one. That block is parsed back out here
/// and translated into a real OpenAI `tool_calls` response, which is the part
/// an agent harness actually acts on.
///
/// Deliberately NOT a markdown code fence: ChatGPT's renderer syntax-
/// highlights fenced blocks, which can strip the opening ```lang backticks
/// from the DOM text entirely (observed in practice — the fence vanished,
/// leaving a bare "chatto-tool" label with no way to detect it). Plain
/// delimiter lines with no markdown-special characters render as ordinary
/// text, so nothing in the pipeline can rewrite or hide them.
use serde::{Deserialize, Serialize};

pub const TOOL_CALL_START: &str = "@@chatto-tool-call@@";
pub const TOOL_CALL_END: &str = "@@end-chatto-tool-call@@";

/// How many characters of the reply's start we're willing to buffer before
/// giving up on finding the start marker and switching to live streaming.
/// Needs enough margin to survive the model ignoring the "nothing else"
/// instruction and adding a short lead-in sentence before the marker anyway
/// (observed in practice) — sized generously since tool-calling reliability
/// matters more here than shaving latency off ordinary chat replies.
pub const PEEK_WINDOW: usize = 200;

// ─── Incoming request: tool schemas the client offers ──────────────────────

#[derive(Debug, Deserialize)]
pub struct ToolDef {
    pub function: ToolFunctionDef,
}

#[derive(Debug, Deserialize)]
pub struct ToolFunctionDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// Text appended to the stitched prompt describing available tools and the
/// calling convention. Empty when the request offered no tools.
pub fn tools_addendum(tools: &[ToolDef]) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let mut s = format!(
        "\n\n[System]: You have tools available through this chat — this is the \
         only channel to call them. These tools run on the user's own machine via a \
         separate program — they are NOT the same as, and have nothing to do with, any \
         code interpreter, analysis tool, or sandboxed execution environment you may \
         have access to natively. Do NOT run these commands yourself in your own \
         environment (e.g. a sandbox whose home directory is something like /home/oai) \
         — that environment cannot see the user's files and any output from it is \
         useless here. The ONLY way to invoke one of these tools is emitting the exact \
         marker below; never actually execute anything yourself. To call one, your \
         ENTIRE reply must be exactly the following and nothing else: no leading \
         sentence, no explanation, no code fence, not even one word before the first \
         line:\n\n\
         {start}\n{{\"name\": \"<tool name>\", \"arguments\": {{ ... }}}}\n{end}\n\n\
         Available tools:\n",
        start = TOOL_CALL_START,
        end = TOOL_CALL_END,
    );
    for t in tools {
        s.push_str(&format!(
            "- {} — {}\n  arguments schema: {}\n",
            t.function.name, t.function.description, t.function.parameters
        ));
    }
    s.push_str(
        "\nThe arguments object must be valid JSON. If the content you're writing itself \
         contains double quotes or backslashes (e.g. batch/PowerShell scripts, JSON, code \
         with string literals), escape them as \\\" and \\\\ so they don't break the \
         surrounding JSON string — this is easy to miss. Before replying, check your own \
         output: mentally parse it as a JSON parser would, character by character through \
         every string value, and confirm every embedded \" and \\ is escaped. If you find \
         one that isn't, fix it before sending your reply — don't send it and wait to be \
         asked to fix it.\
         \nIf you are not calling a tool, just answer normally in plain text.",
    );
    s
}

// ─── Incoming history: prior tool calls / results, round-tripped by the client ──

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallIn {
    // Part of the wire shape we accept from history; not needed when
    // reconstructing the marker block for the prompt.
    #[allow(dead_code)]
    #[serde(default)]
    pub id: String,
    pub function: ToolCallFunctionIn,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallFunctionIn {
    pub name: String,
    #[serde(default)]
    pub arguments: String,
}

// ─── Outgoing: a tool call detected in the model's reply ───────────────────

#[derive(Debug, Serialize)]
pub struct ToolCallFunctionPayload {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: ToolCallFunctionPayload,
}

#[derive(Debug, Serialize)]
pub struct ToolCallDelta {
    pub index: u32,
    #[serde(flatten)]
    pub call: ToolCall,
}

pub fn new_tool_call(id: String, name: String, arguments: String) -> ToolCall {
    ToolCall {
        id,
        kind: "function",
        function: ToolCallFunctionPayload { name, arguments },
    }
}

pub struct ParsedToolCall {
    pub name: String,
    pub arguments: String,
}

/// Whether `TOOL_CALL_START` shows up within the first `window` bytes of
/// `text` — the same rule used to decide, while still streaming, whether a
/// reply is a tool call at all. Exposed so the non-streaming path (which
/// already has the full text in hand) applies the identical boundary rather
/// than matching the marker anywhere, so a stray mention of it deep in an
/// unrelated answer can't be misread as a real tool call.
pub fn marker_appears_early(text: &str, window: usize) -> bool {
    matches!(text.find(TOOL_CALL_START), Some(idx) if idx <= window)
}

/// Scan `text` for a `TOOL_CALL_START ... TOOL_CALL_END` block and return the
/// raw text between them (fence-stripped, untrimmed of JSON validity) — or
/// `None` if the start marker isn't present at all. Tolerant of the end
/// marker being missing (a reply can get cut short).
pub fn extract_marker_body(text: &str) -> Option<String> {
    let start = text.find(TOOL_CALL_START)?;
    let after_start = &text[start + TOOL_CALL_START.len()..];

    let body = match after_start.find(TOOL_CALL_END) {
        Some(end) => &after_start[..end],
        None => after_start,
    };

    Some(strip_incidental_fence(body.trim()).to_string())
}

fn tool_call_from_exact_json(json_str: &str) -> Option<ParsedToolCall> {
    let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let name = parsed.get("name")?.as_str()?.to_string();
    let arguments_value = parsed
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let arguments = serde_json::to_string(&arguments_value).ok()?;

    Some(ParsedToolCall { name, arguments })
}

/// Try `json_str` as-is; if that fails, run the deterministic quote/backslash
/// repair and try again. This is instant and has no round-trip cost, so it
/// runs before ever asking the model to fix its own output.
fn tool_call_from_json_str(json_str: &str) -> Option<ParsedToolCall> {
    tool_call_from_exact_json(json_str).or_else(|| tool_call_from_exact_json(&repair_unescaped_json(json_str)))
}

/// Best-effort deterministic repair for the two most common ways
/// LLM-generated JSON breaks:
///
/// - Literal, unescaped `"` inside a string value (e.g. a batch script's own
///   `"..."=="..."` comparison syntax). Fixed by re-escaping any quote that
///   isn't actually closing the string — recognized by what comes right
///   after it, skipping whitespace: `,`, `:`, `}`, `]`, or end of input, the
///   same set of characters that can legally follow a real JSON string
///   terminator.
/// - A backslash followed by a character that is *never* valid after `\` in
///   JSON (anything other than `" \ / b f n r t u`) — e.g. `\System32`,
///   `\cmd.exe`. There's no legal interpretation where that's a real escape,
///   so it's unambiguously a literal backslash that needs doubling.
///
/// Deliberately leaves `\` followed by one of `b f n r t u` untouched even
/// though it's still occasionally wrong (e.g. `\test`, `\file` in a Windows
/// path): that one IS ambiguous — it could be a legitimate escape (`\t`,
/// `\uXXXX`, and `\r\n` is in fact how every real newline arrives) or the
/// start of an ordinary word. Guessing wrong there would silently corrupt
/// otherwise-valid content, which is worse than leaving a genuine problem
/// for the JSON parse (and the LLM repair round-trip) to catch.
///
/// This is a heuristic, not a JSON parser — it can misjudge pathological
/// input — but every fix it makes targets an unambiguous defect, so a wrong
/// guess degrades to "still doesn't parse" rather than silently corrupting
/// otherwise-valid JSON.
fn repair_unescaped_json(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 16);
    let mut in_string = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if !in_string {
            out.push(c);
            if c == '"' {
                in_string = true;
            }
            i += 1;
            continue;
        }

        if c == '\\' {
            match chars.get(i + 1) {
                Some(next) if matches!(next, '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u') => {
                    // Ambiguous — could be a real escape or a word that
                    // happens to start with the same letter. Leave as-is.
                    out.push('\\');
                    out.push(*next);
                    i += 2;
                }
                Some(next) => {
                    // Never a legal JSON escape — unambiguously a literal
                    // backslash that was never doubled.
                    out.push('\\');
                    out.push('\\');
                    out.push(*next);
                    i += 2;
                }
                None => {
                    out.push('\\');
                    out.push('\\');
                    i += 1;
                }
            }
            continue;
        }

        if c == '"' {
            let mut j = i + 1;
            while matches!(chars.get(j), Some(ch) if ch.is_whitespace()) {
                j += 1;
            }
            let closes_string = matches!(chars.get(j), None | Some(',' | ':' | '}' | ']'));
            if closes_string {
                out.push('"');
                in_string = false;
            } else {
                out.push('\\');
                out.push('"');
            }
            i += 1;
            continue;
        }

        out.push(c);
        i += 1;
    }

    out
}

/// Scan `text` for a `TOOL_CALL_START ... TOOL_CALL_END` block and parse the
/// JSON between them as `{"name": ..., "arguments": {...}}`. Returns `None`
/// if the start marker isn't found, or the contents don't parse even after
/// the deterministic repair pass — callers should then attempt the LLM
/// repair round-trip (see `build_repair_prompt`) rather than drop the
/// response outright.
///
/// Tolerant of a stray code fence sneaking in around the JSON despite being
/// asked not to — both the renderer and the model itself have been observed
/// adding one.
pub fn parse_tool_marker(text: &str) -> Option<ParsedToolCall> {
    tool_call_from_json_str(&extract_marker_body(text)?)
}

/// Build a follow-up prompt asking the model to fix JSON that failed to
/// parse. Sent as a second round-trip when `parse_tool_marker` fails on an
/// otherwise-detected tool call — in practice a model that gets its own
/// malformed JSON handed back with a direct "fix this" request reliably
/// escapes it correctly, even though it didn't the first time unprompted.
pub fn build_repair_prompt(malformed_body: &str) -> String {
    format!(
        "[System]: The JSON object below is malformed and failed to parse. \
         Return ONLY the corrected, valid JSON object — no markers, no code \
         fence, no commentary, nothing else. Make sure every double quote \
         and backslash inside string values is escaped as \\\" and \\\\.\n\n\
         Malformed JSON:\n{}\n",
        malformed_body
    )
}

/// Parse a repair-pass reply. The model may re-wrap its corrected JSON in our
/// marker convention out of habit (observed in practice) even though the
/// repair prompt didn't ask for it, so try that shape first before falling
/// back to treating the whole reply as bare JSON.
pub fn parse_repaired_json(text: &str) -> Option<ParsedToolCall> {
    if let Some(call) = parse_tool_marker(text) {
        return Some(call);
    }
    tool_call_from_json_str(strip_incidental_fence(text.trim()))
}

/// Strip an optional ```lang ... ``` wrapper around the JSON body, in case
/// one snuck in anyway.
fn strip_incidental_fence(s: &str) -> &str {
    let s = match s.strip_prefix("```") {
        Some(rest) => match rest.find('\n') {
            Some(i) => rest[i + 1..].trim_start(),
            None => rest,
        },
        None => s,
    };
    s.strip_suffix("```").unwrap_or(s).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(json: &str) -> String {
        format!("{}\n{}\n{}", TOOL_CALL_START, json, TOOL_CALL_END)
    }

    #[test]
    fn parses_well_formed_marker() {
        let text = wrap(r#"{"name": "write", "arguments": {"path": "a.txt", "content": "hi"}}"#);
        let call = parse_tool_marker(&text).unwrap();
        assert_eq!(call.name, "write");
        let args: serde_json::Value = serde_json::from_str(&call.arguments).unwrap();
        assert_eq!(args["path"], "a.txt");
        assert_eq!(args["content"], "hi");
    }

    #[test]
    fn parses_marker_with_surrounding_whitespace() {
        let text = format!("  {}\n  {{\"name\": \"read\", \"arguments\": {{\"path\": \"x\"}}}}  \n{}  ", TOOL_CALL_START, TOOL_CALL_END);
        let call = parse_tool_marker(&text).unwrap();
        assert_eq!(call.name, "read");
    }

    #[test]
    fn defaults_missing_arguments_to_empty_object() {
        let text = wrap(r#"{"name": "list_providers"}"#);
        let call = parse_tool_marker(&text).unwrap();
        assert_eq!(call.name, "list_providers");
        assert_eq!(call.arguments, "{}");
    }

    #[test]
    fn tolerates_missing_end_marker() {
        // A reply can be cut short before the closing delimiter arrives.
        let text = format!("{}\n{{\"name\": \"write\", \"arguments\": {{}}}}", TOOL_CALL_START);
        let call = parse_tool_marker(&text).unwrap();
        assert_eq!(call.name, "write");
    }

    #[test]
    fn tolerates_incidental_code_fence_around_json() {
        // Observed in practice: either the model or the renderer wraps the
        // JSON in a fence despite being told not to.
        let text = format!("{}\n```json\n{{\"name\": \"write\", \"arguments\": {{}}}}\n```\n{}", TOOL_CALL_START, TOOL_CALL_END);
        let call = parse_tool_marker(&text).unwrap();
        assert_eq!(call.name, "write");
    }

    #[test]
    fn returns_none_when_no_marker_present() {
        assert!(parse_tool_marker("just a normal reply").is_none());
    }

    #[test]
    fn marker_appears_early_true_when_within_window() {
        let text = format!("{}\n{{}}", TOOL_CALL_START);
        assert!(marker_appears_early(&text, 64));
    }

    #[test]
    fn marker_appears_early_false_when_beyond_window() {
        let padding = "x".repeat(100);
        let text = format!("{}{}", padding, TOOL_CALL_START);
        assert!(!marker_appears_early(&text, 64));
    }

    #[test]
    fn marker_appears_early_false_when_absent() {
        assert!(!marker_appears_early("no marker here", 64));
    }

    #[test]
    fn returns_none_when_body_is_invalid_json() {
        let text = wrap("not json");
        assert!(parse_tool_marker(&text).is_none());
    }

    #[test]
    fn extract_marker_body_returns_raw_text_verbatim() {
        let text = wrap(r#"{"name":"write","arguments":{"content":"if /i "%x%"=="1" (...)"}}"#);
        let body = extract_marker_body(&text).unwrap();
        assert!(body.contains(r#"if /i "%x%"=="1""#));
    }

    #[test]
    fn parse_tool_marker_deterministically_repairs_unescaped_quotes_in_batch_script() {
        // The exact failure mode observed in practice: a batch script's own
        // `if /i "..."=="..."` comparison syntax has unescaped quotes. The
        // deterministic repair should fix this with no round-trip needed.
        let text = wrap(
            r#"{"name":"write","arguments":{"path":"a.bat","content":"@echo off\r\nif /i "%cmdcmdline%"=="%SystemRoot%\System32\cmd.exe" (\r\n echo hi\r\n)"}}"#,
        );
        let call = parse_tool_marker(&text).expect("deterministic repair should recover this");
        assert_eq!(call.name, "write");
        let args: serde_json::Value = serde_json::from_str(&call.arguments).unwrap();
        assert_eq!(args["path"], "a.bat");
        assert!(args["content"].as_str().unwrap().contains(r#"if /i "%cmdcmdline%"=="%SystemRoot%\System32\cmd.exe""#));
    }

    #[test]
    fn repair_unescaped_json_leaves_already_valid_json_unchanged_in_effect() {
        let json = r#"{"name":"write","arguments":{"content":"already \"escaped\" and a\\backslash"}}"#;
        let repaired = repair_unescaped_json(json);
        let original: serde_json::Value = serde_json::from_str(json).unwrap();
        let after_repair: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(original, after_repair);
    }

    #[test]
    fn repair_unescaped_json_leaves_ambiguous_backslash_letters_untouched() {
        // `\t` and `\f` here are ambiguous (real escape vs. "\test"/"\file")
        // — deliberately not touched. See the doc comment for why.
        let json = r#"{"name":"write","arguments":{"path":"C:\test\file.txt"}}"#;
        assert_eq!(repair_unescaped_json(json), json);
    }

    #[test]
    fn repair_unescaped_json_fixes_unambiguous_backslash() {
        // `\S` and `\c` are never valid JSON escapes — unambiguous.
        let json = r#"{"name":"write","arguments":{"path":"C:\System32\cmd.exe"}}"#;
        let repaired = repair_unescaped_json(json);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).expect("should now parse");
        assert_eq!(parsed["arguments"]["path"], r"C:\System32\cmd.exe");
    }

    #[test]
    fn build_repair_prompt_includes_the_malformed_body() {
        let prompt = build_repair_prompt(r#"{"name":"write","arguments":{"content":"bad "quote""}}"#);
        assert!(prompt.contains("malformed"));
        assert!(prompt.contains(r#"bad "quote""#));
    }

    #[test]
    fn parse_repaired_json_handles_bare_json() {
        let text = r#"{"name": "write", "arguments": {"path": "a.txt"}}"#;
        let call = parse_repaired_json(text).unwrap();
        assert_eq!(call.name, "write");
    }

    #[test]
    fn parse_repaired_json_handles_json_re_wrapped_in_markers() {
        let text = wrap(r#"{"name": "write", "arguments": {"path": "a.txt"}}"#);
        let call = parse_repaired_json(&text).unwrap();
        assert_eq!(call.name, "write");
    }

    #[test]
    fn parse_repaired_json_handles_json_in_incidental_fence() {
        let text = "```json\n{\"name\": \"write\", \"arguments\": {}}\n```";
        let call = parse_repaired_json(text).unwrap();
        assert_eq!(call.name, "write");
    }

    #[test]
    fn parse_repaired_json_returns_none_if_still_invalid() {
        assert!(parse_repaired_json("still not json").is_none());
    }

    #[test]
    fn tools_addendum_empty_when_no_tools() {
        assert_eq!(tools_addendum(&[]), "");
    }

    #[test]
    fn tools_addendum_lists_each_tool() {
        let tools = vec![ToolDef {
            function: ToolFunctionDef {
                name: "write".to_string(),
                description: "Create or overwrite a file".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        }];
        let addendum = tools_addendum(&tools);
        assert!(addendum.contains(TOOL_CALL_START));
        assert!(addendum.contains(TOOL_CALL_END));
        assert!(addendum.contains("write"));
        assert!(addendum.contains("Create or overwrite a file"));
    }

    #[test]
    fn tools_addendum_disclaims_the_model_own_sandbox() {
        // Regression: ChatGPT has been observed running the requested command
        // in its own native code-interpreter sandbox (home dir /home/oai)
        // instead of emitting our marker — the tools must be described as
        // categorically different from any sandbox the model has natively.
        let tools = vec![ToolDef {
            function: ToolFunctionDef {
                name: "write".to_string(),
                description: "Create or overwrite a file".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        }];
        let addendum = tools_addendum(&tools);
        assert!(addendum.contains("code interpreter"));
        assert!(addendum.contains("sandbox"));
        assert!(addendum.contains("Do NOT run these commands yourself"));
    }

    #[test]
    fn tools_addendum_warns_about_escaping_quotes_and_backslashes() {
        let tools = vec![ToolDef {
            function: ToolFunctionDef {
                name: "write".to_string(),
                description: "Create or overwrite a file".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        }];
        let addendum = tools_addendum(&tools);
        assert!(addendum.contains("valid JSON"));
        assert!(addendum.contains(r#"\""#));
        assert!(addendum.contains(r"\\"));
    }
}
