//! AO STDIO provider plugin for OpenCode.

use std::sync::Arc;

use animus_plugin_runtime::{run_provider, ProviderInfo, SessionBackendProvider};
use anyhow::Result;
use cli_wrapper::session::OpenCodeSessionBackend;

const INFO: ProviderInfo = ProviderInfo {
    plugin_name: "animus-provider-opencode",
    plugin_version: env!("CARGO_PKG_VERSION"),
    description: "OpenCode provider for AO (wraps llm-cli-wrapper opencode backend)",
    default_tool: "opencode",
    default_model: "glm-5",
};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let backend = SessionBackendProvider::new(Arc::new(OpenCodeSessionBackend::new()));
    run_provider(INFO, backend).await
}
