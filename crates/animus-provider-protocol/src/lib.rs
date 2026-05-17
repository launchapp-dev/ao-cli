//! `ProviderBackend` trait and request/response shapes for Animus LLM
//! provider plugins.
//!
//! Provider plugins wrap an LLM CLI (Claude Code, Codex, Gemini, opencode,
//! ...) or HTTP API (OpenAI-compatible, on-prem hosted models, ...) so the
//! Animus daemon can spawn agent runs through a uniform interface. Each
//! provider runs as its own stdio child process, just like subject backends,
//! and speaks the same JSON-RPC 2.0 envelope defined in
//! [`animus-plugin-protocol`].
//!
//! The trait below is the Rust-side surface plugin authors implement.
//! Wire-level method names (`agent/run`, `agent/resume`, `agent/cancel`,
//! `health/check`) are exported as constants so non-Rust SDK authors can bind
//! to the same names.
//!
//! Streaming results (`agent/output`, `agent/thinking`, `agent/toolCall`,
//! `agent/toolResult`, `agent/error`) are emitted as JSON-RPC notifications
//! carrying the original `agent/run` request id. The runtime in
//! [`animus-plugin-runtime`] handles wiring the trait's event channel onto
//! the wire; trait implementers only emit events.

#![warn(missing_docs)]

use std::collections::HashMap;
use std::path::PathBuf;

use animus_plugin_protocol::{error_codes, HealthCheckResult, RpcError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// =====================================================================
// Method-name constants
// =====================================================================

/// `agent/run` — start a new agent session.
pub const METHOD_AGENT_RUN: &str = "agent/run";

/// `agent/resume` — resume a prior agent session by id.
pub const METHOD_AGENT_RESUME: &str = "agent/resume";

/// `agent/cancel` — cancel an in-flight agent session.
pub const METHOD_AGENT_CANCEL: &str = "agent/cancel";

/// `agent/output` — server-streaming notification for incremental output.
pub const NOTIFICATION_AGENT_OUTPUT: &str = "agent/output";

/// `agent/thinking` — server-streaming notification for visible reasoning.
pub const NOTIFICATION_AGENT_THINKING: &str = "agent/thinking";

/// `agent/toolCall` — server-streaming notification when the agent invokes
/// a tool.
pub const NOTIFICATION_AGENT_TOOL_CALL: &str = "agent/toolCall";

/// `agent/toolResult` — server-streaming notification when a tool returns.
pub const NOTIFICATION_AGENT_TOOL_RESULT: &str = "agent/toolResult";

/// `agent/error` — server-streaming notification for recoverable or fatal
/// errors mid-run.
pub const NOTIFICATION_AGENT_ERROR: &str = "agent/error";

// =====================================================================
// Manifest
// =====================================================================

/// Static manifest describing what a provider plugin supports.
///
/// Returned by both the one-shot `--manifest` CLI mode and the `initialize`
/// JSON-RPC handshake. The fields here are the provider-specific overlay on
/// top of [`animus_plugin_protocol::PluginManifest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderManifest {
    /// Plugin name (e.g. `"animus-provider-claude"`).
    pub name: String,
    /// Plugin semver.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Concrete model identifiers this provider can route to.
    ///
    /// Examples: `["claude-sonnet-4-6", "claude-opus-4-7"]`,
    /// `["gpt-5", "gpt-5-mini"]`. Hosts use this to validate the `model`
    /// field of an [`AgentRunRequest`] before dispatching.
    pub supported_models: Vec<String>,
    /// Tool name passed through to the wrapped CLI (`"claude"`, `"codex"`,
    /// `"gemini"`, ...). Custom HTTP providers may set this to their plugin
    /// name.
    pub tool: String,
    /// Capability flags.
    pub capabilities: ProviderCapabilities,
}

/// Provider capability flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Provider emits `agent/output` deltas as the model produces them.
    #[serde(default)]
    pub streaming: bool,
    /// Provider supports `agent/resume` against prior session ids.
    #[serde(default)]
    pub resume: bool,
    /// Provider supports `agent/cancel`.
    #[serde(default)]
    pub cancellation: bool,
    /// Provider can edit files in the working directory (vs. read-only
    /// research providers).
    #[serde(default)]
    pub write_capable: bool,
    /// Provider supports MCP server bridging (i.e. accepts the
    /// `mcp_servers` field of [`AgentRunRequest`]).
    #[serde(default)]
    pub mcp: bool,
}

