//! AO STDIO provider plugin for OpenAI-compatible APIs (OpenRouter, Together, MiniMax, ZAI, etc.).

use std::sync::Arc;

use animus_plugin_runtime::{run_provider, ProviderInfo, SessionBackendProvider};
use animus_session_backend::session::OaiRunnerSessionBackend;
use anyhow::Result;

const INFO: ProviderInfo = ProviderInfo {
    plugin_name: "animus-provider-oai",
    plugin_version: env!("CARGO_PKG_VERSION"),
    description: "OpenAI-compatible API provider for AO (wraps llm-cli-wrapper oai-runner backend)",
    default_tool: "oai-runner",
    default_model: "openrouter/auto",
};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let backend = SessionBackendProvider::new(Arc::new(OaiRunnerSessionBackend::new()));
    run_provider(INFO, backend).await
}
