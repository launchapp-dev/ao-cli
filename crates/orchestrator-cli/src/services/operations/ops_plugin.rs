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

/// Install a plugin binary from a public GitHub release (`owner/repo[@tag]`),
/// a local path, or an HTTPS URL. Wired into both the CLI
/// (`handle_plugin_install`) and the MCP install tool.
pub(crate) async fn run_plugin_install(req: PluginInstallRequest) -> Result<PluginInstallOutput> {
    let provided = [req.source.is_some(), req.path.is_some(), req.url.is_some()].iter().filter(|b| **b).count();
    if provided == 0 {
        return Err(invalid_input_error("one of `source` (owner/repo[@tag]), `path`, or `url` must be provided"));
    }
    if provided > 1 {
        return Err(invalid_input_error("`source`, `path`, and `url` are mutually exclusive"));
    }
    if req.tag.is_some() && req.source.is_none() {
        return Err(invalid_input_error("`tag` only applies when installing from a public repo (`source`)"));
    }
    if req.url.is_some() && req.sha256.is_none() {
        return Err(invalid_input_error(
            "--sha256 is required when installing from a URL; compute via `shasum -a 256 <plugin>`",
        ));
    }

    let (source_path, default_name, provenance) = if let Some(slug) = req.source.as_deref() {
        let spec = parse_repo_spec(slug)?;
        let release = resolve_release_install(spec, req.tag.clone()).await?;
        let provenance = InstallProvenance {
            source_kind: Some("release"),
            origin: Some(release.origin.clone()),
            release_tag: Some(release.release_tag.clone()),
            asset_name: Some(release.asset_name.clone()),
            sha256_verified: Some(release.sha256_verified),
        };
        (release.binary_path, release.plugin_name_hint, provenance)
    } else if let Some(p) = req.path.as_deref() {
        (PathBuf::from(p), String::new(), InstallProvenance { source_kind: Some("path"), ..Default::default() })
    } else if let Some(u) = req.url.as_deref() {
        let expected = req
            .sha256
            .as_deref()
            .ok_or_else(|| invalid_input_error("sha256 is required when installing from a URL"))?;
        let path = fetch_url_to_temp(u, expected).await?;
        let provenance = InstallProvenance {
            source_kind: Some("url"),
            origin: Some(u.to_string()),
            sha256_verified: Some(true),
            ..Default::default()
        };
        (path, String::new(), provenance)
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
        None => {
            if !default_name.is_empty() {
                default_name
            } else {
                source_path
                    .file_name()
                    .and_then(|f| f.to_str())
                    .ok_or_else(|| invalid_input_error("could not derive plugin name from source path"))?
                    .to_string()
            }
        }
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
        if let Some(kind) = provenance.source_kind {
            map.insert(
                serde_yaml::Value::String("source_kind".to_string()),
                serde_yaml::Value::String(kind.to_string()),
            );
        }
        if let Some(origin) = provenance.origin.as_ref() {
            map.insert(serde_yaml::Value::String("origin".to_string()), serde_yaml::Value::String(origin.clone()));
        }
        if let Some(tag) = provenance.release_tag.as_ref() {
            map.insert(serde_yaml::Value::String("release_tag".to_string()), serde_yaml::Value::String(tag.clone()));
        }
        if let Some(asset) = provenance.asset_name.as_ref() {
            map.insert(serde_yaml::Value::String("asset".to_string()), serde_yaml::Value::String(asset.clone()));
        }
        map.insert(serde_yaml::Value::String("sha256".to_string()), serde_yaml::Value::String(computed_sha.clone()));
        map.insert(
            serde_yaml::Value::String("installed_at".to_string()),
            serde_yaml::Value::String(chrono::Utc::now().to_rfc3339()),
        );
        map
    };
    let table = match manifest.as_ref().map(|m| m.plugin_kind.as_str()) {
        Some("provider") => &mut config.providers,
        _ => &mut config.plugins,
    };
    table.insert(serde_yaml::Value::String(plugin_name.clone()), serde_yaml::Value::Mapping(entry));
    save_plugins_yaml(&yaml_path, &config)?;

    let sha256_verified = match provenance.sha256_verified {
        Some(verified) => verified,
        None => req.sha256.is_some(),
    };

    Ok(PluginInstallOutput {
        name: plugin_name,
        installed_path: installed_path.to_string_lossy().to_string(),
        sha256: computed_sha,
        manifest,
        plugins_yaml: yaml_path.to_string_lossy().to_string(),
        source_kind: provenance.source_kind,
        origin: provenance.origin,
        release_tag: provenance.release_tag,
        asset_name: provenance.asset_name,
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

// ===== Public-repo (GitHub release) install support =====

/// Parsed `owner/repo[@tag]` positional source.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoSpec {
    owner: String,
    repo: String,
    tag: Option<String>,
}

/// Parse an `owner/repo` or `owner/repo@tag` slug. Whitespace is trimmed.
fn parse_repo_spec(raw: &str) -> Result<RepoSpec> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(invalid_input_error("repo source must not be empty"));
    }
    let (slug, tag) = match trimmed.split_once('@') {
        Some((slug, tag)) => {
            let tag = tag.trim();
            if tag.is_empty() {
                return Err(invalid_input_error(format!("repo source '{trimmed}' has an empty tag after '@'")));
            }
            (slug.trim(), Some(tag.to_string()))
        }
        None => (trimmed, None),
    };
    let (owner, repo) = slug.split_once('/').ok_or_else(|| {
        invalid_input_error(format!("repo source '{trimmed}' must be in the form 'owner/repo[@tag]'"))
    })?;
    let owner = owner.trim();
    let repo = repo.trim();
    if owner.is_empty() || repo.is_empty() {
        return Err(invalid_input_error(format!("repo source '{trimmed}' must be in the form 'owner/repo[@tag]'")));
    }
    Ok(RepoSpec { owner: owner.to_string(), repo: repo.to_string(), tag })
}

