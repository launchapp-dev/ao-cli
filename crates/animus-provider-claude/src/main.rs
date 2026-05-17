//! AO STDIO provider plugin for Claude Code.
//!
//! Wraps `cli_wrapper::session::ClaudeSessionBackend` via the shared
//! `animus_plugin_runtime` so every provider gets identical lifecycle,
//! streaming, and result-aggregation behavior.

use std::sync::Arc;

use animus_plugin_runtime::{run_provider, ProviderInfo, SessionBackendProvider};
use anyhow::Result;
use cli_wrapper::session::ClaudeSessionBackend;

const INFO: ProviderInfo = ProviderInfo {
    plugin_name: "animus-provider-claude",
    plugin_version: env!("CARGO_PKG_VERSION"),
    description: "Claude Code provider for AO (wraps llm-cli-wrapper claude backend)",
    default_tool: "claude",
    default_model: "claude-sonnet-4-6",
};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let backend = SessionBackendProvider::new(Arc::new(ClaudeSessionBackend::new()));
    run_provider(INFO, backend).await
}
