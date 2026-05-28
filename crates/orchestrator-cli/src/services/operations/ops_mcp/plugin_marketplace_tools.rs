//! MCP tools for the plugin marketplace surface (search, browse, update).
//!
//! Lives in a separate file from `plugin_tools.rs` so concurrent edits to the
//! core plugin tools (list/info/install/uninstall/ping/call) don't conflict
//! with the marketplace tool additions. The router is registered alongside
//! the core plugin router in `ops_mcp.rs::new_ao_mcp_server`.

use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::*;
use crate::services::operations::ops_plugin::{
    run_plugin_browse, run_plugin_search, run_plugin_update, PluginBrowseRequest, PluginSearchRequest,
    PluginUpdateRequest,
};

const DEFAULT_PLUGIN_REGISTRY_URL_MCP: &str =
    "https://raw.githubusercontent.com/launchapp-dev/animus-plugin-registry/main/plugins.json";

fn anyhow_to_mcp_marketplace(err: anyhow::Error) -> McpError {
    let chain: Vec<String> = err.chain().map(|cause| cause.to_string()).collect();
    if chain.iter().any(|c| c.contains("not found"))
        || chain.iter().any(|c| c.contains("must") || c.contains("invalid"))
    {
        McpError::invalid_params(chain.join(": "), None)
    } else {
        McpError::internal_error(chain.join(": "), None)
    }
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub(super) struct PluginSearchInput {
    /// Optional substring matched against name/description (case-insensitive).
    #[serde(default)]
    query: Option<String>,
    /// Filter by plugin kind (`provider`, `subject_backend`, `trigger`, ...).
    #[serde(default)]
    kind: Option<String>,
    /// Repeatable tag filter (all required).
    #[serde(default)]
    tag: Option<Vec<String>>,
    /// Filter by repo owner.
    #[serde(default)]
    org: Option<String>,
    /// Filter by stability (`alpha`, `beta`, `stable`).
    #[serde(default)]
    stability: Option<String>,
    /// Registry URL override.
    #[serde(default)]
    registry_url: Option<String>,
    /// Bypass the local cache.
    #[serde(default)]
    no_cache: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub(super) struct PluginBrowseInput {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    installed: Option<bool>,
    #[serde(default)]
    available: Option<bool>,
    #[serde(default)]
    registry_url: Option<String>,
    #[serde(default)]
    no_cache: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub(super) struct PluginUpdateInput {
    /// Optional plugin name. When omitted, all installed release-source plugins are considered.
    #[serde(default)]
    name: Option<String>,
    /// Pin to a specific tag instead of resolving the latest release.
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    force: Option<bool>,
    #[serde(default)]
    registry_url: Option<String>,
    #[serde(default)]
    no_cache: Option<bool>,
}

#[tool_router(router = plugin_marketplace_tool_router, vis = "pub(super)")]
impl AoMcpServer {
    #[tool(
        name = "animus.plugin.search",
        description = "Search the public Animus plugin registry (https://github.com/launchapp-dev/animus-plugin-registry) by substring + filters. Optional `query` matches against name/description (case-insensitive); `kind`, `tag` (repeatable), `org`, and `stability` filter results. Returns `{registry_url, total, matched, results}` where each result has `{name, kind, description, repo, latest_tag, stability, tags, install_command}`. Registry is cached at `~/.cache/animus/plugin-registry.json` for 6h; set `no_cache=true` to bypass.",
        input_schema = ao_schema_for_type::<PluginSearchInput>()
    )]
    async fn ao_plugin_search(&self, params: Parameters<PluginSearchInput>) -> Result<CallToolResult, McpError> {
        let PluginSearchInput { query, kind, tag, org, stability, registry_url, no_cache } = params.0;
        let output = run_plugin_search(PluginSearchRequest {
            query,
            kind,
            tag: tag.unwrap_or_default(),
            org,
            stability,
            registry_url: registry_url.unwrap_or_else(|| DEFAULT_PLUGIN_REGISTRY_URL_MCP.to_string()),
            no_cache: no_cache.unwrap_or(false),
        })
        .await
        .map_err(anyhow_to_mcp_marketplace)?;
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.search",
            "result": output,
        })))
    }

    #[tool(
        name = "animus.plugin.browse",
        description = "Browse the public Animus plugin registry grouped by kind. Optional filters: `kind` (single kind), `installed` (only currently-installed), `available` (only not-yet-installed). Returns `{registry_url, total, shown, groups}` where `groups` maps kind to `[{name, kind, description, repo, latest_tag, stability, installed, installed_tag}]`. Same caching as `animus.plugin.search`.",
        input_schema = ao_schema_for_type::<PluginBrowseInput>()
    )]
    async fn ao_plugin_browse(&self, params: Parameters<PluginBrowseInput>) -> Result<CallToolResult, McpError> {
        let PluginBrowseInput { kind, installed, available, registry_url, no_cache } = params.0;
        let output = run_plugin_browse(PluginBrowseRequest {
            kind,
            installed: installed.unwrap_or(false),
            available: available.unwrap_or(false),
            registry_url: registry_url.unwrap_or_else(|| DEFAULT_PLUGIN_REGISTRY_URL_MCP.to_string()),
            no_cache: no_cache.unwrap_or(false),
        })
        .await
        .map_err(anyhow_to_mcp_marketplace)?;
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.browse",
            "result": output,
        })))
    }

    #[tool(
        name = "animus.plugin.update",
        description = "Re-resolve the latest release tag for installed plugins whose `source_kind=release`. With no `name`, all installed release-source plugins are considered; with `name`, only that plugin. `tag` pins to a specific release. `dry_run=true` reports what would change without installing. `force=true` reinstalls even when the installed tag matches. Path/url source plugins are skipped (no remote to query). Returns `{dry_run, considered, updated, results}` where each result has `{name, installed_tag, target_tag, origin, status, detail, install?}`. After successful updates, the cached plugin registry is dropped.",
        input_schema = ao_schema_for_type::<PluginUpdateInput>()
    )]
    async fn ao_plugin_update(&self, params: Parameters<PluginUpdateInput>) -> Result<CallToolResult, McpError> {
        let PluginUpdateInput { name, tag, dry_run, force, registry_url, no_cache } = params.0;
        let output = run_plugin_update(PluginUpdateRequest {
            name,
            tag,
            dry_run: dry_run.unwrap_or(false),
            force: force.unwrap_or(false),
            registry_url: registry_url.unwrap_or_else(|| DEFAULT_PLUGIN_REGISTRY_URL_MCP.to_string()),
            no_cache: no_cache.unwrap_or(false),
        })
        .await
        .map_err(anyhow_to_mcp_marketplace)?;
        if output.updated > 0 {
            // Drop every cached per-project registry so subsequent calls
            // rediscover the freshly updated binaries.
            let mut guard = self.plugin_registry.lock().await;
            guard.clear();
        }
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.update",
            "result": output,
        })))
    }
}

#[cfg(test)]
mod marketplace_tool_tests {
    use super::super::new_ao_mcp_server;
    use tempfile::TempDir;

    #[tokio::test]
    async fn marketplace_router_registers_three_tools() {
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(project.path().to_str().unwrap());
        let names: Vec<String> = server.tool_router.list_all().into_iter().map(|t| t.name.to_string()).collect();
        for required in ["animus.plugin.search", "animus.plugin.browse", "animus.plugin.update"] {
            assert!(names.contains(&required.to_string()), "router missing {required}; have: {names:?}");
            assert!(server.tool_router.has_route(required));
        }
    }
}
