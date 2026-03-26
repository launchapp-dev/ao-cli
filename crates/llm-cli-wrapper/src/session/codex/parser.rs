use serde_json::{json, Value};

use crate::session::session_event::SessionEvent;

pub(crate) fn parse_codex_stdout_line(line: &str) -> Vec<SessionEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return vec![SessionEvent::TextDelta { text: line.to_string() }];
    };

    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    match event_type {
        "thread.started" | "turn.started" => vec![SessionEvent::Metadata { metadata: value }],
        "turn.completed" => parse_codex_turn_completed(&value),
        "item.completed" => parse_codex_item_completed(&value),
        "error" => parse_codex_top_level_error(&value),
        _ => Vec::new(),
    }
}

fn parse_codex_turn_completed(value: &Value) -> Vec<SessionEvent> {
    let usage = value.get("usage").cloned().unwrap_or_else(|| json!({}));
    vec![SessionEvent::Metadata {
        metadata: json!({
            "type": "codex_usage",
            "usage": usage,
            "raw": value,
        }),
    }]
}

fn parse_codex_item_completed(value: &Value) -> Vec<SessionEvent> {
    let Some(item) = value.get("item") else {
        return Vec::new();
    };

    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");
    match item_type {
        "reasoning" => item
            .get("text")
            .and_then(Value::as_str)
            .map(|text| vec![SessionEvent::Thinking { text: text.to_string() }])
            .unwrap_or_default(),
        "agent_message" | "message" => parse_codex_message_item(item),
        "function_call" => parse_codex_function_call_item(item),
        "tool_call" | "mcp_tool_call" => parse_codex_tool_call_item(item),
        "shell_call" | "local_shell_call" => parse_codex_shell_call_item(item),
        "function_call_output" => parse_codex_function_call_output_item(item),
        "tool_result" | "tool_error" | "mcp_tool_result" | "command_execution" => parse_codex_tool_result_item(item),
        "shell_call_output" | "local_shell_call_output" => parse_codex_shell_call_output_item(item),
        "artifact" | "file" | "attachment" | "patch" | "diff" | "output_image" | "image" => {
            parse_codex_artifact_item(item)
        }
        "error" => parse_codex_item_error(item),
        _ => Vec::new(),
    }
}

fn parse_codex_top_level_error(value: &Value) -> Vec<SessionEvent> {
    let message = codex_error_message(value).unwrap_or_else(|| "codex session failed".to_string());
    vec![SessionEvent::Error { message, recoverable: false }]
}

fn parse_codex_message_item(item: &Value) -> Vec<SessionEvent> {
    if let Some(text) = item.get("text").and_then(Value::as_str) {
        if !text.is_empty() {
            return vec![SessionEvent::FinalText { text: text.to_string() }];
        }
    }

    let Some(content) = item.get("content").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut text = String::new();
    for block in content {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(block_type, "output_text" | "text") {
            if let Some(segment) = block.get("text").and_then(Value::as_str) {
                text.push_str(segment);
            }
        }
    }

    if text.is_empty() {
        Vec::new()
    } else {
        vec![SessionEvent::FinalText { text }]
    }
}

fn parse_codex_tool_call_item(item: &Value) -> Vec<SessionEvent> {
    let tool_name = codex_tool_name(item).unwrap_or_else(|| "codex_tool".to_string());
    let arguments = codex_value_from_fields(item, &["arguments", "input", "params", "tool_input", "payload"])
        .unwrap_or_else(|| json!({}));
    let server = item
        .get("server")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    vec![SessionEvent::ToolCall { tool_name, arguments, server }]
}

fn parse_codex_tool_result_item(item: &Value) -> Vec<SessionEvent> {
    let tool_name = codex_tool_name(item).unwrap_or_else(|| "codex_tool".to_string());
    let success = match item.get("type").and_then(Value::as_str).unwrap_or("") {
        "tool_error" => false,
        _ => item
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| item.get("exit_code").and_then(Value::as_i64).map(|value| value == 0).unwrap_or(true)),
    };
    let output = codex_value_from_fields(item, &["output", "result", "content", "text", "message"])
        .unwrap_or_else(|| item.clone());

    vec![SessionEvent::ToolResult { tool_name, output, success }]
}