/// Returns the list of platform-target substrings to match against asset names,
/// in priority order. The first asset whose name contains any of these
/// substrings (case-insensitive) is selected.
fn current_platform_tokens() -> &'static [&'static str] {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        &["aarch64-apple-darwin", "macos-aarch64", "darwin-arm64", "darwin-aarch64", "macos-arm64"]
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        &["x86_64-apple-darwin", "macos-x86_64", "darwin-amd64", "darwin-x86_64", "macos-amd64"]
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        &["x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl", "linux-x86_64", "linux-amd64"]
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl", "linux-aarch64", "linux-arm64"]
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        &["x86_64-pc-windows-msvc", "x86_64-pc-windows-gnu", "windows-x86_64", "windows-amd64"]
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        &[]
    }
}

/// Human-readable label for the current build target.
fn current_platform_label() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GithubReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// Pure asset-selection helper. Picks the first asset whose name contains any
/// of `platform_tokens` (case-insensitive). Sidecar `.sha256` assets are
/// excluded from candidate matching.
fn pick_release_asset<'a>(
    assets: &'a [GithubReleaseAsset],
    platform_tokens: &[&str],
) -> Option<&'a GithubReleaseAsset> {
    for token in platform_tokens {
        let needle = token.to_ascii_lowercase();
        for asset in assets {
            let lower = asset.name.to_ascii_lowercase();
            if lower.ends_with(".sha256") || lower.ends_with(".sha256sum") {
                continue;
            }
            if lower.contains(&needle) {
                return Some(asset);
            }
        }
    }
    None
}

/// Look up the sidecar `<asset_name>.sha256` in the same release, if present.
fn find_sha256_sidecar<'a>(assets: &'a [GithubReleaseAsset], asset_name: &str) -> Option<&'a GithubReleaseAsset> {
    let sidecar = format!("{asset_name}.sha256");
    assets.iter().find(|a| a.name.eq_ignore_ascii_case(&sidecar))
}

