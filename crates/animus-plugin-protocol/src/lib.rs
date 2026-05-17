//! Wire types for the Animus stdio plugin protocol.
//!
//! Every Animus plugin — providers (LLM CLIs), subject backends (Linear, Jira,
//! GitHub Issues, ...), trigger backends (Slack, webhooks, ...), and any future
//! plugin kind — speaks the same newline-delimited JSON-RPC 2.0 protocol over
//! stdin/stdout. This crate defines the language-neutral wire shapes the host
//! and plugin agree on: the request/response envelope, error codes, the
//! `initialize`/`initialized`/`health/check` lifecycle, plugin kinds, and the
//! capability declarations exchanged during the handshake.
//!
//! Plugin compatibility is intentionally defined by these wire shapes rather
//! than by Rust crate linkage. A Python or TypeScript plugin that emits the
//! same JSON over stdio is just as compatible as a Rust plugin that links this
//! crate.
//!
//! # See also
//!
//! - The companion `spec.md` in this repository — the language-agnostic
//!   protocol specification.
//! - [`animus-subject-protocol`] for the subject-backend trait + schema layered
//!   on top of these wire types.
//! - [`animus-provider-protocol`] for the provider-backend trait layered on top
//!   of these wire types.
//! - [`animus-plugin-runtime`] for the shared stdio loop that consumes these
//!   types and dispatches into trait implementations.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current protocol version implemented by this crate.
///
/// Plugins declare the version they were built against in
/// [`InitializeResult::protocol_version`] during the handshake, and the host
/// declares its own in [`InitializeParams::protocol_version`]. A plugin and
/// host with the same major version are compatible. See `spec.md` for the
/// full versioning policy.
pub const PROTOCOL_VERSION: &str = "1.0.0";

/// Plugin kind for LLM provider plugins (Claude, Codex, Gemini, OpenAI-compat,
/// on-prem, ...).
///
/// Provider plugins implement `agent/run`, `agent/resume`, and `agent/cancel`.
pub const PLUGIN_KIND_PROVIDER: &str = "provider";

/// Plugin kind for subject backend plugins (Linear, Jira, GitHub Issues,
/// Notion, Asana, native task store, ...).
///
/// Subject backends implement the `subject/*` method family — `subject/list`,
/// `subject/get`, `subject/update`, optional `subject/watch`, and
/// `subject/schema`.
pub const PLUGIN_KIND_SUBJECT_BACKEND: &str = "subject_backend";

/// Plugin kind for trigger backend plugins (Slack, generic webhooks, file
/// watchers, ...).
///
/// Reserved for v0.4.x. The trigger protocol is not finalized in v0.4.0.
pub const PLUGIN_KIND_TRIGGER_BACKEND: &str = "trigger_backend";

/// Plugin kind for plugins that don't fit a built-in category.
///
/// Custom plugins still go through the standard
/// `initialize`/`initialized`/`health/check` lifecycle but the host treats
/// their domain methods opaquely. Custom plugins are typically invoked via
/// the `animus.plugin.call` MCP tool.
pub const PLUGIN_KIND_CUSTOM: &str = "custom";

/// JSON-RPC 2.0 standard error codes plus Animus-specific extensions.
///
/// The `-32700`..`-32600` range follows the JSON-RPC 2.0 specification. The
/// `-32000`..`-32099` range is reserved by JSON-RPC 2.0 for implementation
/// errors; Animus uses it for protocol-level conditions that the host needs
/// to react to programmatically (e.g. graceful fallback when a plugin doesn't
/// support an optional method).
pub mod error_codes {
    /// Invalid JSON was received by the server.
    pub const PARSE_ERROR: i32 = -32700;
    /// The JSON sent is not a valid request object.
    pub const INVALID_REQUEST: i32 = -32600;
    /// The method does not exist or is not available.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid method parameter(s).
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal JSON-RPC error.
    pub const INTERNAL_ERROR: i32 = -32603;

    /// Animus extension: the plugin received a domain method before
    /// `initialize` completed.
    pub const PLUGIN_NOT_INITIALIZED: i32 = -32000;
    /// Animus extension: the plugin recognizes the method but does not
    /// implement it (e.g. a polling-only subject backend rejecting
    /// `subject/watch`). The host should fall back rather than fail.
    pub const METHOD_NOT_SUPPORTED: i32 = -32001;
    /// Animus extension: the host cancelled an in-flight request.
    pub const REQUEST_CANCELLED: i32 = -32002;
    /// Animus extension: a request timed out before completing.
    pub const TIMEOUT: i32 = -32003;
}

