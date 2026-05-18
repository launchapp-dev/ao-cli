use std::path::Path;

use orchestrator_plugin_host::{PluginDiscovery, PluginRegistry};
use rmcp::model::CallToolResult;
use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::*;
use crate::services::operations::ops_plugin::{
    run_plugin_info, run_plugin_install, run_plugin_list, run_plugin_ping, run_plugin_uninstall, PluginInfoRequest,
    PluginInstallRequest, PluginListRequest, PluginPingRequest, PluginUninstallRequest,
};

fn anyhow_to_mcp(err: anyhow::Error) -> McpError {
    let chain: Vec<String> = err.chain().map(|cause| cause.to_string()).collect();
    if chain.iter().any(|c| c.contains("not found")) {
        McpError::invalid_params(chain.join(": "), None)
    } else if chain.iter().any(|c| c.contains("invalid_input") || c.contains("must")) {
        McpError::invalid_params(chain.join(": "), None)
    } else {
        McpError::internal_error(chain.join(": "), None)
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct PluginListInput {
    #[serde(default)]
    project_root: Option<String>,
    /// Also scan $PATH for animus-provider-* and animus-plugin-* binaries.
    #[serde(default)]
    include_system_path: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct PluginInfoInput {
    #[serde(default)]
    project_root: Option<String>,
    /// Plugin name as discovered (matches manifest.name or filename).
    name: String,
    /// Also scan $PATH while resolving the plugin.
    #[serde(default)]
    include_system_path: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct PluginPingInput {
    #[serde(default)]
    project_root: Option<String>,
    /// Plugin name to spawn and ping.
    name: String,
    /// Also scan $PATH while resolving the plugin.
    #[serde(default)]
    include_system_path: Option<bool>,
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

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub(super) struct PluginInstallInput {
    /// Public GitHub repo slug to install from (e.g.
    /// `launchapp-dev/animus-provider-claude`, or with a tag
    /// `launchapp-dev/animus-provider-claude@v0.1.0`). Mutually exclusive
    /// with `path` and `url`. When the public-repo install pipeline is not
    /// yet wired in, this call returns a clear error directing callers at
    /// `path` or `url`.
    #[serde(default)]
    source: Option<String>,
    /// Local filesystem path to the plugin binary to install.
    #[serde(default)]
    path: Option<String>,
    /// HTTPS URL to download the plugin binary from.
    #[serde(default)]
    url: Option<String>,
    /// Optional release tag override for `source` installs.
    #[serde(default)]
    tag: Option<String>,
    /// Optional logical plugin name. Defaults to the binary file name.
    #[serde(default)]
    name: Option<String>,
    /// Expected SHA256 hex digest. Required when `url` is set; optional for
    /// `path` and `source`. The install fails if the downloaded/copied
    /// binary's checksum does not match.
    #[serde(default)]
    sha256: Option<String>,
    /// Overwrite an existing installed plugin with the same name.
    #[serde(default)]
    force: Option<bool>,
    /// Skip running `--manifest` against the installed binary.
    #[serde(default)]
    skip_manifest_check: Option<bool>,
    /// Override the plugin install directory. Takes precedence over
    /// `$ANIMUS_PLUGIN_DIR`. Defaults to `~/.animus/plugins/`.
    #[serde(default)]
    plugin_dir: Option<String>,
    /// Refuse install when no cosign signature bundle is present or when
    /// verification fails. Defaults to false (verify-if-present).
    #[serde(default)]
    require_signature: Option<bool>,
    /// Skip cosign signature verification entirely.
    #[serde(default)]
    skip_signature: Option<bool>,
    /// Optional path to a trusted-signers YAML allowlist (overrides default
    /// `~/.animus/trusted-signers.yaml`).
    #[serde(default)]
    trusted_signers: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(super) struct PluginUninstallInput {
    /// Logical plugin name to uninstall.
    name: String,
    /// Override the plugin install directory. Takes precedence over
    /// `$ANIMUS_PLUGIN_DIR`. Defaults to `~/.animus/plugins/`.
    #[serde(default)]
    plugin_dir: Option<String>,
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
        name = "animus.plugin.list",
        description = "List discovered Animus STDIO plugins (providers, subject backends, custom). Returns name, version, plugin_kind, description, protocol_version, capabilities, source, and path for each plugin found via plugins.yaml, .animus/plugins/, $ANIMUS_PLUGIN_DIR, or $ANIMUS_PLUGIN_PATH. The structured payload also includes `warnings` for plugins that were discovered but failed to load (manifest probe failed, malformed yaml, etc.).",
        input_schema = ao_schema_for_type::<PluginListInput>()
    )]
    async fn ao_plugin_list(&self, params: Parameters<PluginListInput>) -> Result<CallToolResult, McpError> {
        let PluginListInput { project_root, include_system_path } = params.0;
        let project_root = self.project_root_or_default(project_root);
        let output = run_plugin_list(PluginListRequest {
            project_root,
            include_system_path: include_system_path.unwrap_or(false),
        })
        .map_err(anyhow_to_mcp)?;
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.list",
            "result": output,
        })))
    }

    #[tool(
        name = "animus.plugin.info",
        description = "Spawn the named plugin, complete the initialize handshake, then shut it down. Returns the plugin's full manifest (name, version, plugin_kind, description, capabilities, protocol_version, license, repository, homepage) plus the initialize response (plugin_info + capabilities advertised at runtime). Use this to inspect a plugin's surface before calling it.",
        input_schema = ao_schema_for_type::<PluginInfoInput>()
    )]
    async fn ao_plugin_info(&self, params: Parameters<PluginInfoInput>) -> Result<CallToolResult, McpError> {
        let PluginInfoInput { project_root, name, include_system_path } = params.0;
        let project_root = self.project_root_or_default(project_root);
        let output = run_plugin_info(PluginInfoRequest {
            project_root,
            name,
            include_system_path: include_system_path.unwrap_or(false),
        })
        .await
        .map_err(anyhow_to_mcp)?;
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.info",
            "result": output,
        })))
    }

    #[tool(
        name = "animus.plugin.ping",
        description = "Health-check a plugin by spawning it, completing the initialize handshake, dispatching `$/ping`, then shutting it down. Returns `{ok: true, name, plugin_info}` on success. Use this to verify that an installed plugin can actually start and respond — `animus.plugin.list` only checks discovery, not liveness.",
        input_schema = ao_schema_for_type::<PluginPingInput>()
    )]
    async fn ao_plugin_ping(&self, params: Parameters<PluginPingInput>) -> Result<CallToolResult, McpError> {
        let PluginPingInput { project_root, name, include_system_path } = params.0;
        let project_root = self.project_root_or_default(project_root);
        let output = run_plugin_ping(PluginPingRequest {
            project_root,
            name,
            include_system_path: include_system_path.unwrap_or(false),
        })
        .await
        .map_err(anyhow_to_mcp)?;
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.ping",
            "result": output,
        })))
    }

    #[tool(
        name = "animus.plugin.call",
        description = "Send a JSON-RPC request to a discovered plugin and return its response. The plugin is spawned (or reused via the cached PluginRegistry), handshaked, then invoked with the supplied method and optional params. Use this for direct one-off calls; for sustained dispatch use `animus.agent.run` (provider plugins) or the subject-routed tools (subject backends).",
        input_schema = ao_schema_for_type::<PluginCallInput>()
    )]
    async fn ao_plugin_call(&self, params: Parameters<PluginCallInput>) -> Result<CallToolResult, McpError> {
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
        let registry =
            guard.as_mut().ok_or_else(|| McpError::internal_error("plugin registry not initialized", None))?;
        let host = registry
            .get_plugin(&trimmed_name)
            .await
            .map_err(|err| McpError::internal_error(format!("failed to load plugin '{trimmed_name}': {err}"), None))?;
        let result = host.request(trimmed_method.clone(), rpc_params).await.map_err(|err| {
            McpError::internal_error(format!("plugin call failed ({}): {}", err.code, err.message), None)
        })?;

        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.call",
            "result": {
                "name": trimmed_name,
                "method": trimmed_method,
                "response": result,
            }
        })))
    }

    #[tool(
        name = "animus.plugin.install",
        description = "Install an Animus plugin binary. Exactly one of `source` (public GitHub repo slug like `owner/repo[@tag]`), `path` (local filesystem path), or `url` (https download) must be provided. When `url` is set, `sha256` is required for integrity verification. The plugin is copied into the install directory (`$ANIMUS_PLUGIN_DIR` or `~/.animus/plugins/`, overridable via `plugin_dir`), made executable, probed via `--manifest`, and registered in `~/.animus/plugins.yaml`. Returns name, installed_path, sha256, manifest, plugins_yaml, and provenance fields (source_kind, origin, release_tag, asset_name, sha256_verified).",
        input_schema = ao_schema_for_type::<PluginInstallInput>()
    )]
    async fn ao_plugin_install(&self, params: Parameters<PluginInstallInput>) -> Result<CallToolResult, McpError> {
        let PluginInstallInput {
            source,
            path,
            url,
            tag,
            name,
            sha256,
            force,
            skip_manifest_check,
            plugin_dir,
            require_signature,
            skip_signature,
            trusted_signers,
        } = params.0;
        let output = run_plugin_install(PluginInstallRequest {
            source,
            path,
            url,
            tag,
            name,
            sha256,
            force: force.unwrap_or(false),
            skip_manifest_check: skip_manifest_check.unwrap_or(false),
            plugin_dir,
            require_signature: require_signature.unwrap_or(false),
            skip_signature: skip_signature.unwrap_or(false),
            trusted_signers: trusted_signers.map(std::path::PathBuf::from),
        })
        .await
        .map_err(anyhow_to_mcp)?;
        // Drop the cached plugin registry so subsequent calls re-discover the
        // freshly installed binary.
        {
            let mut guard = self.plugin_registry.lock().await;
            *guard = None;
        }
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.install",
            "result": output,
        })))
    }

    #[tool(
        name = "animus.plugin.uninstall",
        description = "Remove an installed plugin. Deletes the binary from the install directory (`$ANIMUS_PLUGIN_DIR` or `~/.animus/plugins/`, overridable via `plugin_dir`) and drops the entry from `~/.animus/plugins.yaml`. Returns `{name, removed_path, plugins_yaml}`. Fails with `not_found` when the plugin is not installed.",
        input_schema = ao_schema_for_type::<PluginUninstallInput>()
    )]
    async fn ao_plugin_uninstall(&self, params: Parameters<PluginUninstallInput>) -> Result<CallToolResult, McpError> {
        let PluginUninstallInput { name, plugin_dir } = params.0;
        let output = run_plugin_uninstall(PluginUninstallRequest { name, plugin_dir }).map_err(anyhow_to_mcp)?;
        // Drop the cached plugin registry so subsequent calls re-discover the
        // current set of installed plugins.
        {
            let mut guard = self.plugin_registry.lock().await;
            *guard = None;
        }
        Ok(CallToolResult::structured(json!({
            "tool": "animus.plugin.uninstall",
            "result": output,
        })))
    }
}