/// Build the GitHub releases API URL for either `latest` or a specific tag.
fn github_release_api_url(owner: &str, repo: &str, tag: Option<&str>) -> String {
    match tag {
        Some(tag) => format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}"),
        None => format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"),
    }
}

fn release_user_agent() -> String {
    format!("animus-cli/{}", env!("CARGO_PKG_VERSION"))
}

async fn fetch_github_release(owner: &str, repo: &str, tag: Option<&str>) -> Result<GithubRelease> {
    let url = github_release_api_url(owner, repo, tag);
    let client =
        reqwest::Client::builder().user_agent(release_user_agent()).build().context("failed to build HTTP client")?;
    let response = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .with_context(|| format!("failed to GET {url}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(not_found_error(format!(
            "no release found at {url} (check the repo slug, tag, or whether a release has been published yet)"
        )));
    }
    let response = response.error_for_status().with_context(|| format!("GET {url} returned non-success status"))?;
    let release: GithubRelease =
        response.json().await.with_context(|| format!("failed to parse GitHub release JSON from {url}"))?;
    Ok(release)
}

/// Parse an `algo:hex` digest string (as returned by the GitHub API's `digest`
/// field on release assets), returning the lowercased hex if the algorithm is
/// `sha256`. Returns `None` for unsupported algorithms.
fn parse_release_digest(digest: &str) -> Option<String> {
    let trimmed = digest.trim();
    let (algo, hex) = trimmed.split_once(':')?;
    if !algo.eq_ignore_ascii_case("sha256") {
        return None;
    }
    let hex = hex.trim();
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_lowercase())
}

/// Parse a sidecar file body (commonly `<hex>  <filename>\n`), returning the
/// leading hex digest if present.
fn parse_sha256_sidecar(body: &str) -> Option<String> {
    let line = body.lines().next()?.trim();
    let token = line.split_whitespace().next()?;
    if token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(token.to_ascii_lowercase())
    } else {
        None
    }
}

async fn download_to_path(url: &str, dest: &Path) -> Result<()> {
    let client =
        reqwest::Client::builder().user_agent(release_user_agent()).build().context("failed to build HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("download from {url} returned non-success status"))?;
    let bytes = response.bytes().await.with_context(|| format!("failed to read body from {url}"))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create download dir {}", parent.display()))?;
    }
    std::fs::write(dest, &bytes).with_context(|| format!("failed to write {}", dest.display()))?;
    Ok(())
}

async fn download_text(url: &str) -> Result<String> {
    let client =
        reqwest::Client::builder().user_agent(release_user_agent()).build().context("failed to build HTTP client")?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("download from {url} returned non-success status"))?;
    response.text().await.with_context(|| format!("failed to read body from {url}"))
}

/// Extract a `.tar.gz` archive into `dest_dir`. Returns the path of the
/// first regular file found inside the archive (the plugin binary).
fn extract_tarball(archive: &Path, dest_dir: &Path) -> Result<PathBuf> {
    let file = std::fs::File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar_reader = tar::Archive::new(gz);
    std::fs::create_dir_all(dest_dir)
        .with_context(|| format!("failed to create extract dir {}", dest_dir.display()))?;
    tar_reader
        .unpack(dest_dir)
        .with_context(|| format!("failed to extract {} into {}", archive.display(), dest_dir.display()))?;

    fn first_file(dir: &Path) -> Result<Option<PathBuf>> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                return Ok(Some(path));
            }
            if metadata.is_dir() {
                if let Some(found) = first_file(&path)? {
                    return Ok(Some(found));
                }
            }
        }
        Ok(None)
    }

    first_file(dest_dir)?.ok_or_else(|| anyhow!("tarball {} contained no regular files", archive.display()))
}

/// Result of resolving a public-repo install source.
#[derive(Debug)]
struct ReleaseInstall {
    binary_path: PathBuf,
    plugin_name_hint: String,
    asset_name: String,
    release_tag: String,
    origin: String,
    sha256_verified: bool,
}

