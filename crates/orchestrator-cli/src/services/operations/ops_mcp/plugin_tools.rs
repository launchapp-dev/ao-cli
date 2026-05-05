use std::path::Path;

use orchestrator_plugin_host::{discover_plugins, PluginRegistry};
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::*;

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct PluginListInput {
    #[serde(default)]
    project_root: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct PluginCallInput {
    #[serde(default)]
    project_root: Option<String>,
    /// Plugin name as discovered (matches manifest.name or filename).
    name: String,
    /// JSON-RPC method to invoke on the plugin.
    method: String,
    /// Optional JSON params object passed verbatim to the plugin.
    #[serde(default)]
    params: Option<Value>,
}

impl AoMcpServer {
    fn project_root_or_default(&self, override_root: Option<String>) -> String {
        normalize_non_empty(override_root).unwrap_or_else(|| self.default_project_root.clone())
    }

    async fn ensure_plugin_registry(&self, project_root: &str) -> Result<(), McpError> {
        let mut guard = self.plugin_registry.lock().await;
        if guard.is_none() {
            let registry = PluginRegistry::discover(Path::new(project_root))
                .map_err(|err| McpError::internal_error(format!("plugin discovery failed: {err}"), None))?;
            *guard = Some(registry);
        }
        Ok(())
    }
}

#[tool_router(router = plugin_tool_router, vis = "pub(super)")]
impl AoMcpServer {
    #[tool(
        name = "ao.plugin.list",
        description = "List discovered AO STDIO plugins (providers, subject backends, custom). Returns name, version, kind, source, and path for each plugin found via plugins.yaml, .ao/plugins/, or AO_PLUGIN_PATH.",
        input_schema = ao_schema_for_type::<PluginListInput>()
    )]
    async fn ao_plugin_list(
        &self,
        params: Parameters<PluginListInput>,
    ) -> Result<CallToolResult, McpError> {
        let project_root = self.project_root_or_default(params.0.project_root);
        let plugins = discover_plugins(Path::new(&project_root)).map_err(|err| {
            McpError::internal_error(format!("plugin discovery failed: {err}"), None)
        })?;
        let rows: Vec<Value> = plugins
            .into_iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "version": p.manifest.version,
                    "plugin_kind": p.manifest.plugin_kind,
                    "description": p.manifest.description,
                    "source": match p.source {
                        orchestrator_plugin_host::DiscoverySource::ExplicitConfig => "explicit_config",
                        orchestrator_plugin_host::DiscoverySource::ProjectLocal => "project_local",
                        orchestrator_plugin_host::DiscoverySource::PluginPath => "plugin_path",
                        orchestrator_plugin_host::DiscoverySource::SystemPath => "system_path",
                    },
                    "path": p.path.display().to_string(),
                })
            })
            .collect();
        Ok(CallToolResult::structured(json!({
            "tool": "ao.plugin.list",
            "result": rows,
        })))
    }

    #[tool(
        name = "ao.plugin.call",
        description = "Send a JSON-RPC request to a discovered plugin and return its response. The plugin is spawned (or reused), handshaked, then invoked with the supplied method and optional params.",
        input_schema = ao_schema_for_type::<PluginCallInput>()
    )]
    async fn ao_plugin_call(
        &self,
        params: Parameters<PluginCallInput>,
    ) -> Result<CallToolResult, McpError> {
        let PluginCallInput { project_root, name, method, params: rpc_params } = params.0;
        let project_root = self.project_root_or_default(project_root);
        let trimmed_name = name.trim().to_string();
        let trimmed_method = method.trim().to_string();
        if trimmed_name.is_empty() {
            return Err(McpError::invalid_params("name must not be empty", None));
        }
        if trimmed_method.is_empty() {
            return Err(McpError::invalid_params("method must not be empty", None));
        }

        self.ensure_plugin_registry(&project_root).await?;
        let mut guard = self.plugin_registry.lock().await;
        let registry = guard.as_mut().expect("plugin registry should be initialized");
        let host = registry
            .get_plugin(&trimmed_name)
            .await
            .map_err(|err| McpError::internal_error(format!("failed to load plugin '{trimmed_name}': {err}"), None))?;
        let result = host
            .request(trimmed_method.clone(), rpc_params)
            .await
            .map_err(|err| McpError::internal_error(format!("plugin call failed ({}): {}", err.code, err.message), None))?;

        Ok(CallToolResult::structured(json!({
            "tool": "ao.plugin.call",
            "result": {
                "name": trimmed_name,
                "method": trimmed_method,
                "response": result,
            }
        })))
    }
}
