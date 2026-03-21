use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

use crate::api::client::ApiClient;
use crate::api::types::*;
use crate::config::StructuredOutputSupport;
use crate::tools::{executor, mcp_client};

use super::context;
use super::output::OutputFormatter;

const SCHEMA_RETRY_LIMIT: usize = 3;
const DEFAULT_MAX_SESSIONS: usize = 100;
const DEFAULT_SESSION_MAX_AGE_HOURS: u64 = 168; // 7 days

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn config_dir() -> PathBuf {
    let dir = std::env::var("AO_CONFIG_DIR")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.ao", h)))
        .unwrap_or_else(|_| ".ao".to_string());
    PathBuf::from(dir)
}

fn session_file_path_in(base: &Path, session_id: &str) -> PathBuf {
    base.join("sessions").join(format!("{}.json", session_id))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SessionTokenUsage {
    pub total_input: u64,
    pub total_output: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionMetadata {
    pub created_at: u64,
    pub last_updated: u64,
    pub turn_count: u32,
    pub token_usage: SessionTokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionFile {
    metadata: SessionMetadata,
    messages: Vec<ChatMessage>,
}

fn load_session_from(base: &Path, session_id: &str) -> (Vec<ChatMessage>, Option<SessionMetadata>) {
    let path = session_file_path_in(base, session_id);
    if !path.exists() {
        return (Vec::new(), None);
    }
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return (Vec::new(), None),
    };
    // Try new format with metadata first
    if let Ok(session_file) = serde_json::from_str::<SessionFile>(&data) {
        return (session_file.messages, Some(session_file.metadata));
    }
    // Fall back to legacy Vec<ChatMessage> format
    if let Ok(messages) = serde_json::from_str::<Vec<ChatMessage>>(&data) {
        return (messages, None);
    }
    // Corrupted file — start fresh
    eprintln!("[oai-runner] Warning: session file {} is corrupted, starting fresh", path.display());
    (Vec::new(), None)
}

#[cfg(test)]
fn load_session_messages_from(base: &Path, session_id: &str) -> Vec<ChatMessage> {
    load_session_from(base, session_id).0
}

fn save_session_to(
    base: &Path,
    session_id: &str,
    messages: &[ChatMessage],
    turn_count: u32,
    token_usage: &SessionTokenUsage,
    existing_metadata: Option<&SessionMetadata>,
) -> Result<()> {
    let path = session_file_path_in(base, session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let now = unix_now();
    let created_at = existing_metadata.map(|m| m.created_at).unwrap_or(now);
    let metadata = SessionMetadata { created_at, last_updated: now, turn_count, token_usage: token_usage.clone() };
    let session_file = SessionFile { metadata, messages: messages.to_vec() };
    let data = serde_json::to_string_pretty(&session_file)?;
    std::fs::write(&path, data)?;
    Ok(())
}

fn delete_session(base: &Path, session_id: &str) {
    let path = session_file_path_in(base, session_id);
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            eprintln!("[oai-runner] Warning: failed to delete session {}: {}", session_id, e);
        }
    }
}

fn max_sessions_limit() -> usize {
    std::env::var("AO_MAX_SESSIONS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(DEFAULT_MAX_SESSIONS)
}

fn session_max_age_secs() -> u64 {
    std::env::var("AO_SESSION_MAX_AGE_HOURS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SESSION_MAX_AGE_HOURS)
        * 3600
}

fn read_session_last_updated(path: &Path) -> u64 {
    if let Ok(data) = std::fs::read_to_string(path) {
        if let Ok(sf) = serde_json::from_str::<SessionFile>(&data) {
            return sf.metadata.last_updated;
        }
    }
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn enforce_session_limit(base: &Path, max_sessions: usize) {
    let sessions_dir = base.join("sessions");
    if !sessions_dir.exists() {
        return;
    }
    let mut sessions: Vec<(u64, PathBuf)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&sessions_dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let last_updated = read_session_last_updated(&path);
        sessions.push((last_updated, path));
    }
    if sessions.len() <= max_sessions {
        return;
    }
    sessions.sort_by_key(|(ts, _)| *ts); // oldest first
    let to_remove = sessions.len() - max_sessions;
    for (_, path) in sessions.iter().take(to_remove) {
        let _ = std::fs::remove_file(path);
    }
}

fn cleanup_expired_sessions(base: &Path, max_age_secs: u64) {
    let sessions_dir = base.join("sessions");
    if !sessions_dir.exists() {
        return;
    }
    let now = unix_now();
    let Ok(entries) = std::fs::read_dir(&sessions_dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let last_updated = read_session_last_updated(&path);
        if now.saturating_sub(last_updated) > max_age_secs {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn build_json_schema_format(schema: &Value) -> ResponseFormat {
    let mut strict_schema = schema.clone();
    if let Some(obj) = strict_schema.as_object_mut() {
        obj.entry("additionalProperties").or_insert(serde_json::Value::Bool(false));
    }
    ResponseFormat {
        type_: "json_schema".to_string(),
        json_schema: Some(JsonSchemaSpec { name: "phase_output".to_string(), strict: true, schema: strict_schema }),
    }
}

fn build_json_object_format() -> ResponseFormat {
    ResponseFormat { type_: "json_object".to_string(), json_schema: None }
}

fn build_schema_injection(schema: &Value) -> String {
    format!(
        "\n\nIMPORTANT: Your final response MUST be a single valid JSON object matching this exact schema. \
         Do not wrap it in markdown. Do not add explanation. Output ONLY the JSON.\n\nRequired JSON Schema:\n{}",
        serde_json::to_string_pretty(schema).unwrap_or_default()
    )
}

fn synthesize_fallback(model: &str, summary: &str, confidence: f64) -> Value {
    serde_json::json!({
        "kind": "implementation_result",
        "commit_message": format!("Implementation by {}", model),
        "phase_decision": {
            "kind": "phase_decision",
            "verdict": "rework",
            "confidence": confidence,
            "risk": "high",
            "reason": format!("Agent did not produce valid structured output. Summary: {}", summary)
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn run_agent_loop(
    client: &ApiClient,
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    tools: &[ToolDefinition],
    working_dir: &Path,
    max_turns: usize,
    output: &mut OutputFormatter,
    response_schema: Option<&Value>,
    mcp_clients: &[mcp_client::McpClient],
    session_id: Option<&str>,
    structured_output: Option<StructuredOutputSupport>,
    cancel_token: CancellationToken,
    context_limit: usize,
    max_tokens: usize,
) -> Result<()> {
    let base = config_dir();
    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut turn_count: u32 = 0;
    let mut accumulated_usage = SessionTokenUsage::default();
    let mut existing_metadata: Option<SessionMetadata> = None;

    if let Some(sid) = session_id {
        // Cleanup housekeeping before loading
        cleanup_expired_sessions(&base, session_max_age_secs());
        enforce_session_limit(&base, max_sessions_limit().saturating_sub(1));

        let (prior_messages, prior_metadata) = load_session_from(&base, sid);
        if !prior_messages.is_empty() {
            eprintln!("[oai-runner] Resuming session {} ({} prior messages)", sid, prior_messages.len());
            // Carry forward accumulated metadata from prior runs
            if let Some(ref meta) = prior_metadata {
                turn_count = meta.turn_count;
                accumulated_usage = SessionTokenUsage {
                    total_input: meta.token_usage.total_input,
                    total_output: meta.token_usage.total_output,
                    total: meta.token_usage.total,
                };
            }
            messages.extend(prior_messages);
        }
        existing_metadata = prior_metadata;
    }

    let needs_schema_in_prompt =
        structured_output == Some(StructuredOutputSupport::JsonObjectOnly) && response_schema.is_some();

    if messages.is_empty() {
        let mut sys = system_prompt.to_string();
        if needs_schema_in_prompt {
            sys.push_str(&build_schema_injection(response_schema.unwrap()));
        }
        if !sys.is_empty() {
            messages.push(ChatMessage {
                reasoning_content: None,
                role: "system".to_string(),
                content: Some(sys),
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }

    messages.push(ChatMessage {
        reasoning_content: None,
        role: "user".to_string(),
        content: Some(user_prompt.to_string()),
        tool_calls: None,
        tool_call_id: None,
    });

    let needs_tool_name_sanitization = model.contains("kimi");
    let sanitized_tools: Vec<ToolDefinition> = if needs_tool_name_sanitization {
        tools
            .iter()
            .map(|t| {
                let mut t = t.clone();
                t.function.name = t.function.name.replace('.', "_");
                t
            })
            .collect()
    } else {
        tools.to_vec()
    };
    let api_tools = if needs_tool_name_sanitization { &sanitized_tools } else { tools };

    for turn in 0..max_turns {
        if cancel_token.is_cancelled() {
            eprintln!("[oai-runner] Cancelled by signal");
            if let Some(sid) = session_id {
                if let Err(e) =
                    save_session_to(&base, sid, &messages, turn_count, &accumulated_usage, existing_metadata.as_ref())
                {
                    eprintln!("[oai-runner] Warning: failed to save session on cancel {}: {}", sid, e);
                }
            }
            output.emit_session_summary();
            anyhow::bail!("Cancelled by shutdown signal");
        }

        context::truncate_to_fit(&mut messages, context_limit, max_tokens);

        let format = match structured_output {
            Some(StructuredOutputSupport::JsonSchema) => response_schema.map(build_json_schema_format),
            Some(StructuredOutputSupport::JsonObjectOnly) if response_schema.is_some() => {
                Some(build_json_object_format())
            }
            _ => None,
        };

        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.clone(),
            stream: true,
            tools: Some(api_tools.to_vec()),
            max_tokens: Some(max_tokens as u32),
            response_format: format,
            stream_options: Some(StreamOptions { include_usage: true }),
        };

        let (assistant_msg, usage) = client
            .stream_chat(&request, &mut |chunk| {
                output.text_chunk(chunk);
            })
            .await?;

        if let Some(u) = &usage {
            output.metadata(u);
            accumulated_usage.total_input += u.prompt_tokens;
            accumulated_usage.total_output += u.completion_tokens;
            accumulated_usage.total += u.effective_total();
        }

        turn_count += 1;
        let has_tool_calls = assistant_msg.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty());

        messages.push(assistant_msg.clone());

        if let Some(sid) = session_id {
            let _ = save_session_to(&base, sid, &messages, turn_count, &accumulated_usage, existing_metadata.as_ref());
        }

        if !has_tool_calls {
            output.flush_result();
            let content = assistant_msg.content.as_deref().unwrap_or("");
            let mut schema_ok = true;
            if let Some(schema) = response_schema {
                if let Err(errors) = validate_output_against_schema(content, schema) {
                    let system_msg = messages.iter().find(|m| m.role == "system").cloned();
                    let corrected = retry_schema_validation(
                        client,
                        model,
                        system_msg.as_ref(),
                        &mut messages,
                        schema,
                        &errors,
                        output,
                        structured_output,
                    )
                    .await;
                    if !corrected {
                        eprintln!(
                            "Warning: schema validation failed after {} retries, synthesizing fallback result",
                            SCHEMA_RETRY_LIMIT
                        );
                        schema_ok = false;
                    }
                }
            }
            if !schema_ok {
                let summary = content.chars().take(200).collect::<String>();
                let fallback = synthesize_fallback(model, &summary, 0.4);
                let fallback_str = serde_json::to_string(&fallback).unwrap_or_default();
                output.text_chunk(&fallback_str);
                output.flush_result();
            }
            // Session completed successfully — persist final state then remove file
            if let Some(sid) = session_id {
                let _ = save_session_to(&base, sid, &messages, turn_count, &accumulated_usage, existing_metadata.as_ref());
                delete_session(&base, sid);
            }
            output.emit_session_summary();
            output.newline();
            return Ok(());
        }

        let tool_calls = assistant_msg.tool_calls.as_ref().unwrap();

        for tc in tool_calls {
            if cancel_token.is_cancelled() {
                eprintln!("[oai-runner] Cancelled between tool calls");
                break;
            }

            let tool_name = if needs_tool_name_sanitization {
                tc.function.name.replace('_', ".")
            } else {
                tc.function.name.clone()
            };

            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::Value::Null);

            output.tool_call(&tool_name, &args);

            let result = if let Some(mcp) = mcp_client::find_client_for_tool(mcp_clients, &tool_name) {
                match mcp_client::call_tool(mcp, &tool_name, &tc.function.arguments).await {
                    Ok(r) => {
                        output.tool_result(&tool_name, &r);
                        r
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        output.tool_error(&tool_name, &err_msg);
                        format!("Error: {}", err_msg)
                    }
                }
            } else {
                match executor::execute_tool(&tool_name, &tc.function.arguments, working_dir).await {
                    Ok(r) => {
                        output.tool_result(&tool_name, &r);
                        r
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        output.tool_error(&tool_name, &err_msg);
                        format!("Error: {}", err_msg)
                    }
                }
            };

            messages.push(ChatMessage {
                reasoning_content: None,
                role: "tool".to_string(),
                content: Some(result),
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
            });

            if let Some(sid) = session_id {
                let _ = save_session_to(
                    &base,
                    sid,
                    &messages,
                    turn_count,
                    &accumulated_usage,
                    existing_metadata.as_ref(),
                );
            }
        }

        if turn == max_turns - 1 {
            eprintln!("Warning: reached maximum turns ({}). Stopping.", max_turns);
        }
    }

    if let Some(sid) = session_id {
        if let Err(e) = save_session_to(&base, sid, &messages, turn_count, &accumulated_usage, existing_metadata.as_ref()) {
            eprintln!("[oai-runner] Warning: failed to save session {}: {}", sid, e);
        }
    }
    output.flush_result();
    if response_schema.is_some() {
        eprintln!("[oai-runner] Max turns reached, synthesizing fallback result");
        let fallback = synthesize_fallback(model, "Agent reached maximum turns. Work may be partially complete.", 0.3);
        let fallback_str = serde_json::to_string(&fallback).unwrap_or_default();
        output.text_chunk(&fallback_str);
        output.flush_result();
    }
    output.emit_session_summary();
    output.newline();
    Ok(())
}

async fn retry_schema_validation(
    client: &ApiClient,
    model: &str,
    system_msg: Option<&ChatMessage>,
    messages: &mut Vec<ChatMessage>,
    schema: &Value,
    initial_errors: &str,
    output: &mut OutputFormatter,
    structured_output: Option<StructuredOutputSupport>,
) -> bool {
    let mut last_errors = initial_errors.to_string();

    let last_assistant_content =
        messages.iter().rev().find(|m| m.role == "assistant").and_then(|m| m.content.clone()).unwrap_or_default();

    for attempt in 1..=SCHEMA_RETRY_LIMIT {
        eprintln!("Schema validation failed (attempt {}/{}): {}", attempt, SCHEMA_RETRY_LIMIT, last_errors);

        let correction = format!(
            "Your last response did not match the required output JSON schema. Errors:\n{}\n\n\
             The required schema is:\n{}\n\n\
             Please respond with ONLY a valid JSON object matching the schema above. No markdown, no explanation, just the raw JSON.",
            last_errors,
            serde_json::to_string_pretty(schema).unwrap_or_default()
        );

        let mut retry_messages: Vec<ChatMessage> = Vec::new();
        if let Some(sys) = system_msg {
            retry_messages.push(sys.clone());
        }
        retry_messages.push(ChatMessage {
            reasoning_content: None,
            role: "assistant".to_string(),
            content: Some(last_assistant_content.clone()),
            tool_calls: None,
            tool_call_id: None,
        });
        retry_messages.push(ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some(correction.clone()),
            tool_calls: None,
            tool_call_id: None,
        });

        let retry_request = ChatRequest {
            model: model.to_string(),
            messages: retry_messages,
            stream: true,
            tools: None,
            max_tokens: Some(4096),
            response_format: Some(match structured_output {
                Some(StructuredOutputSupport::JsonObjectOnly) | None => build_json_object_format(),
                _ => build_json_schema_format(schema),
            }),
            stream_options: Some(StreamOptions { include_usage: true }),
        };

        let retry_result = client
            .stream_chat(&retry_request, &mut |chunk| {
                output.text_chunk(chunk);
            })
            .await;

        let (retry_msg, usage) = match retry_result {
            Ok(r) => r,
            Err(_) => return false,
        };

        if let Some(u) = &usage {
            output.metadata(u);
        }

        let content = retry_msg.content.clone().unwrap_or_default();
        messages.push(ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some(correction),
            tool_calls: None,
            tool_call_id: None,
        });
        messages.push(retry_msg);

        match validate_output_against_schema(&content, schema) {
            Ok(()) => return true,
            Err(errors) => last_errors = errors,
        }
    }

    false
}

fn validate_output_against_schema(content: &str, schema: &Value) -> std::result::Result<(), String> {
    let parsed = extract_json_from_content(content)
        .ok_or_else(|| "Response does not contain valid JSON. Expected a JSON object.".to_string())?;

    let validator = jsonschema::validator_for(schema).map_err(|e| format!("Invalid schema: {}", e))?;

    let errors: Vec<String> = validator
        .iter_errors(&parsed)
        .map(|e| {
            let path = e.instance_path().to_string();
            if path.is_empty() {
                format!("{}", e)
            } else {
                format!("at '{}': {}", path, e)
            }
        })
        .collect();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn extract_json_from_content(content: &str) -> Option<Value> {
    let trimmed = content.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Some(v);
    }

    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.find("```") {
            if let Ok(v) = serde_json::from_str::<Value>(after[..end].trim()) {
                return Some(v);
            }
        }
    }

    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        if let Some(end) = after.find("```") {
            if let Ok(v) = serde_json::from_str::<Value>(after[..end].trim()) {
                return Some(v);
            }
        }
    }

    // Fallback: find the first '{' and last '}' and try to parse everything in between.
    // This handles multi-line JSON that isn't wrapped in markdown.
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end > start {
            let potential_json = &trimmed[start..=end];
            if let Ok(v) = serde_json::from_str::<Value>(potential_json) {
                return Some(v);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_valid_json_against_schema() {
        let schema = json!({
            "type": "object",
            "required": ["kind", "verdict"],
            "properties": {
                "kind": { "const": "phase_decision" },
                "verdict": { "type": "string", "enum": ["advance", "rework", "fail"] }
            }
        });
        let content = r#"{"kind":"phase_decision","verdict":"advance","reason":"all good"}"#;
        assert!(validate_output_against_schema(content, &schema).is_ok());
    }

    #[test]
    fn rejects_missing_required_field() {
        let schema = json!({
            "type": "object",
            "required": ["kind", "verdict"],
            "properties": {
                "kind": { "const": "phase_decision" },
                "verdict": { "type": "string" }
            }
        });
        let content = r#"{"kind":"phase_decision"}"#;
        let err = validate_output_against_schema(content, &schema).unwrap_err();
        assert!(err.contains("verdict"), "Error should mention 'verdict': {}", err);
    }

    #[test]
    fn rejects_wrong_type() {
        let schema = json!({
            "type": "object",
            "required": ["confidence"],
            "properties": {
                "confidence": { "type": "number" }
            }
        });
        let content = r#"{"confidence":"high"}"#;
        let err = validate_output_against_schema(content, &schema).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn rejects_wrong_const() {
        let schema = json!({
            "type": "object",
            "required": ["kind"],
            "properties": {
                "kind": { "const": "phase_decision" }
            }
        });
        let content = r#"{"kind":"something_else"}"#;
        let err = validate_output_against_schema(content, &schema).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn rejects_invalid_enum_value() {
        let schema = json!({
            "type": "object",
            "required": ["verdict"],
            "properties": {
                "verdict": { "type": "string", "enum": ["advance", "rework", "fail"] }
            }
        });
        let content = r#"{"verdict":"maybe"}"#;
        let err = validate_output_against_schema(content, &schema).unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn extracts_json_from_markdown_code_block() {
        let schema = json!({
            "type": "object",
            "required": ["kind"],
            "properties": {
                "kind": { "const": "phase_decision" }
            }
        });
        let content = "Here is my assessment:\n```json\n{\"kind\":\"phase_decision\"}\n```\n";
        assert!(validate_output_against_schema(content, &schema).is_ok());
    }

    #[test]
    fn extracts_json_from_inline_line() {
        let schema = json!({
            "type": "object",
            "required": ["kind"],
            "properties": {
                "kind": { "const": "phase_decision" }
            }
        });
        let content = "My analysis is complete.\n{\"kind\":\"phase_decision\"}\nDone.";
        assert!(validate_output_against_schema(content, &schema).is_ok());
    }

    #[test]
    fn rejects_non_json_content() {
        let schema = json!({
            "type": "object",
            "required": ["kind"],
            "properties": {}
        });
        let content = "This is just plain text with no JSON at all.";
        let err = validate_output_against_schema(content, &schema).unwrap_err();
        assert!(err.contains("does not contain valid JSON"));
    }

    #[test]
    fn validates_nested_objects() {
        let schema = json!({
            "type": "object",
            "required": ["phase_decision"],
            "properties": {
                "phase_decision": {
                    "type": "object",
                    "required": ["verdict", "confidence"],
                    "properties": {
                        "verdict": { "type": "string", "enum": ["advance", "rework", "fail"] },
                        "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
                    }
                }
            }
        });
        let valid = r#"{"phase_decision":{"verdict":"advance","confidence":0.9}}"#;
        assert!(validate_output_against_schema(valid, &schema).is_ok());

        let invalid = r#"{"phase_decision":{"verdict":"maybe","confidence":0.9}}"#;
        assert!(validate_output_against_schema(invalid, &schema).is_err());

        let missing = r#"{"phase_decision":{"verdict":"advance"}}"#;
        assert!(validate_output_against_schema(missing, &schema).is_err());
    }

    #[test]
    fn validates_one_of() {
        let schema = json!({
            "oneOf": [
                {
                    "type": "object",
                    "required": ["kind"],
                    "properties": { "kind": { "const": "success" } }
                },
                {
                    "type": "object",
                    "required": ["kind"],
                    "properties": { "kind": { "const": "failure" } }
                }
            ]
        });
        let valid = r#"{"kind":"success"}"#;
        assert!(validate_output_against_schema(valid, &schema).is_ok());

        let invalid = r#"{"kind":"other"}"#;
        assert!(validate_output_against_schema(invalid, &schema).is_err());
    }

    #[test]
    fn validates_array_min_items() {
        let schema = json!({
            "type": "object",
            "required": ["items"],
            "properties": {
                "items": { "type": "array", "minItems": 1 }
            }
        });
        let valid = r#"{"items":["a"]}"#;
        assert!(validate_output_against_schema(valid, &schema).is_ok());

        let invalid = r#"{"items":[]}"#;
        assert!(validate_output_against_schema(invalid, &schema).is_err());
    }

    #[test]
    fn validates_string_pattern() {
        let schema = json!({
            "type": "object",
            "required": ["version"],
            "properties": {
                "version": { "type": "string", "pattern": "^\\d+\\.\\d+\\.\\d+$" }
            }
        });
        let valid = r#"{"version":"1.2.3"}"#;
        assert!(validate_output_against_schema(valid, &schema).is_ok());

        let invalid = r#"{"version":"not-a-version"}"#;
        assert!(validate_output_against_schema(invalid, &schema).is_err());
    }

    #[test]
    fn fallback_uses_rework_verdict() {
        let fallback = synthesize_fallback("test-model", "test summary", 0.4);
        let decision = &fallback["phase_decision"];
        assert_eq!(decision["verdict"], "rework");
        assert_eq!(decision["confidence"], 0.4);
        assert_eq!(decision["risk"], "high");
    }

    #[test]
    fn session_save_and_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let sid = "test-session-round-trip";
        let messages = vec![
            ChatMessage {
                reasoning_content: None,
                role: "system".to_string(),
                content: Some("You are helpful.".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                reasoning_content: None,
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                reasoning_content: None,
                role: "assistant".to_string(),
                content: Some("Hi there!".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let usage = SessionTokenUsage { total_input: 100, total_output: 50, total: 150 };
        save_session_to(base, sid, &messages, 1, &usage, None).unwrap();
        let loaded = load_session_messages_from(base, sid);
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].role, "system");
        assert_eq!(loaded[1].content.as_deref(), Some("Hello"));
        assert_eq!(loaded[2].content.as_deref(), Some("Hi there!"));
    }

    #[test]
    fn session_metadata_persists_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let sid = "test-metadata";
        let messages = vec![ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some("test".to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];
        let usage = SessionTokenUsage { total_input: 200, total_output: 80, total: 280 };
        save_session_to(base, sid, &messages, 3, &usage, None).unwrap();

        let (loaded_msgs, meta) = load_session_from(base, sid);
        assert_eq!(loaded_msgs.len(), 1);
        let meta = meta.unwrap();
        assert_eq!(meta.turn_count, 3);
        assert_eq!(meta.token_usage.total_input, 200);
        assert_eq!(meta.token_usage.total_output, 80);
        assert_eq!(meta.token_usage.total, 280);
        assert!(meta.created_at > 0);
        assert!(meta.last_updated >= meta.created_at);
    }

    #[test]
    fn session_metadata_preserves_created_at_on_update() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let sid = "test-created-at";
        let messages = vec![ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some("first".to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];
        let usage = SessionTokenUsage::default();
        save_session_to(base, sid, &messages, 1, &usage, None).unwrap();

        let (_, first_meta) = load_session_from(base, sid);
        let first_meta = first_meta.unwrap();
        let original_created_at = first_meta.created_at;

        // Update the session, preserving created_at
        let updated_messages = vec![
            messages[0].clone(),
            ChatMessage {
                reasoning_content: None,
                role: "assistant".to_string(),
                content: Some("second".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        save_session_to(base, sid, &updated_messages, 2, &usage, Some(&first_meta)).unwrap();

        let (_, updated_meta) = load_session_from(base, sid);
        let updated_meta = updated_meta.unwrap();
        assert_eq!(updated_meta.created_at, original_created_at);
        assert_eq!(updated_meta.turn_count, 2);
    }

    #[test]
    fn load_nonexistent_session_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_session_messages_from(dir.path(), "nonexistent-session-id");
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_legacy_format_session_returns_messages() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let sid = "legacy-session";
        // Write old Vec<ChatMessage> format directly
        let sessions_dir = base.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let path = sessions_dir.join(format!("{}.json", sid));
        let legacy_data = serde_json::json!([
            {"role": "user", "content": "hello", "tool_calls": null, "tool_call_id": null}
        ]);
        std::fs::write(&path, legacy_data.to_string()).unwrap();

        let (messages, meta) = load_session_from(base, sid);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert!(meta.is_none(), "legacy format should have no metadata");
    }

    #[test]
    fn load_corrupted_session_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let sid = "corrupted-session";
        let sessions_dir = base.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let path = sessions_dir.join(format!("{}.json", sid));
        std::fs::write(&path, "this is not valid json at all!!!").unwrap();

        let (messages, meta) = load_session_from(base, sid);
        assert!(messages.is_empty(), "corrupted session should return empty messages");
        assert!(meta.is_none());
    }

    #[test]
    fn delete_session_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let sid = "delete-me";
        let messages = vec![ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some("bye".to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];
        save_session_to(base, sid, &messages, 1, &SessionTokenUsage::default(), None).unwrap();
        assert!(session_file_path_in(base, sid).exists());

        delete_session(base, sid);
        assert!(!session_file_path_in(base, sid).exists());
    }

    #[test]
    fn enforce_session_limit_evicts_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sessions_dir = base.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let messages = vec![ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some("x".to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        // Create 5 sessions with distinct timestamps
        for i in 0..5u64 {
            let sid = format!("session-{}", i);
            let usage = SessionTokenUsage::default();
            save_session_to(base, &sid, &messages, 1, &usage, None).unwrap();
            // Manually set a distinct last_updated by patching the file
            let path = session_file_path_in(base, &sid);
            let data = std::fs::read_to_string(&path).unwrap();
            let mut sf: SessionFile = serde_json::from_str(&data).unwrap();
            sf.metadata.last_updated = 1000 + i; // older sessions have smaller timestamps
            sf.metadata.created_at = 1000 + i;
            std::fs::write(&path, serde_json::to_string_pretty(&sf).unwrap()).unwrap();
        }

        // Enforce limit of 3 — should evict session-0 and session-1 (oldest)
        enforce_session_limit(base, 3);

        assert!(!session_file_path_in(base, "session-0").exists(), "oldest session should be evicted");
        assert!(!session_file_path_in(base, "session-1").exists(), "second oldest should be evicted");
        assert!(session_file_path_in(base, "session-2").exists());
        assert!(session_file_path_in(base, "session-3").exists());
        assert!(session_file_path_in(base, "session-4").exists());
    }

    #[test]
    fn cleanup_expired_sessions_removes_old() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sessions_dir = base.join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();

        let messages = vec![ChatMessage {
            reasoning_content: None,
            role: "user".to_string(),
            content: Some("x".to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        // Create an expired session (last_updated = 0, very old)
        let sid_old = "old-session";
        save_session_to(base, sid_old, &messages, 1, &SessionTokenUsage::default(), None).unwrap();
        let path_old = session_file_path_in(base, sid_old);
        let data = std::fs::read_to_string(&path_old).unwrap();
        let mut sf: SessionFile = serde_json::from_str(&data).unwrap();
        sf.metadata.last_updated = 1; // epoch + 1 second, definitely expired
        std::fs::write(&path_old, serde_json::to_string_pretty(&sf).unwrap()).unwrap();

        // Create a fresh session (last_updated = now)
        let sid_new = "new-session";
        save_session_to(base, sid_new, &messages, 1, &SessionTokenUsage::default(), None).unwrap();

        // Cleanup with 1 hour max age
        cleanup_expired_sessions(base, 3600);

        assert!(!session_file_path_in(base, sid_old).exists(), "expired session should be removed");
        assert!(session_file_path_in(base, sid_new).exists(), "fresh session should remain");
    }
}