// =====================================================================
// Run requests / responses
// =====================================================================

/// Parameters for an `agent/run` (or `agent/resume`) call.
///
/// The same struct is reused for both methods; resume calls additionally
/// carry the prior `session_id` so the provider knows which transcript to
/// continue. The shape is intentionally tolerant of provider-specific
/// extensions via [`AgentRunRequest::extras`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunRequest {
    /// Existing session id when resuming. `None` for fresh runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// User prompt (latest turn).
    pub prompt: String,

    /// Concrete model identifier. Must appear in
    /// [`ProviderManifest::supported_models`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Optional system prompt. Many providers prefer this be set once at
    /// session start and ignored on subsequent turns; the provider decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Working directory the agent should operate from. The provider
    /// passes this through to the wrapped CLI (e.g. `cwd` for `claude`).
    pub cwd: PathBuf,

    /// Project root, if distinct from `cwd` (e.g. running from a subdir).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_root: Option<PathBuf>,

    /// Permission mode (`"safe"`, `"acceptEdits"`, `"bypassPermissions"`,
    /// ...). Provider-specific; consult the provider's manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,

    /// Hard timeout in seconds. The runtime will issue `agent/cancel` if
    /// this elapses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,

    /// Environment variables to inject into the spawned child.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,

    /// MCP server descriptors for the provider to bridge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Value>,

    /// Tool allow/deny config. Provider-specific shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Value>,

    /// Optional response schema (JSON Schema) the provider should constrain
    /// the model to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Value>,

    /// Runtime contract envelope (workflow-runner-supplied metadata).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_contract: Option<Value>,

    /// Provider-specific extras the daemon doesn't interpret.
    #[serde(default, flatten)]
    pub extras: HashMap<String, Value>,
}

/// Parameters for an `agent/resume` call.
///
/// Re-exports [`AgentRunRequest`] under the resume name so callers are
/// explicit about intent. The wire shape is identical; the runtime
/// distinguishes by RPC method name.
pub type AgentResumeRequest = AgentRunRequest;

/// Parameters for an `agent/cancel` call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCancelRequest {
    /// Session id to cancel.
    pub session_id: String,
}

/// Final response to `agent/run` or `agent/resume`.
///
/// Streaming notifications are sent during the run; this is the aggregated
/// terminal payload. Hosts may persist it as the canonical run record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunResponse {
    /// Provider-issued session id. Stable for the life of the session;
    /// usable with `agent/resume` later.
    pub session_id: String,

    /// Process exit code from the wrapped CLI, if any.
    pub exit_code: i32,

    /// Concatenated final assistant output.
    pub output: String,

    /// Free-form metadata entries the provider chose to surface.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<Value>,

    /// All tool invocations the agent made during the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<Value>,

    /// All tool results returned to the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<Value>,

    /// Visible reasoning traces (when the model produced any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thinking: Vec<String>,

    /// Errors emitted during the run (recoverable or terminal).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,

    /// Total wall-clock duration of the run.
    pub duration_ms: u64,

    /// Provider-specific backend label (e.g. `"claude-code:1.0.0"`).
    pub backend: String,

    /// Token-accounting summary, if the provider tracks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<TokenUsage>,

    /// Optional verdict for review/QA agents that produce pass/fail
    /// decisions. Free-form so review providers can shape their own
    /// envelopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_verdict: Option<Value>,
}

/// Token-accounting summary for an agent run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens consumed.
    pub input: u64,
    /// Output tokens generated.
    pub output: u64,
    /// Tokens served from a prompt cache, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
    /// Tokens written to a prompt cache, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_writes: Option<u64>,
}

// =====================================================================
// Errors
// =====================================================================

