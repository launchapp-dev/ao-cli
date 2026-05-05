//! AO STDIO provider plugin for OpenAI Codex CLI.
//!
//! Wraps `llm-cli-wrapper`'s CodexSessionBackend in the JSON-RPC 2.0 STDIO
//! plugin protocol so AO dispatches Codex agent runs through this plugin
//! binary rather than calling the wrapper crate directly.

use std::collections::HashMap;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use cli_wrapper::session::{
    session_backend::SessionBackend, session_event::SessionEvent, session_request::SessionRequest,
    CodexSessionBackend,
};
use orchestrator_plugin_protocol::{
    error_codes, HealthCheckResult, HealthStatus, InitializeResult, PluginCapabilities, PluginInfo, PluginManifest,
    RpcError, RpcRequest, RpcResponse, PROTOCOL_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

const PLUGIN_NAME: &str = "ao-provider-codex";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const PLUGIN_KIND: &str = orchestrator_plugin_protocol::PLUGIN_KIND_PROVIDER;

#[derive(Debug, Deserialize)]
struct AgentRunParams {
    #[serde(default)]
    session_id: Option<String>,
    prompt: String,
    #[serde(default)]
    model: Option<String>,
    cwd: PathBuf,
    #[serde(default)]
    project_root: Option<PathBuf>,
    #[serde(default)]
    system_prompt: Option<String>,
    #[serde(default)]
    permission_mode: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    mcp_servers: Option<Value>,
    #[serde(default)]
    tools: Option<Value>,
    #[serde(default)]
    response_schema: Option<Value>,
    #[serde(default)]
    runtime_contract: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct AgentCancelParams {
    session_id: String,
}

fn manifest() -> PluginManifest {
    PluginManifest {
        name: PLUGIN_NAME.to_string(),
        version: PLUGIN_VERSION.to_string(),
        plugin_kind: PLUGIN_KIND.to_string(),
        description: "OpenAI Codex provider for AO (wraps llm-cli-wrapper codex backend)".to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: vec![
            "agent/run".to_string(),
            "agent/cancel".to_string(),
            "agent/resume".to_string(),
            "health/check".to_string(),
        ],
    }
}

fn initialize_result() -> InitializeResult {
    InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        plugin_info: PluginInfo {
            name: PLUGIN_NAME.to_string(),
            version: PLUGIN_VERSION.to_string(),
            plugin_kind: PLUGIN_KIND.to_string(),
        },
        capabilities: PluginCapabilities {
            methods: vec![
                "agent/run".to_string(),
                "agent/cancel".to_string(),
                "agent/resume".to_string(),
                "health/check".to_string(),
            ],
            streaming: false,
            projections: Vec::new(),
            subject_kinds: Vec::new(),
            mcp_tools: Vec::new(),
        },
    }
}

fn print_manifest_and_exit() -> ! {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{}", serde_json::to_string(&manifest()).expect("serialize manifest"));
    let _ = stdout.flush();
    std::process::exit(0);
}

fn handle_cli_args() {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--manifest" | "-m" => print_manifest_and_exit(),
            "--help" | "-h" => {
                eprintln!("ao-provider-codex {PLUGIN_VERSION} — STDIO provider plugin for OpenAI Codex CLI");
                eprintln!("Usage:");
                eprintln!("  ao-provider-codex --manifest    Print plugin manifest as JSON and exit");
                eprintln!("  ao-provider-codex               Run JSON-RPC loop on stdin/stdout");
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    handle_cli_args();

    if io::stdin().is_terminal() {
        eprintln!("ao-provider-codex is a STDIO plugin; pipe JSON-RPC on stdin or pass --manifest");
        std::process::exit(2);
    }

    let backend = Arc::new(CodexSessionBackend::new());
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let request: RpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(error) => {
                tracing::warn!(%error, "invalid JSON-RPC frame");
                continue;
            }
        };

        let backend = backend.clone();
        let stdout = stdout.clone();
        tokio::spawn(async move {
            if let Some(response) = handle_request(request, backend).await {
                if let Ok(mut payload) = serde_json::to_string(&response) {
                    payload.push('\n');
                    let mut guard = stdout.lock().await;
                    let _ = guard.write_all(payload.as_bytes()).await;
                    let _ = guard.flush().await;
                }
            }
        });
    }

    Ok(())
}