/// A JSON-RPC 2.0 request frame.
///
/// `id` is `Some` for requests that expect a response. Notifications use
/// [`RpcNotification`] instead and have no `id`. `params` is structurally
/// typed via [`Value`] so the runtime can dispatch to method-specific
/// deserializers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcRequest {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Request id. `None` indicates a notification (use [`RpcNotification`]
    /// instead in that case; this field exists to round-trip permissively).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    /// JSON-RPC method name (e.g. `"initialize"`, `"subject/list"`).
    pub method: String,
    /// Method parameters; structurally validated by the receiving handler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl RpcRequest {
    /// Build a request with the given id, method, and optional params.
    pub fn new(id: impl Into<Value>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self { jsonrpc: "2.0".to_string(), id: Some(id.into()), method: method.into(), params }
    }
}

/// A JSON-RPC 2.0 notification frame.
///
/// Notifications are fire-and-forget — they have no `id` and the recipient
/// never replies. Server-streaming results from a single request id (e.g.
/// `subject/changed` watch events) are also delivered as notifications; in
/// that case the original request id is carried inside `params`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcNotification {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// JSON-RPC method name.
    pub method: String,
    /// Notification parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl RpcNotification {
    /// Build a notification with the given method and optional params.
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self { jsonrpc: "2.0".to_string(), method: method.into(), params }
    }
}

/// A JSON-RPC 2.0 response frame.
///
/// Exactly one of `result` or `error` should be set. Use [`RpcResponse::ok`]
/// or [`RpcResponse::err`] to construct correctly-shaped responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcResponse {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// Echoes the id of the originating request. `None` only when the request
    /// id could not be determined (e.g. parse error on the request frame).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    /// Successful result. Mutually exclusive with `error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Error payload. Mutually exclusive with `result`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    /// Build a successful response carrying the given result value.
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }

    /// Build an error response carrying the given error payload.
    pub fn err(id: Option<Value>, error: RpcError) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: None, error: Some(error) }
    }
}

/// JSON-RPC 2.0 error payload.
///
/// `code` is one of the constants in [`error_codes`] or an
/// implementation-specific value in the reserved JSON-RPC range. `data` is
/// optional structured detail that the host can surface in logs or pass back
/// to the originating CLI/MCP caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// Error code; see [`error_codes`].
    pub code: i32,
    /// Short human-readable description.
    pub message: String,
    /// Optional structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Identity of the host issuing the `initialize` call.
///
/// Plugins may log this for debugging or vary behavior based on the host
/// version (e.g. enabling features only available in newer hosts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    /// Conventionally `"animus"` for the official Animus daemon.
    pub name: String,
    /// Semver of the host.
    pub version: String,
}

/// Identity of the plugin returned in the `initialize` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin's published name (e.g. `"animus-subject-linear"`).
    pub name: String,
    /// Plugin's semver.
    pub version: String,
    /// One of the `PLUGIN_KIND_*` constants.
    pub plugin_kind: String,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Capabilities the host advertises during the handshake.
///
/// Plugins may use these to enable optional features. The host promises to
/// honor any capability it advertises.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCapabilities {
    /// Host accepts server-streaming notifications carrying the original
    /// request id.
    #[serde(default)]
    pub streaming: bool,
    /// Host accepts `$/progress` notifications.
    #[serde(default)]
    pub progress: bool,
    /// Host may issue `$/cancelRequest` notifications to cancel in-flight
    /// requests.
    #[serde(default)]
    pub cancellation: bool,
}

/// Capabilities the plugin advertises during the handshake.
///
/// `methods` is the closed set of domain methods the plugin implements; the
/// host uses it to skip calls the plugin would reject anyway. `subject_kinds`
/// and `mcp_tools` are supplemental hints for subject-backend and
/// custom-plugin kinds respectively.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// Concrete methods the plugin implements (e.g. `["subject/list",
    /// "subject/get", "subject/update"]`).
    #[serde(default)]
    pub methods: Vec<String>,
    /// Plugin emits server-streaming notifications.
    #[serde(default)]
    pub streaming: bool,
    /// Plugin honors `$/progress` notifications.
    #[serde(default)]
    pub progress: bool,
    /// Plugin honors `$/cancelRequest` notifications.
    #[serde(default)]
    pub cancellation: bool,
    /// Subject kinds the plugin can produce (subject backends only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_kinds: Vec<String>,
    /// MCP tools exposed by the plugin (custom plugins only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<McpTool>,
}