// Discovery helpers re-export for cfg(test) so tests can drive discovery
// without spinning up the full MCP server.
#[allow(dead_code)]
pub(super) fn discover_for_tests(
    project_root: &str,
) -> Result<Vec<orchestrator_plugin_host::DiscoveredPlugin>, anyhow::Error> {
    PluginDiscovery::new().with_project_root(Path::new(project_root)).discover().map_err(Into::into)
}

#[cfg(test)]
mod plugin_tool_tests {
    use super::super::new_ao_mcp_server;
    use super::*;
    use protocol::test_utils::EnvVarGuard;
    use rmcp::handler::server::wrapper::Parameters;
    use serde_json::Value;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn structured(result: &rmcp::model::CallToolResult) -> Value {
        result.structured_content.clone().expect("expected structured_content on tool result")
    }

    fn data(result: &rmcp::model::CallToolResult) -> Value {
        let payload = structured(result);
        payload.get("result").cloned().expect("structured result should include `result`")
    }

    /// Per-test mutex so concurrent plugin-tool tests don't race on the
    /// process-global ANIMUS_PLUGIN_DIR / ANIMUS_CONFIG_DIR env vars.
    static PLUGIN_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Isolate the plugin install dir and plugins.yaml location via
    /// `ANIMUS_PLUGIN_DIR` + `ANIMUS_CONFIG_DIR` (which both
    /// `plugin_install_dir` and `plugins_registry_path` honor) so tests
    /// don't touch the real `~/.animus/plugins/`. Holds [`PLUGIN_TEST_LOCK`]
    /// for the lifetime of the returned guard so the env vars are stable
    /// across the test body. HOME is intentionally NOT touched here — other
    /// tests in this binary read HOME for unrelated paths and a concurrent
    /// mutation would race them.
    fn isolated_plugin_dirs() -> (TempDir, std::sync::MutexGuard<'static, ()>, Vec<EnvVarGuard>) {
        let lock = PLUGIN_TEST_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        let home = TempDir::new().expect("plugin dirs tempdir");
        let plugins_dir = home.path().join("plugins");
        std::fs::create_dir_all(&plugins_dir).expect("create plugins subdir");
        // Write an empty plugins.yaml at the canonical location so the legacy
        // fallback (~/.config/animus/plugins.yaml) does NOT win. Without this,
        // the contributor's real ~/.config/animus/plugins.yaml would leak
        // entries into discovery for the test.
        let registry_path = home.path().join("plugins.yaml");
        std::fs::write(&registry_path, "plugins: {}\nproviders: {}\n").expect("write empty plugins.yaml");
        let mut guards = Vec::new();
        guards.push(EnvVarGuard::set("ANIMUS_PLUGIN_DIR", Some(plugins_dir.to_str().expect("plugin dir utf-8"))));
        guards.push(EnvVarGuard::set("ANIMUS_CONFIG_DIR", Some(home.path().to_str().expect("config dir utf-8"))));
        guards.push(EnvVarGuard::set("ANIMUS_PLUGIN_PATH", None));
        (home, lock, guards)
    }