async fn resolve_release_install(spec: RepoSpec, explicit_tag: Option<String>) -> Result<ReleaseInstall> {
    let tag = match (spec.tag.clone(), explicit_tag) {
        (Some(spec_tag), Some(flag_tag)) => {
            if spec_tag != flag_tag {
                return Err(invalid_input_error(format!(
                    "conflicting tag: positional says '{spec_tag}', --tag says '{flag_tag}'"
                )));
            }
            Some(spec_tag)
        }
        (Some(tag), None) | (None, Some(tag)) => Some(tag),
        (None, None) => None,
    };

    let release = fetch_github_release(&spec.owner, &spec.repo, tag.as_deref()).await?;
    let platform_tokens = current_platform_tokens();
    if platform_tokens.is_empty() {
        return Err(invalid_input_error(format!(
            "current platform '{}' is not supported by `animus plugin install` (no asset selectors registered)",
            current_platform_label()
        )));
    }

    let asset = pick_release_asset(&release.assets, platform_tokens).ok_or_else(|| {
        let available: Vec<String> = release.assets.iter().map(|a| a.name.clone()).collect();
        invalid_input_error(format!(
            "no release asset matched current platform '{}' (looked for any of: {}). Available assets in {}: [{}]",
            current_platform_label(),
            platform_tokens.join(", "),
            release.tag_name,
            available.join(", ")
        ))
    })?;

    let temp_dir = std::env::temp_dir().join(format!("animus-plugin-install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).with_context(|| format!("failed to create temp dir {}", temp_dir.display()))?;

    let asset_path = temp_dir.join(&asset.name);
    download_to_path(&asset.browser_download_url, &asset_path).await?;

    // Resolve expected SHA256: sidecar asset > release `digest` field.
    let mut expected_sha: Option<String> = None;
    if let Some(sidecar_asset) = find_sha256_sidecar(&release.assets, &asset.name) {
        match download_text(&sidecar_asset.browser_download_url).await {
            Ok(body) => {
                if let Some(hex) = parse_sha256_sidecar(&body) {
                    expected_sha = Some(hex);
                } else {
                    eprintln!(
                        "warning: sha256 sidecar '{}' had unexpected format; skipping verification",
                        sidecar_asset.name
                    );
                }
            }
            Err(err) => {
                eprintln!("warning: failed to download sha256 sidecar '{}': {}", sidecar_asset.name, err);
            }
        }
    }
    if expected_sha.is_none() {
        if let Some(digest) = asset.digest.as_deref() {
            if let Some(hex) = parse_release_digest(digest) {
                expected_sha = Some(hex);
            }
        }
    }

    let mut sha256_verified = false;
    let computed = sha256_of_file(&asset_path)?;
    if let Some(expected) = expected_sha.as_ref() {
        if !expected.eq_ignore_ascii_case(&computed) {
            return Err(invalid_input_error(format!(
                "sha256 mismatch for asset '{}': expected {expected}, computed {computed}",
                asset.name
            )));
        }
        sha256_verified = true;
    } else {
        eprintln!(
            "warning: no sha256 sidecar or digest for asset '{}'; install proceeding without checksum verification",
            asset.name
        );
    }

    // Extract if tarball; otherwise treat as a bare binary.
    let lower = asset.name.to_ascii_lowercase();
    let binary_path = if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let extract_dir = temp_dir.join("extracted");
        extract_tarball(&asset_path, &extract_dir)?
    } else {
        asset_path.clone()
    };

    let plugin_name_hint = binary_path
        .file_name()
        .and_then(|f| f.to_str())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| spec.repo.clone());

    Ok(ReleaseInstall {
        binary_path,
        plugin_name_hint,
        asset_name: asset.name.clone(),
        release_tag: release.tag_name.clone(),
        origin: format!("{}/{}@{}", spec.owner, spec.repo, release.tag_name),
        sha256_verified,
    })
}

