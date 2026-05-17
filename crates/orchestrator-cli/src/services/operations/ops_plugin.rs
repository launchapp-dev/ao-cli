mod new;

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use orchestrator_plugin_host::{
    discover_plugins, legacy_plugins_registry_path, plugin_install_dir, plugins_registry_path, DiscoveredPlugin,
    DiscoverySource, DiscoveryWarning, PluginDiscovery, PluginHost,
};
use orchestrator_plugin_protocol::PluginManifest;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    invalid_input_error, not_found_error, print_value, PluginCallArgs, PluginCommand, PluginInfoArgs,
    PluginInstallArgs, PluginListArgs, PluginPingArgs, PluginUninstallArgs,
};

#[derive(Debug, Serialize)]
pub(crate) struct DiscoveredPluginRow {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) plugin_kind: String,
    pub(crate) description: String,
    pub(crate) protocol_version: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) source: &'static str,
    pub(crate) path: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginWarningRow {
    pub(crate) name: String,
    pub(crate) source: &'static str,
    pub(crate) path: String,
    pub(crate) reason: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginListOutput {
    pub(crate) plugins: Vec<DiscoveredPluginRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) warnings: Vec<PluginWarningRow>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginInfoOutput {
    pub(crate) name: String,
    pub(crate) source: &'static str,
    pub(crate) path: String,
    pub(crate) manifest: PluginManifest,
    pub(crate) initialize: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginCallOutput {
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) result: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginPingOutput {
    pub(crate) name: String,
    pub(crate) ok: bool,
    pub(crate) plugin_info: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginInstallOutput {
    pub(crate) name: String,
    pub(crate) installed_path: String,
    pub(crate) sha256: String,
    pub(crate) manifest: Option<PluginManifest>,
    pub(crate) plugins_yaml: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) release_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) asset_name: Option<String>,
    pub(crate) sha256_verified: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct PluginUninstallOutput {
    pub(crate) name: String,
    pub(crate) removed_path: Option<String>,
    pub(crate) plugins_yaml: String,
}

// ===== Typed request structs (shared between CLI and MCP) =====

/// Typed request for `plugin list`. Both CLI and MCP build one of these and
/// call [`run_plugin_list`]. The CLI handler additionally streams warnings to
/// stderr when in text mode; MCP returns warnings inside the structured payload.
#[derive(Debug, Clone, Default)]
pub(crate) struct PluginListRequest {
    pub(crate) project_root: String,
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginInfoRequest {
    pub(crate) project_root: String,
    pub(crate) name: String,
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginPingRequest {
    pub(crate) project_root: String,
    pub(crate) name: String,
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginCallRequest {
    pub(crate) project_root: String,
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) params: Option<Value>,
    pub(crate) include_system_path: bool,
}

/// Typed request for `plugin install`. Mirrors the CLI arg surface so MCP can
/// invoke the same install pipeline. Exactly one of `source` / `path` / `url`
/// must be supplied. When `url` is set, `sha256` is required. The `source`
/// (owner/repo[@tag]) input is forwarded to the CLI install pipeline; if the
/// underlying handler does not yet implement public-repo installs, a clear
/// error is returned.
#[derive(Debug, Clone, Default)]
pub(crate) struct PluginInstallRequest {
    pub(crate) source: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) tag: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) sha256: Option<String>,
    pub(crate) force: bool,
    pub(crate) skip_manifest_check: bool,
    /// Override the plugin install directory. Takes precedence over
    /// `$ANIMUS_PLUGIN_DIR`. When `None`, falls back to env / default.
    pub(crate) plugin_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PluginUninstallRequest {
    pub(crate) name: String,
    pub(crate) plugin_dir: Option<String>,
}

pub(crate) async fn handle_plugin(command: PluginCommand, project_root: &str, json: bool) -> Result<()> {
    match command {
        PluginCommand::List(args) => handle_plugin_list(args, project_root, json),
        PluginCommand::Info(args) => handle_plugin_info(args, project_root, json).await,
        PluginCommand::Call(args) => handle_plugin_call(args, project_root, json).await,
        PluginCommand::Ping(args) => handle_plugin_ping(args, project_root, json).await,
        PluginCommand::Install(args) => handle_plugin_install(args, json).await,
        PluginCommand::Uninstall(args) => handle_plugin_uninstall(args, json),
        PluginCommand::New(args) => new::handle_plugin_new(args, json),
    }
}

// ===== Reusable typed entry points (shared between CLI and MCP) =====

/// List discovered plugins. Identical surface as `animus plugin list`.
pub(crate) fn run_plugin_list(req: PluginListRequest) -> Result<PluginListOutput> {
    let (discovered, warnings) = discover_with_warnings(&req.project_root, req.include_system_path)?;
    let rows: Vec<DiscoveredPluginRow> = discovered
        .into_iter()
        .map(|plugin| DiscoveredPluginRow {
            name: plugin.name,
            version: plugin.manifest.version,
            plugin_kind: plugin.manifest.plugin_kind,
            description: plugin.manifest.description,
            protocol_version: plugin.manifest.protocol_version,
            capabilities: plugin.manifest.capabilities,
            source: source_label(plugin.source),
            path: plugin.path.display().to_string(),
        })
        .collect();
    let warning_rows: Vec<PluginWarningRow> = warnings
        .into_iter()
        .map(|warning| PluginWarningRow {
            name: warning.name,
            source: source_label(warning.source),
            path: warning.path.display().to_string(),
            reason: warning.reason,
        })
        .collect();
    Ok(PluginListOutput { plugins: rows, warnings: warning_rows })
}

/// Spawn the named plugin, complete the handshake, and return manifest +
/// initialize-time capabilities.
pub(crate) async fn run_plugin_info(req: PluginInfoRequest) -> Result<PluginInfoOutput> {
    let discovered = find_plugin(&req.project_root, &req.name, req.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    let _ = host.shutdown().await;
    Ok(PluginInfoOutput {
        name: discovered.name,
        source: source_label(discovered.source),
        path: discovered.path.display().to_string(),
        manifest: discovered.manifest,
        initialize: serde_json::to_value(initialize)?,
    })
}

/// Health-check a plugin by spawning it, completing the handshake, and
/// dispatching `$/ping`.
pub(crate) async fn run_plugin_ping(req: PluginPingRequest) -> Result<PluginPingOutput> {
    let discovered = find_plugin(&req.project_root, &req.name, req.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    host.ping().await.context("plugin ping failed")?;
    let _ = host.shutdown().await;
    Ok(PluginPingOutput { name: discovered.name, ok: true, plugin_info: serde_json::to_value(initialize.plugin_info)? })
}

/// Send a JSON-RPC request to a discovered plugin and return its response.
pub(crate) async fn run_plugin_call(req: PluginCallRequest) -> Result<PluginCallOutput> {
    let method = req.method.trim().to_string();
    if method.is_empty() {
        return Err(invalid_input_error("method must not be empty"));
    }
    let discovered = find_plugin(&req.project_root, &req.name, req.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let _ = host.handshake().await.context("plugin initialize failed")?;
    let result = host
        .request(method.clone(), req.params)
        .await
        .map_err(|err| anyhow!("plugin call failed ({}): {}", err.code, err.message))?;
    let _ = host.shutdown().await;
    Ok(PluginCallOutput { name: discovered.name, method, result })
}

/// Uninstall a plugin from the install dir and registry yaml.
pub(crate) fn run_plugin_uninstall(req: PluginUninstallRequest) -> Result<PluginUninstallOutput> {
    let plugin_name = req.name.trim().to_string();
    if plugin_name.is_empty() {
        return Err(invalid_input_error("name must not be empty"));
    }

    let yaml_path = plugins_yaml_path()?;
    let mut config = load_plugins_yaml(&yaml_path)?;
    let key = serde_yaml::Value::String(plugin_name.clone());
    let removed_in_yaml = config.plugins.remove(&key).is_some() || config.providers.remove(&key).is_some();
    if removed_in_yaml {
        save_plugins_yaml(&yaml_path, &config)?;
    }

    let install_dir = install_root(req.plugin_dir.as_deref())?;
    let installed_path = install_dir.join(&plugin_name);
    let removed = if installed_path.exists() {
        std::fs::remove_file(&installed_path)
            .with_context(|| format!("failed to remove {}", installed_path.display()))?;
        Some(installed_path.to_string_lossy().to_string())
    } else {
        None
    };

    if !removed_in_yaml && removed.is_none() {
        return Err(not_found_error(format!("plugin '{plugin_name}' is not installed")));
    }

    Ok(PluginUninstallOutput {
        name: plugin_name,
        removed_path: removed,
        plugins_yaml: yaml_path.to_string_lossy().to_string(),
    })
}

/// Install a plugin binary from a local path or HTTPS URL. The `source`
/// (owner/repo[@tag]) field is reserved for the public-repo install pipeline
/// being landed in parallel; until that pipeline is wired in, requests with
/// only `source` are rejected with a clear error pointing callers at `path`
/// or `url`.
pub(crate) async fn run_plugin_install(req: PluginInstallRequest) -> Result<PluginInstallOutput> {
    let provided = [req.source.is_some(), req.path.is_some(), req.url.is_some()].iter().filter(|b| **b).count();
    if provided == 0 {
        return Err(invalid_input_error("one of `source` (owner/repo[@tag]), `path`, or `url` must be provided"));
    }
    if provided > 1 {
        return Err(invalid_input_error("`source`, `path`, and `url` are mutually exclusive"));
    }
    if req.source.is_some() {
        return Err(invalid_input_error(
            "public-repo install (`source`) is not yet wired through the install pipeline; use `path` (local binary) or `url`+`sha256` (direct download)",
        ));
    }
    if req.url.is_some() && req.sha256.is_none() {
        return Err(invalid_input_error(
            "sha256 is required when installing from a URL; compute via `shasum -a 256 <plugin>`",
        ));
    }

    let source_path = if let Some(p) = req.path.as_deref() {
        PathBuf::from(p)
    } else if let Some(u) = req.url.as_deref() {
        let expected = req
            .sha256
            .as_deref()
            .ok_or_else(|| invalid_input_error("sha256 is required when installing from a URL"))?;
        fetch_url_to_temp(u, expected).await?
    } else {
        unreachable!("validated above");
    };

    if !source_path.exists() {
        return Err(not_found_error(format!("plugin source not found: {}", source_path.display())));
    }
    if !source_path.is_file() {
        return Err(invalid_input_error(format!("plugin source is not a file: {}", source_path.display())));
    }

    let computed_sha = sha256_of_file(&source_path)?;
    if let Some(expected) = req.sha256.as_deref() {
        if !expected.eq_ignore_ascii_case(&computed_sha) {
            return Err(invalid_input_error(format!("sha256 mismatch: expected {expected}, computed {computed_sha}")));
        }
    }

    let plugin_name = match req.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => name.to_string(),
        None => source_path
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| invalid_input_error("could not derive plugin name from source path"))?
            .to_string(),
    };

    let install_dir = install_root(req.plugin_dir.as_deref())?;
    let installed_path = install_dir.join(&plugin_name);
    if installed_path.exists() && !req.force {
        return Err(invalid_input_error(format!(
            "plugin '{plugin_name}' already installed at {} (pass force=true to overwrite)",
            installed_path.display()
        )));
    }

    std::fs::copy(&source_path, &installed_path)
        .with_context(|| format!("failed to copy {} → {}", source_path.display(), installed_path.display()))?;
    ensure_executable(&installed_path)?;

    let manifest = if req.skip_manifest_check {
        None
    } else {
        let output = std::process::Command::new(&installed_path)
            .arg("--manifest")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("failed to run {} --manifest", installed_path.display()))?;
        if !output.status.success() {
            let _ = std::fs::remove_file(&installed_path);
            return Err(anyhow!(
                "installed binary failed --manifest probe (exit={:?}): {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        match serde_json::from_slice::<PluginManifest>(&output.stdout) {
            Ok(manifest) => Some(manifest),
            Err(error) => {
                let _ = std::fs::remove_file(&installed_path);
                return Err(anyhow!("installed binary returned malformed --manifest JSON: {error}"));
            }
        }
    };

    let yaml_path = plugins_yaml_path()?;
    let mut config = load_plugins_yaml(&yaml_path)?;
    let entry: serde_yaml::Mapping = {
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("binary".to_string()),
            serde_yaml::Value::String(installed_path.to_string_lossy().to_string()),
        );
        if let Some(m) = manifest.as_ref() {
            map.insert(serde_yaml::Value::String("name".to_string()), serde_yaml::Value::String(m.name.clone()));
        }
        map
    };
    let table = match manifest.as_ref().map(|m| m.plugin_kind.as_str()) {
        Some("provider") => &mut config.providers,
        _ => &mut config.plugins,
    };
    table.insert(serde_yaml::Value::String(plugin_name.clone()), serde_yaml::Value::Mapping(entry));
    save_plugins_yaml(&yaml_path, &config)?;

    let sha256_verified = req.sha256.is_some();
    let (source_kind, origin) = if req.url.is_some() {
        (Some("url"), req.url.clone())
    } else if req.path.is_some() {
        (Some("path"), None)
    } else {
        (None, None)
    };

    Ok(PluginInstallOutput {
        name: plugin_name,
        installed_path: installed_path.to_string_lossy().to_string(),
        sha256: computed_sha,
        manifest,
        plugins_yaml: yaml_path.to_string_lossy().to_string(),
        source_kind,
        origin,
        release_tag: None,
        asset_name: None,
        sha256_verified,
    })
}

fn discover(project_root: &str, include_system_path: bool) -> Result<Vec<DiscoveredPlugin>> {
    PluginDiscovery::new()
        .with_project_root(Path::new(project_root))
        .include_system_path(include_system_path)
        .discover()
        .context("plugin discovery failed")
}

fn discover_with_warnings(
    project_root: &str,
    include_system_path: bool,
) -> Result<(Vec<DiscoveredPlugin>, Vec<DiscoveryWarning>)> {
    PluginDiscovery::new()
        .with_project_root(Path::new(project_root))
        .include_system_path(include_system_path)
        .discover_with_warnings()
        .context("plugin discovery failed")
}

fn source_label(source: DiscoverySource) -> &'static str {
    match source {
        DiscoverySource::ExplicitConfig => "explicit_config",
        DiscoverySource::ProjectLocal => "project_local",
        DiscoverySource::PluginPath => "plugin_path",
        DiscoverySource::SystemPath => "system_path",
    }
}

fn handle_plugin_list(args: PluginListArgs, project_root: &str, json: bool) -> Result<()> {
    let output = run_plugin_list(PluginListRequest {
        project_root: project_root.to_string(),
        include_system_path: args.include_system_path,
    })?;

    if !json {
        for warning in &output.warnings {
            eprintln!(
                "warning: plugin '{}' was discovered ({}) but could not be loaded: {} ({})",
                warning.name, warning.source, warning.reason, warning.path
            );
        }
    }

    print_value(output, json)
}

fn find_plugin(project_root: &str, name: &str, include_system_path: bool) -> Result<DiscoveredPlugin> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid_input_error("--name must not be empty"));
    }
    let mut matches =
        if include_system_path { discover(project_root, true)? } else { discover_plugins(Path::new(project_root))? };
    matches.retain(|plugin| plugin.name == trimmed);
    matches.pop().ok_or_else(|| not_found_error(format!("plugin not found: {trimmed}")))
}