    fn project_root_for(tmp: &TempDir) -> String {
        tmp.path().to_string_lossy().to_string()
    }

    /// Write a fake plugin binary that responds to `--manifest` with a minimal
    /// PluginManifest JSON document. The script is a portable POSIX shell.
    fn write_fake_plugin_binary(path: &std::path::Path, name: &str, kind: &str) {
        let body = format!(
            r#"#!/usr/bin/env sh
if [ "$1" = "--manifest" ]; then
  cat <<'JSON'
{{"name":"{name}","version":"0.0.1-test","plugin_kind":"{kind}","description":"fixture plugin","protocol_version":"1","capabilities":[],"license":null,"repository":null,"homepage":null}}
JSON
  exit 0
fi
exit 0
"#,
            name = name,
            kind = kind,
        );
        fs::write(path, body).expect("write fake plugin script");
        let mut perms = fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod fake plugin");
    }

    #[tokio::test]
    async fn plugin_router_registers_all_six_tools() {
        let (_home, _lock, _guards) = isolated_plugin_dirs();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));
        let names: Vec<String> = server.tool_router.list_all().into_iter().map(|tool| tool.name.to_string()).collect();
        for required in [
            "animus.plugin.list",
            "animus.plugin.info",
            "animus.plugin.install",
            "animus.plugin.uninstall",
            "animus.plugin.ping",
            "animus.plugin.call",
        ] {
            assert!(names.contains(&required.to_string()), "router missing {required}; have: {names:?}");
            assert!(server.tool_router.has_route(required));
        }
    }

    #[tokio::test]
    async fn plugin_list_returns_empty_for_isolated_project() {
        let (_home, _lock, _guards) = isolated_plugin_dirs();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let result = server
            .ao_plugin_list(Parameters(PluginListInput { project_root: None, include_system_path: Some(false) }))
            .await
            .expect("list ok");
        let payload = data(&result);
        let plugins = payload.pointer("/plugins").and_then(Value::as_array).expect("plugins array");
        assert!(
            plugins.is_empty(),
            "fresh isolated project should have zero plugins, got: {}",
            serde_json::to_string_pretty(plugins).unwrap_or_default()
        );
        // warnings field should be present but empty (or omitted by skip_serializing_if).
        let warnings = payload.pointer("/warnings").and_then(Value::as_array);
        if let Some(arr) = warnings {
            assert!(arr.is_empty(), "fresh project should have no warnings: {arr:?}");
        }
    }

    #[tokio::test]
    async fn plugin_install_uninstall_round_trip() {
        let (home, _lock, _guards) = isolated_plugin_dirs();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        // Stage a fake plugin binary outside the install dir.
        let staging = TempDir::new().expect("staging dir");
        let stage_path = staging.path().join("animus-plugin-mcp-fixture");
        write_fake_plugin_binary(&stage_path, "animus-plugin-mcp-fixture", "subject_backend");

        // Install via MCP.
        let install_result = server
            .ao_plugin_install(Parameters(PluginInstallInput {
                path: Some(stage_path.to_string_lossy().to_string()),
                force: Some(false),
                skip_manifest_check: Some(false),
                ..Default::default()
            }))
            .await
            .expect("install should succeed");
        let installed = data(&install_result);
        assert_eq!(
            installed.get("name").and_then(Value::as_str),
            Some("animus-plugin-mcp-fixture"),
            "installed payload should echo plugin name: {installed}"
        );
        let installed_path = installed.get("installed_path").and_then(Value::as_str).expect("installed_path string");
        assert!(
            installed_path.starts_with(home.path().to_str().expect("plugin dir utf-8")),
            "installed_path should land under isolated plugin dir: {installed_path}"
        );
        assert!(std::path::Path::new(installed_path).exists(), "installed binary should exist on disk");

        // Uninstall via MCP.
        let uninstall_result = server
            .ao_plugin_uninstall(Parameters(PluginUninstallInput {
                name: "animus-plugin-mcp-fixture".to_string(),
                plugin_dir: None,
            }))
            .await
            .expect("uninstall should succeed");
        let uninstalled = data(&uninstall_result);
        assert_eq!(uninstalled.get("name").and_then(Value::as_str), Some("animus-plugin-mcp-fixture"));
        let removed_path = uninstalled.get("removed_path").and_then(Value::as_str).expect("removed_path");
        assert_eq!(removed_path, installed_path, "removed_path should match installed_path");
        assert!(!std::path::Path::new(installed_path).exists(), "binary should be removed from disk");
    }

    #[tokio::test]
    async fn plugin_install_rejects_missing_source_and_path() {
        let (_home, _lock, _guards) = isolated_plugin_dirs();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let err = server
            .ao_plugin_install(Parameters(PluginInstallInput::default()))
            .await
            .expect_err("missing source/path/url should be rejected");
        let msg = err.message.to_string();
        assert!(
            msg.contains("source") && msg.contains("path") && msg.contains("url"),
            "error should mention all three source options: {msg}"
        );
    }

    #[tokio::test]
    async fn plugin_install_rejects_url_without_sha256() {
        let (_home, _lock, _guards) = isolated_plugin_dirs();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let err = server
            .ao_plugin_install(Parameters(PluginInstallInput {
                url: Some("https://example.invalid/plugin".to_string()),
                ..Default::default()
            }))
            .await
            .expect_err("url without sha256 should be rejected");
        let msg = err.message.to_string();
        assert!(msg.contains("sha256"), "error should mention sha256: {msg}");
    }

    #[tokio::test]
    async fn plugin_uninstall_unknown_plugin_returns_error() {
        let (_home, _lock, _guards) = isolated_plugin_dirs();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let err = server
            .ao_plugin_uninstall(Parameters(PluginUninstallInput {
                name: "animus-plugin-does-not-exist".to_string(),
                plugin_dir: None,
            }))
            .await
            .expect_err("missing plugin should error");
        let msg = err.message.to_string();
        assert!(msg.contains("not installed") || msg.contains("not found"), "error should be descriptive: {msg}");
    }

    #[tokio::test]
    async fn plugin_info_unknown_plugin_returns_error() {
        let (_home, _lock, _guards) = isolated_plugin_dirs();
        let project = TempDir::new().expect("project tempdir");
        let server = new_ao_mcp_server(&project_root_for(&project));

        let err = server
            .ao_plugin_info(Parameters(PluginInfoInput {
                project_root: None,
                name: "animus-plugin-does-not-exist".to_string(),
                include_system_path: Some(false),
            }))
            .await
            .expect_err("missing plugin should error");
        let msg = err.message.to_string();
        assert!(msg.contains("not found") || msg.contains("plugin"), "error should mention plugin: {msg}");
    }
}
