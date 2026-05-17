//! AO STDIO provider plugin for OpenAI Codex CLI.

use std::sync::Arc;

use animus_plugin_runtime::{run_provider, ProviderInfo, SessionBackendProvider};
use anyhow::Result;
use cli_wrapper::session::CodexSessionBackend;

const INFO: ProviderInfo = ProviderInfo {
    plugin_name: "animus-provider-codex",
    plugin_version: env!("CARGO_PKG_VERSION"),
    description: "OpenAI Codex provider for AO (wraps llm-cli-wrapper codex backend)",
    default_tool: "codex",
    default_model: "gpt-5",
};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let backend = SessionBackendProvider::new(Arc::new(CodexSessionBackend::new()));
    run_provider(INFO, backend).await
}