async fn handle_plugin_info(args: PluginInfoArgs, project_root: &str, json: bool) -> Result<()> {
    let output = run_plugin_info(PluginInfoRequest {
        project_root: project_root.to_string(),
        name: args.name,
        include_system_path: args.include_system_path,
    })
    .await?;
    print_value(output, json)
}

async fn handle_plugin_call(args: PluginCallArgs, project_root: &str, json: bool) -> Result<()> {
    let params = match args.params {
        Some(raw) => Some(serde_json::from_str::<Value>(&raw).context("--params must be valid JSON")?),
        None => None,
    };
    let output = run_plugin_call(PluginCallRequest {
        project_root: project_root.to_string(),
        name: args.name,
        method: args.method,
        params,
        include_system_path: args.include_system_path,
    })
    .await?;
    print_value(output, json)
}

async fn handle_plugin_ping(args: PluginPingArgs, project_root: &str, json: bool) -> Result<()> {
    let output = run_plugin_ping(PluginPingRequest {
        project_root: project_root.to_string(),
        name: args.name,
        include_system_path: args.include_system_path,
    })
    .await?;
    print_value(output, json)
}

/// Resolves the plugin install directory.
///
/// Resolution order:
/// 1. `--plugin-dir <path>` CLI arg (when provided)
/// 2. `$ANIMUS_PLUGIN_DIR` env var (via [`plugin_install_dir`])
/// 3. Default `~/.animus/plugins/`
fn install_root(cli_override: Option<&str>) -> Result<PathBuf> {
    let dir = match cli_override.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => plugin_install_dir(),
    };
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create install dir {}", dir.display()))?;
    Ok(dir)
}

