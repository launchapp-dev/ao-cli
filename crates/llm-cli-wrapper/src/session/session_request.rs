use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRequest {
    pub tool: String,
    pub model: String,
    pub prompt: String,
    pub cwd: PathBuf,
    pub project_root: Option<PathBuf>,
    pub mcp_endpoints: Vec<String>,
    pub permission_mode: Option<String>,
    pub timeout_secs: Option<u64>,
    pub env_vars: Vec<(String, String)>,
    pub extras: Value,
}
