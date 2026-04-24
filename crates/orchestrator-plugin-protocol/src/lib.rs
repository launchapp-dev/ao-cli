//! Wire types for AO-compatible stdio plugins.
//!
//! The protocol is newline-delimited JSON-RPC 2.0. Plugin compatibility is
//! intentionally defined by these wire shapes rather than by Rust crate linkage.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "1.0.0";
pub const PLUGIN_KIND_PROVIDER: &str = "provider";
pub const PLUGIN_KIND_TASK_BACKEND: &str = "task_backend";
pub const PLUGIN_KIND_SUBJECT_BACKEND: &str = "subject_backend";
pub const PLUGIN_KIND_CUSTOM: &str = "custom";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl RpcRequest {
    pub fn new(id: impl Into<Value>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self { jsonrpc: "2.0".to_string(), id: Some(id.into()), method: method.into(), params }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl RpcNotification {
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self { jsonrpc: "2.0".to_string(), method: method.into(), params }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }

    pub fn err(id: Option<Value>, error: RpcError) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: None, error: Some(error) }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    pub const REQUEST_CANCELLED: i32 = -32800;
    pub const PLUGIN_NOT_READY: i32 = -1000;
    pub const CAPABILITY_NOT_SUPPORTED: i32 = -1001;
    pub const TIMEOUT: i32 = -1003;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCapabilities {
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub progress: bool,
    #[serde(default)]
    pub cancellation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub host_info: HostInfo,
    pub capabilities: HostCapabilities,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub plugin_kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projections: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subject_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_tools: Vec<McpTool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub plugin_info: PluginInfo,
    pub capabilities: PluginCapabilities,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub plugin_kind: String,
    pub description: String,
    pub protocol_version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_usage_bytes: Option<u64>,
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
    fn capabilities_accept_unknown_plugin_kind_as_string() {
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
}