/// Resolves the plugin registry yaml path, performing a one-shot migration from
/// the legacy `~/.config/animus/plugins.yaml` location when needed.
fn plugins_yaml_path() -> Result<PathBuf> {
    let canonical = plugins_registry_path();
    if let Some(parent) = canonical.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("failed to create config dir {}", parent.display()))?;
    }

    if !canonical.exists() {
        let legacy = legacy_plugins_registry_path();
        if legacy.exists() {
            std::fs::copy(&legacy, &canonical).with_context(|| {
                format!("failed to migrate plugin registry from {} to {}", legacy.display(), canonical.display())
            })?;
            tracing::info!(
                from = %legacy.display(),
                to = %canonical.display(),
                "migrated plugin registry to canonical location",
            );
        }
    }

    Ok(canonical)
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct PluginsYamlConfig {
    #[serde(default)]
    plugins: serde_yaml::Mapping,
    #[serde(default)]
    providers: serde_yaml::Mapping,
}

fn load_plugins_yaml(path: &Path) -> Result<PluginsYamlConfig> {
    if !path.exists() {
        return Ok(PluginsYamlConfig::default());
    }
    let contents = std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_yaml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

fn save_plugins_yaml(path: &Path, config: &PluginsYamlConfig) -> Result<()> {
    let serialized = serde_yaml::to_string(config).context("failed to serialize plugins.yaml")?;
    std::fs::write(path, serialized).with_context(|| format!("failed to write {}", path.display()))
}

fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(unix)]
fn ensure_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn ensure_executable(_path: &Path) -> Result<()> {
    Ok(())
}