/// Errors a provider may return.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// Caller asked for a model the provider doesn't support.
    #[error("model not supported: {0}")]
    ModelNotSupported(String),

    /// Wrapped CLI failed to start.
    #[error("session start failed: {0}")]
    SessionStartFailed(String),

    /// Wrapped CLI exited with a non-zero status mid-run.
    #[error("agent run failed: {0}")]
    RunFailed(String),

    /// Provider was cancelled.
    #[error("cancelled")]
    Cancelled,

    /// Provider (or its upstream) is temporarily unavailable.
    #[error("provider unavailable: {0}")]
    Unavailable(String),

    /// Anything else.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<BackendError> for RpcError {
    fn from(error: BackendError) -> Self {
        match error {
            BackendError::ModelNotSupported(msg) => RpcError {
                code: error_codes::INVALID_PARAMS,
                message: format!("model not supported: {msg}"),
                data: Some(serde_json::json!({"category": "model_not_supported"})),
            },
            BackendError::SessionStartFailed(msg) => RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: format!("session start failed: {msg}"),
                data: Some(serde_json::json!({"category": "session_start_failed"})),
            },
            BackendError::RunFailed(msg) => RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: format!("agent run failed: {msg}"),
                data: Some(serde_json::json!({"category": "run_failed"})),
            },
            BackendError::Cancelled => RpcError {
                code: error_codes::REQUEST_CANCELLED,
                message: "cancelled".to_string(),
                data: Some(serde_json::json!({"category": "cancelled"})),
            },
            BackendError::Unavailable(msg) => RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: format!("provider unavailable: {msg}"),
                data: Some(serde_json::json!({"category": "unavailable"})),
            },
            BackendError::Other(error) => RpcError {
                code: error_codes::INTERNAL_ERROR,
                message: error.to_string(),
                data: Some(serde_json::json!({"category": "other"})),
            },
        }
    }
}

// =====================================================================
// The trait
// =====================================================================

/// What a provider plugin implements.
///
/// The Animus daemon uses this trait via the runtime in
/// [`animus-plugin-runtime`]. Trait implementers don't deal with JSON-RPC
/// directly; they receive deserialized request structs and return
/// deserialized response structs (or errors).
///
/// # Streaming
///
/// `run_agent` returns the *final* aggregated response. Incremental output
/// is delivered through a side channel the runtime supplies — this trait
/// surface intentionally hides that detail. See the runtime crate for the
/// concrete `EventEmitter` shape used in v0.4.0; in the meantime, providers
/// can use the wire constants (e.g. [`NOTIFICATION_AGENT_OUTPUT`]) to drive
/// their own streaming if they bypass the runtime.
#[async_trait]
pub trait ProviderBackend: Send + Sync + 'static {
    /// Static manifest. Should be cheap (preferably a constant).
    fn manifest(&self) -> ProviderManifest;

    /// Start a fresh agent session.
    async fn run_agent(&self, request: AgentRunRequest) -> Result<AgentRunResponse, BackendError>;

    /// Resume a prior session by id. Providers without resume support
    /// should advertise `capabilities.resume = false` in the manifest and
    /// return [`BackendError::Other`] with a clear message if called.
    async fn resume_agent(&self, request: AgentResumeRequest) -> Result<AgentRunResponse, BackendError>;

    /// Cancel an in-flight session.
    async fn cancel_agent(&self, session_id: &str) -> Result<(), BackendError>;

    /// Provider health.
    async fn health(&self) -> Result<HealthCheckResult, BackendError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips() {
        let manifest = ProviderManifest {
            name: "animus-provider-claude".into(),
            version: "0.1.0".into(),
            description: "Claude Code CLI provider".into(),
            supported_models: vec!["claude-sonnet-4-6".into()],
            tool: "claude".into(),
            capabilities: ProviderCapabilities {
                streaming: true,
                resume: true,
                cancellation: true,
                write_capable: true,
                mcp: true,
            },
        };
        let v = serde_json::to_value(&manifest).unwrap();
        let back: ProviderManifest = serde_json::from_value(v).unwrap();
        assert_eq!(back, manifest);
    }

    #[test]
    fn cancel_maps_to_request_cancelled() {
        let rpc: RpcError = BackendError::Cancelled.into();
        assert_eq!(rpc.code, error_codes::REQUEST_CANCELLED);
    }
}