fn parse_codex_artifact_item(item: &Value) -> Vec<SessionEvent> {
    let artifact_id = codex_artifact_id(item).unwrap_or_else(|| "codex_artifact".to_string());
    vec![SessionEvent::Artifact { artifact_id, metadata: item.clone() }]
}

fn parse_codex_item_error(item: &Value) -> Vec<SessionEvent> {
    let message = codex_error_message(item).unwrap_or_else(|| "codex item error".to_string());
    vec![SessionEvent::Error { message, recoverable: false }]
}

fn codex_tool_name(item: &Value) -> Option<String> {
    item.get("tool_name")
        .and_then(Value::as_str)
        .or_else(|| item.get("name").and_then(Value::as_str))
        .or_else(|| item.pointer("/function/name").and_then(Value::as_str))
        .or_else(|| item.get("call_id").and_then(Value::as_str))
        .or_else(|| item.get("command").and_then(Value::as_str))
        .or_else(|| item.get("program").and_then(Value::as_str))
        .or_else(|| item.get("shell").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn codex_artifact_id(item: &Value) -> Option<String> {
    item.get("artifact_id")
        .and_then(Value::as_str)
        .or_else(|| item.get("id").and_then(Value::as_str))
        .or_else(|| item.get("name").and_then(Value::as_str))
        .or_else(|| item.get("path").and_then(Value::as_str))
        .or_else(|| item.get("file_path").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn codex_value_from_fields(item: &Value, fields: &[&str]) -> Option<Value> {
    for field in fields {
        if let Some(value) = item.get(field) {
            return Some(normalize_codex_value(value));
        }
    }

    None
}

fn normalize_codex_value(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.clone())),
        other => other.clone(),
    }
}

fn codex_error_message(value: &Value) -> Option<String> {
    let message = value
        .pointer("/error/message")
        .or_else(|| value.pointer("/error/detail"))
        .or_else(|| value.pointer("/error/reason"))
        .or_else(|| value.get("message"))
        .or_else(|| value.get("detail"))
        .or_else(|| value.get("reason"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string);

    let code = value
        .get("code")
        .or_else(|| value.pointer("/error/code"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToString::to_string);

    match (code, message) {
        (_, Some(message)) => Some(message),
        (Some(code), None) => Some(code),
        _ => None,
    }
}

fn parse_codex_function_call_item(item: &Value) -> Vec<SessionEvent> {
    let tool_name = item.get("name").and_then(Value::as_str).unwrap_or("unknown_tool").to_string();
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .or_else(|| item.get("arguments").cloned())
        .unwrap_or_else(|| json!({}));
    vec![SessionEvent::ToolCall { tool_name, arguments, server: None }]
}

fn parse_codex_function_call_output_item(item: &Value) -> Vec<SessionEvent> {
    let tool_name = item
        .get("call_id")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown_tool".to_string());
    let output = item.get("output").cloned().unwrap_or(Value::Null);
    let success = !item.get("error").is_some();
    vec![SessionEvent::ToolResult { tool_name, output, success }]
}

fn parse_codex_shell_call_item(item: &Value) -> Vec<SessionEvent> {
    let arguments = item.get("action").cloned().unwrap_or_else(|| json!({}));
    vec![SessionEvent::ToolCall { tool_name: "shell".to_string(), arguments, server: None }]
}

fn parse_codex_shell_call_output_item(item: &Value) -> Vec<SessionEvent> {
    let output = item.get("output").cloned().unwrap_or(Value::Null);
    let exit_code = item.pointer("/metadata/exit_code").and_then(Value::as_i64);
    let success = exit_code.map_or(true, |c| c == 0);
    vec![SessionEvent::ToolResult { tool_name: "shell".to_string(), output, success }]
}