async fn fetch_url_to_temp(url: &str, expected_sha256: &str) -> Result<PathBuf> {
    if !url.starts_with("https://") {
        return Err(invalid_input_error("--url must use https://"));
    }
    let response = reqwest::get(url)
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("download from {url} returned non-success status"))?;
    let bytes = response.bytes().await.with_context(|| format!("failed to read body from {url}"))?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let computed_sha = format!("{:x}", hasher.finalize());
    if !expected_sha256.eq_ignore_ascii_case(&computed_sha) {
        return Err(invalid_input_error(format!(
            "sha256 mismatch for {url}: expected {expected_sha256}, computed {computed_sha}"
        )));
    }

    let temp_dir = std::env::temp_dir().join(format!("animus-plugin-install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;
    let filename = url.rsplit('/').next().unwrap_or("plugin");
    let dest = temp_dir.join(filename);
    std::fs::write(&dest, &bytes)
        .with_context(|| format!("failed to write downloaded plugin to {}", dest.display()))?;
    Ok(dest)
}

async fn handle_plugin_install(args: PluginInstallArgs, json: bool) -> Result<()> {
    let output = run_plugin_install(PluginInstallRequest {
        source: args.source,
        path: args.path,
        url: args.url,
        tag: args.tag,
        name: args.name,
        sha256: args.sha256,
        force: args.force,
        skip_manifest_check: args.skip_manifest_check,
        plugin_dir: args.plugin_dir,
    })
    .await?;
    print_value(output, json)
}

fn handle_plugin_uninstall(args: PluginUninstallArgs, json: bool) -> Result<()> {
    let output = run_plugin_uninstall(PluginUninstallRequest {
        name: args.name,
        plugin_dir: args.plugin_dir,
    })?;
    print_value(output, json)
}