/// Description of an MCP tool exposed by a custom plugin.
///
/// Hosts that bridge MCP can re-expose these tools to MCP clients without
/// the plugin author writing MCP-specific code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTool {
    /// MCP tool name.
    pub name: String,
    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the tool's input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
}

/// Parameters sent from host to plugin in the `initialize` request.
///
/// This is the first request the host sends after the plugin process starts.
/// The plugin should validate `protocol_version` and return an
/// [`InitializeResult`] or an error if the versions are incompatible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeParams {
    /// Protocol version the host speaks. See [`PROTOCOL_VERSION`].
    pub protocol_version: String,
    /// Identity of the host.
    pub host_info: HostInfo,
    /// Capabilities the host promises to honor.
    pub capabilities: HostCapabilities,
}

/// Plugin's response to `initialize`.
///
/// The host inspects `protocol_version` for compatibility and stores
/// `capabilities` for the lifetime of the plugin connection so it can avoid
/// calling unsupported methods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Protocol version the plugin speaks. See [`PROTOCOL_VERSION`].
    pub protocol_version: String,
    /// Identity of the plugin.
    pub plugin_info: PluginInfo,
    /// Capabilities the plugin advertises.
    pub capabilities: PluginCapabilities,
}

/// One-shot manifest emitted when a plugin is invoked with `--manifest`.
///
/// This is the discovery surface used by `animus plugin install` and similar
/// tooling that needs to know what a binary is before spawning it as a
/// long-running stdio child. The shape mirrors [`InitializeResult`] but is
/// flat for ease of static parsing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (matches [`PluginInfo::name`]).
    pub name: String,
    /// Plugin semver.
    pub version: String,
    /// One of the `PLUGIN_KIND_*` constants.
    pub plugin_kind: String,
    /// Human-readable description.
    pub description: String,
    /// Protocol version the plugin was built against.
    pub protocol_version: String,
    /// Methods implemented by the plugin.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Health status emitted by `health/check`.
///
/// Hosts surface this in `animus daemon health` and may use it to gate work
/// (e.g. drain in-flight subjects from a `Degraded` plugin before restart).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Plugin is fully functional.
    Healthy,
    /// Plugin is operational but in a reduced state (e.g. stale cache,
    /// upstream rate-limited but recovering).
    Degraded,
    /// Plugin is non-functional. The host may restart or quarantine it.
    Unhealthy,
}

/// Response to `health/check`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Overall status.
    pub status: HealthStatus,
    /// Milliseconds since the plugin process started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
    /// Resident-set memory usage in bytes, if cheap to determine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_usage_bytes: Option<u64>,
    /// Most recent error message, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_json_rpc_2() {
        let request = RpcRequest::new(1, "initialize", None);
        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(serde_json::json!(1)));
        assert_eq!(request.method, "initialize");
    }

    #[test]
    fn response_ok_sets_result_and_clears_error() {
        let response = RpcResponse::ok(Some(serde_json::json!(1)), serde_json::json!({"ok": true}));
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn response_err_sets_error_and_clears_result() {
        let response = RpcResponse::err(
            Some(serde_json::json!(1)),
            RpcError { code: error_codes::METHOD_NOT_FOUND, message: "nope".into(), data: None },
        );
        assert!(response.error.is_some());
        assert!(response.result.is_none());
    }

    #[test]
    fn manifest_round_trips_unknown_plugin_kind() {
        let value = serde_json::json!({
            "name": "linear",
            "version": "0.1.0",
            "plugin_kind": "ticket_backend",
            "description": "external tickets",
            "protocol_version": "1.0.0",
            "capabilities": ["ticket/get"]
        });
        let manifest: PluginManifest = serde_json::from_value(value).expect("manifest should parse");
        assert_eq!(manifest.plugin_kind, "ticket_backend");
    }

    #[test]
    fn health_status_serializes_snake_case() {
        let v = serde_json::to_value(HealthStatus::Degraded).unwrap();
        assert_eq!(v, serde_json::json!("degraded"));
    }
}