async fn handle_request(request: RpcRequest, backend: Arc<CodexSessionBackend>) -> Option<RpcResponse> {
    let id = request.id.clone();
    match request.method.as_str() {
        "initialize" => {
            let result = serde_json::to_value(initialize_result()).expect("encode init");
            Some(RpcResponse::ok(id, result))
        }
        "initialized" => None,
        "$/ping" => Some(RpcResponse::ok(id, json!({}))),
        "health/check" => {
            let result = serde_json::to_value(HealthCheckResult {
                status: HealthStatus::Healthy,
                uptime_ms: None,
                memory_usage_bytes: None,
                last_error: None,
            })
            .expect("encode health");
            Some(RpcResponse::ok(id, result))
        }
        "agent/run" => Some(handle_agent_run(id, request.params, &backend, None).await),
        "agent/resume" => {
            let resume_session = request
                .params
                .as_ref()
                .and_then(|p| p.get("session_id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            Some(handle_agent_run(id, request.params, &backend, resume_session).await)
        }
        "agent/cancel" => Some(handle_agent_cancel(id, request.params, &backend).await),
        "shutdown" => Some(RpcResponse::ok(id, json!({}))),
        "exit" => std::process::exit(0),
        other => {
            if other.starts_with("$/") {
                None
            } else {
                Some(RpcResponse::err(
                    id,
                    RpcError {
                        code: error_codes::METHOD_NOT_FOUND,
                        message: format!("method '{other}' not implemented by ao-provider-codex"),
                        data: None,
                    },
                ))
            }
        }
    }
}

async fn handle_agent_run(
    id: Option<Value>,
    params: Option<Value>,
    backend: &CodexSessionBackend,
    resume_session: Option<String>,
) -> RpcResponse {
    let params: AgentRunParams = match params.ok_or_else(|| invalid_params("missing params for agent/run")) {
        Ok(p) => match serde_json::from_value::<AgentRunParams>(p) {
            Ok(parsed) => parsed,
            Err(error) => return invalid_rpc(id, format!("invalid agent/run params: {error}")),
        },
        Err(error) => return error_rpc(id, error),
    };

    let session_request = build_session_request(params);
    let started_at = Instant::now();

    let run_result = match resume_session {
        Some(ref sid) => backend.resume_session(session_request, sid).await,
        None => backend.start_session(session_request).await,
    };

    let mut run = match run_result {
        Ok(run) => run,
        Err(error) => {
            return error_rpc(
                id,
                RpcError {
                    code: -1002,
                    message: format!("codex session start failed: {error}"),
                    data: None,
                },
            )
        }
    };

    let session_id = run.session_id.clone();
    let backend_label = run.selected_backend.clone();
    let mut output = String::new();
    let mut metadata = Vec::<Value>::new();
    let mut tool_calls = Vec::<Value>::new();
    let mut tool_results = Vec::<Value>::new();
    let mut thinking = Vec::<String>::new();
    let mut errors = Vec::<String>::new();
    let mut exit_code: Option<i32> = None;

    while let Some(event) = run.events.recv().await {
        match event {
            SessionEvent::Started { .. } => {}
            SessionEvent::TextDelta { text } => output.push_str(&text),
            SessionEvent::FinalText { text } => {
                if !output.is_empty() && !output.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str(&text);
            }
            SessionEvent::Thinking { text } => thinking.push(text),
            SessionEvent::ToolCall { tool_name, arguments, server } => tool_calls.push(json!({
                "tool": tool_name,
                "arguments": arguments,
                "server": server,
            })),
            SessionEvent::ToolResult { tool_name, output: tool_output, success } => tool_results.push(json!({
                "tool": tool_name,
                "output": tool_output,
                "success": success,
            })),
            SessionEvent::Artifact { artifact_id, metadata: m } => metadata.push(json!({
                "artifact_id": artifact_id,
                "metadata": m,
            })),
            SessionEvent::Metadata { metadata: m } => metadata.push(m),
            SessionEvent::Error { message, recoverable } => {
                errors.push(message.clone());
                if !recoverable {
                    break;
                }
            }
            SessionEvent::Finished { exit_code: code } => {
                exit_code = code;
                break;
            }
        }
    }

    let duration_ms = started_at.elapsed().as_millis() as u64;
    let result = json!({
        "session_id": session_id,
        "exit_code": exit_code.unwrap_or(0),
        "output": output,
        "metadata": metadata,
        "tool_calls": tool_calls,
        "tool_results": tool_results,
        "thinking": thinking,
        "errors": errors,
        "duration_ms": duration_ms,
        "backend": backend_label,
    });
    RpcResponse::ok(id, result)
}

async fn handle_agent_cancel(
    id: Option<Value>,
    params: Option<Value>,
    backend: &CodexSessionBackend,
) -> RpcResponse {
    let params: AgentCancelParams = match params.ok_or_else(|| invalid_params("missing params for agent/cancel")) {
        Ok(p) => match serde_json::from_value::<AgentCancelParams>(p) {
            Ok(parsed) => parsed,
            Err(error) => return invalid_rpc(id, format!("invalid agent/cancel params: {error}")),
        },
        Err(error) => return error_rpc(id, error),
    };

    match backend.terminate_session(&params.session_id).await {
        Ok(()) => RpcResponse::ok(id, json!({ "session_id": params.session_id, "cancelled": true })),
        Err(error) => error_rpc(
            id,
            RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: format!("codex session terminate failed: {error}"),
                data: None,
            },
        ),
    }
}

fn build_session_request(params: AgentRunParams) -> SessionRequest {
    let mut extras = serde_json::Map::new();
    if let Some(system_prompt) = params.system_prompt {
        extras.insert("system_prompt".to_string(), Value::String(system_prompt));
    }
    if let Some(mcp) = params.mcp_servers {
        extras.insert("mcp_servers".to_string(), mcp);
    }
    if let Some(tools) = params.tools {
        extras.insert("tools".to_string(), tools);
    }
    if let Some(schema) = params.response_schema {
        extras.insert("response_schema".to_string(), schema);
    }
    if let Some(contract) = params.runtime_contract {
        extras.insert("runtime_contract".to_string(), contract);
    }
    if let Some(sid) = params.session_id {
        extras.insert("session_id".to_string(), Value::String(sid));
    }

    SessionRequest {
        tool: "codex".to_string(),
        model: params.model.unwrap_or_else(|| "gpt-5".to_string()),
        prompt: params.prompt,
        cwd: params.cwd,
        project_root: params.project_root,
        mcp_endpoint: None,
        permission_mode: params.permission_mode,
        timeout_secs: params.timeout_secs,
        env_vars: params.env.into_iter().collect(),
        extras: Value::Object(extras),
    }
}

fn invalid_params(message: impl Into<String>) -> RpcError {
    RpcError { code: error_codes::INVALID_PARAMS, message: message.into(), data: None }
}

fn invalid_rpc(id: Option<Value>, message: impl Into<String>) -> RpcResponse {
    RpcResponse::err(id, RpcError { code: error_codes::INVALID_PARAMS, message: message.into(), data: None })
}

fn error_rpc(id: Option<Value>, error: RpcError) -> RpcResponse {
    RpcResponse::err(id, error)
}
