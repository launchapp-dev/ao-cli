//! AO STDIO provider plugin for Google Gemini CLI.

use std::sync::Arc;

use animus_plugin_runtime::{run_provider, ProviderInfo, SessionBackendProvider};
use animus_session_backend::session::GeminiSessionBackend;
use anyhow::Result;

const INFO: ProviderInfo = ProviderInfo {
    plugin_name: "animus-provider-gemini",
    plugin_version: env!("CARGO_PKG_VERSION"),
    description: "Google Gemini CLI provider for AO (wraps llm-cli-wrapper gemini backend)",
    default_tool: "gemini",
    default_model: "gemini-3.1-pro-preview",
};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let backend = SessionBackendProvider::new(Arc::new(GeminiSessionBackend::new()));
    run_provider(INFO, backend).await
}