/// Provenance attached to a resolved install source. Recorded in the registry
/// and surfaced in the install output.
#[derive(Debug, Default)]
struct InstallProvenance {
    source_kind: Option<&'static str>,
    origin: Option<String>,
    release_tag: Option<String>,
    asset_name: Option<String>,
    /// `Some(true)` if checksum verification ran and passed during resolution.
    sha256_verified: Option<bool>,
}

async fn handle_plugin_install(args: PluginInstallArgs, json: bool) -> Result<()> {
    if args.latest && args.tag.is_some() {
        return Err(invalid_input_error("--latest and --tag are mutually exclusive"));
    }
    if (args.tag.is_some() || args.latest) && args.source.is_none() {
        return Err(invalid_input_error(
            "--tag and --latest only apply when installing from a public repo (positional OWNER/REPO[@TAG])",
        ));
    }
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
    let output = run_plugin_uninstall(PluginUninstallRequest { name: args.name, plugin_dir: args.plugin_dir })?;
    print_value(output, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GithubReleaseAsset {
        GithubReleaseAsset {
            name: name.to_string(),
            browser_download_url: format!("https://example.test/{name}"),
            digest: None,
        }
    }

    #[test]
    fn parses_repo_at_tag_syntax() {
        let spec = parse_repo_spec("launchapp-dev/animus-provider-claude@v0.1.0").unwrap();
        assert_eq!(spec.owner, "launchapp-dev");
        assert_eq!(spec.repo, "animus-provider-claude");
        assert_eq!(spec.tag.as_deref(), Some("v0.1.0"));
    }

    #[test]
    fn parses_repo_without_tag() {
        let spec = parse_repo_spec("launchapp-dev/animus-provider-claude").unwrap();
        assert_eq!(spec.owner, "launchapp-dev");
        assert_eq!(spec.repo, "animus-provider-claude");
        assert!(spec.tag.is_none());
    }

    #[test]
    fn parse_repo_spec_trims_whitespace() {
        let spec = parse_repo_spec("  launchapp-dev/foo @ v1.2.3  ").unwrap();
        assert_eq!(spec.owner, "launchapp-dev");
        assert_eq!(spec.repo, "foo");
        assert_eq!(spec.tag.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn parse_repo_spec_rejects_missing_slash() {
        let err = parse_repo_spec("animus-provider-claude").unwrap_err();
        assert!(format!("{err}").contains("owner/repo"));
    }

    #[test]
    fn parse_repo_spec_rejects_empty_tag() {
        let err = parse_repo_spec("launchapp-dev/foo@").unwrap_err();
        assert!(format!("{err}").contains("empty tag"));
    }

    #[test]
    fn parse_repo_spec_rejects_empty_owner_or_repo() {
        assert!(parse_repo_spec("/foo").is_err());
        assert!(parse_repo_spec("foo/").is_err());
        assert!(parse_repo_spec("").is_err());
    }

    #[test]
    fn selects_aarch64_apple_darwin_asset() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz"),
        ];
        let tokens: &[&str] = &["aarch64-apple-darwin", "macos-aarch64", "darwin-arm64"];
        let picked = pick_release_asset(&assets, tokens).expect("expected an asset to match");
        assert_eq!(picked.name, "animus-provider-oai-aarch64-apple-darwin.tar.gz");
    }

    #[test]
    fn selects_x86_64_linux_asset() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz"),
        ];
        let tokens: &[&str] = &["x86_64-unknown-linux-gnu", "linux-x86_64", "linux-amd64"];
        let picked = pick_release_asset(&assets, tokens).expect("expected an asset to match");
        assert_eq!(picked.name, "animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz");
    }

    #[test]
    fn errors_clearly_when_no_matching_asset() {
        let assets = vec![asset("animus-provider-oai-x86_64-unknown-linux-gnu.tar.gz")];
        let tokens: &[&str] = &["aarch64-apple-darwin", "macos-aarch64"];
        assert!(pick_release_asset(&assets, tokens).is_none());
    }

    #[test]
    fn current_platform_has_known_tokens() {
        let tokens = current_platform_tokens();
        assert!(!tokens.is_empty(), "no platform tokens registered for {}", current_platform_label());
    }

    #[test]
    fn excludes_sha256_sidecars_from_asset_picking() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz.sha256"),
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
        ];
        let tokens: &[&str] = &["aarch64-apple-darwin"];
        let picked = pick_release_asset(&assets, tokens).expect("expected the binary asset to win");
        assert_eq!(picked.name, "animus-provider-oai-aarch64-apple-darwin.tar.gz");
    }

    #[test]
    fn verifies_sha256_sidecar_when_present() {
        let assets = vec![
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz"),
            asset("animus-provider-oai-aarch64-apple-darwin.tar.gz.sha256"),
        ];
        let sidecar = find_sha256_sidecar(&assets, "animus-provider-oai-aarch64-apple-darwin.tar.gz")
            .expect("expected sidecar to be found");
        assert_eq!(sidecar.name, "animus-provider-oai-aarch64-apple-darwin.tar.gz.sha256");

        let body =
            "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48  animus-provider-oai-aarch64-apple-darwin.tar.gz\n";
        let hex = parse_sha256_sidecar(body).unwrap();
        assert_eq!(hex, "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48");
    }

    #[test]
    fn returns_none_when_no_sha256_sidecar() {
        let assets = vec![asset("animus-provider-oai-aarch64-apple-darwin.tar.gz")];
        assert!(find_sha256_sidecar(&assets, "animus-provider-oai-aarch64-apple-darwin.tar.gz").is_none());
    }

    #[test]
    fn parses_release_digest_field() {
        let hex =
            parse_release_digest("sha256:a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48").unwrap();
        assert_eq!(hex, "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48");
        assert!(parse_release_digest("md5:deadbeef").is_none());
        assert!(parse_release_digest("sha256:").is_none());
        assert!(parse_release_digest("not-a-digest").is_none());
        assert!(parse_release_digest("sha256:deadbeef").is_none()); // too short
    }

    #[test]
    fn parses_hex_only_sidecar_body() {
        let body = "A10A3A505CA102BC4249D4E660F0622278ABD319054A2E033B72988783EA7A48\n";
        let hex = parse_sha256_sidecar(body).unwrap();
        assert_eq!(hex, "a10a3a505ca102bc4249d4e660f0622278abd319054a2e033b72988783ea7a48");
    }

    #[test]
    fn rejects_malformed_sidecar_body() {
        assert!(parse_sha256_sidecar("").is_none());
        assert!(parse_sha256_sidecar("not-a-hex-digest").is_none());
        assert!(parse_sha256_sidecar("deadbeef").is_none()); // too short
    }

    #[test]
    fn github_release_api_url_is_correct() {
        assert_eq!(
            github_release_api_url("launchapp-dev", "animus-provider-oai", None),
            "https://api.github.com/repos/launchapp-dev/animus-provider-oai/releases/latest"
        );
        assert_eq!(
            github_release_api_url("launchapp-dev", "animus-provider-oai", Some("v0.1.0")),
            "https://api.github.com/repos/launchapp-dev/animus-provider-oai/releases/tags/v0.1.0"
        );
    }

    #[test]
    fn extract_tarball_returns_first_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("plugin.tar.gz");
        let bin_path = dir.path().join("animus-provider-foo");
        std::fs::File::create(&bin_path).unwrap().write_all(b"#!/bin/sh\necho ok\n").unwrap();
        {
            let file = std::fs::File::create(&archive_path).unwrap();
            let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(enc);
            builder.append_path_with_name(&bin_path, "animus-provider-foo").unwrap();
            let enc = builder.into_inner().unwrap();
            enc.finish().unwrap();
        }

        let extract_dir = dir.path().join("extracted");
        let extracted = extract_tarball(&archive_path, &extract_dir).unwrap();
        assert_eq!(extracted.file_name().and_then(|n| n.to_str()), Some("animus-provider-foo"));
        assert!(extracted.exists());
    }
}
