use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use orchestrator_plugin_host::{
    discover_plugins, DiscoveredPlugin, DiscoverySource, PluginDiscovery, PluginHost,
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
struct DiscoveredPluginRow {
    name: String,
    version: String,
    plugin_kind: String,
    description: String,
    protocol_version: String,
    capabilities: Vec<String>,
    source: &'static str,
    path: String,
}

#[derive(Debug, Serialize)]
struct PluginInfoOutput {
    name: String,
    source: &'static str,
    path: String,
    manifest: PluginManifest,
    initialize: Value,
}

#[derive(Debug, Serialize)]
struct PluginCallOutput {
    name: String,
    method: String,
    result: Value,
}

#[derive(Debug, Serialize)]
struct PluginPingOutput {
    name: String,
    ok: bool,
    plugin_info: Value,
}

#[derive(Debug, Serialize)]
struct PluginInstallOutput {
    name: String,
    installed_path: String,
    sha256: String,
    manifest: Option<PluginManifest>,
    plugins_yaml: String,
}

#[derive(Debug, Serialize)]
struct PluginUninstallOutput {
    name: String,
    removed_path: Option<String>,
    plugins_yaml: String,
}

pub(crate) async fn handle_plugin(command: PluginCommand, project_root: &str, json: bool) -> Result<()> {
    match command {
        PluginCommand::List(args) => handle_plugin_list(args, project_root, json),
        PluginCommand::Info(args) => handle_plugin_info(args, project_root, json).await,
        PluginCommand::Call(args) => handle_plugin_call(args, project_root, json).await,
        PluginCommand::Ping(args) => handle_plugin_ping(args, project_root, json).await,
        PluginCommand::Install(args) => handle_plugin_install(args, json).await,
        PluginCommand::Uninstall(args) => handle_plugin_uninstall(args, json),
    }
}

fn discover(project_root: &str, include_system_path: bool) -> Result<Vec<DiscoveredPlugin>> {
    PluginDiscovery::new()
        .with_project_root(Path::new(project_root))
        .include_system_path(include_system_path)
        .discover()
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
    let discovered = discover(project_root, args.include_system_path)?;
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
    print_value(rows, json)
}

fn find_plugin(project_root: &str, name: &str, include_system_path: bool) -> Result<DiscoveredPlugin> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(invalid_input_error("--name must not be empty"));
    }
    let mut matches = if include_system_path {
        discover(project_root, true)?
    } else {
        discover_plugins(Path::new(project_root))?
    };
    matches.retain(|plugin| plugin.name == trimmed);
    matches.pop().ok_or_else(|| not_found_error(format!("plugin not found: {trimmed}")))
}

async fn handle_plugin_info(args: PluginInfoArgs, project_root: &str, json: bool) -> Result<()> {
    let discovered = find_plugin(project_root, &args.name, args.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    let _ = host.shutdown().await;

    let output = PluginInfoOutput {
        name: discovered.name,
        source: source_label(discovered.source),
        path: discovered.path.display().to_string(),
        manifest: discovered.manifest,
        initialize: serde_json::to_value(initialize)?,
    };
    print_value(output, json)
}

async fn handle_plugin_call(args: PluginCallArgs, project_root: &str, json: bool) -> Result<()> {
    let method = args.method.trim().to_string();
    if method.is_empty() {
        return Err(invalid_input_error("--method must not be empty"));
    }
    let params = match args.params {
        Some(raw) => Some(serde_json::from_str::<Value>(&raw).context("--params must be valid JSON")?),
        None => None,
    };

    let discovered = find_plugin(project_root, &args.name, args.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let _ = host.handshake().await.context("plugin initialize failed")?;
    let result = host
        .request(method.clone(), params)
        .await
        .map_err(|err| anyhow!("plugin call failed ({}): {}", err.code, err.message))?;
    let _ = host.shutdown().await;

    print_value(PluginCallOutput { name: discovered.name, method, result }, json)
}

async fn handle_plugin_ping(args: PluginPingArgs, project_root: &str, json: bool) -> Result<()> {
    let discovered = find_plugin(project_root, &args.name, args.include_system_path)?;
    let mut host = PluginHost::spawn(&discovered.path, &[]).await.context("failed to spawn plugin")?;
    let initialize = host.handshake().await.context("plugin initialize failed")?;
    host.ping().await.context("plugin ping failed")?;
    let _ = host.shutdown().await;

    print_value(
        PluginPingOutput { name: discovered.name, ok: true, plugin_info: serde_json::to_value(initialize.plugin_info)? },
        json,
    )
}

fn install_root() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not resolve $HOME"))?;
    let dir = home.join(".ao").join("plugins");
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create install dir {}", dir.display()))?;
    Ok(dir)
}

fn plugins_yaml_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not resolve $HOME"))?;
    let dir = home.join(".config").join("ao");
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create config dir {}", dir.display()))?;
    Ok(dir.join("plugins.yaml"))
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

async fn fetch_url_to_temp(url: &str) -> Result<PathBuf> {
    if !url.starts_with("https://") {
        return Err(invalid_input_error("--url must use https://"));
    }
    let response =
        reqwest::get(url).await.with_context(|| format!("failed to download {url}"))?.error_for_status().with_context(
            || format!("download from {url} returned non-success status"),
        )?;
    let bytes = response.bytes().await.with_context(|| format!("failed to read body from {url}"))?;
    let temp_dir = std::env::temp_dir().join(format!("ao-plugin-install-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir)?;
    let filename = url.rsplit('/').next().unwrap_or("plugin");
    let dest = temp_dir.join(filename);
    std::fs::write(&dest, &bytes).with_context(|| format!("failed to write downloaded plugin to {}", dest.display()))?;
    Ok(dest)
}

async fn handle_plugin_install(args: PluginInstallArgs, json: bool) -> Result<()> {
    let source_path = match (args.path.as_deref(), args.url.as_deref()) {
        (Some(p), None) => PathBuf::from(p),
        (None, Some(u)) => fetch_url_to_temp(u).await?,
        (Some(_), Some(_)) => return Err(invalid_input_error("--path and --url are mutually exclusive")),
        (None, None) => return Err(invalid_input_error("one of --path or --url must be provided")),
    };

    if !source_path.exists() {
        return Err(not_found_error(format!("plugin source not found: {}", source_path.display())));
    }
    if !source_path.is_file() {
        return Err(invalid_input_error(format!("plugin source is not a file: {}", source_path.display())));
    }

    let computed_sha = sha256_of_file(&source_path)?;
    if let Some(expected) = args.sha256.as_deref() {
        if !expected.eq_ignore_ascii_case(&computed_sha) {
            return Err(invalid_input_error(format!(
                "sha256 mismatch: expected {expected}, computed {computed_sha}"
            )));
        }
    }

    let plugin_name = match args.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => name.to_string(),
        None => source_path
            .file_name()
            .and_then(|f| f.to_str())
            .ok_or_else(|| invalid_input_error("could not derive plugin name from source path"))?
            .to_string(),
    };

    let install_dir = install_root()?;
    let installed_path = install_dir.join(&plugin_name);
    if installed_path.exists() && !args.force {
        return Err(invalid_input_error(format!(
            "plugin '{plugin_name}' already installed at {} (pass --force to overwrite)",
            installed_path.display()
        )));
    }

    std::fs::copy(&source_path, &installed_path).with_context(|| {
        format!("failed to copy {} → {}", source_path.display(), installed_path.display())
    })?;
    ensure_executable(&installed_path)?;

    let manifest = if args.skip_manifest_check {
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
            map.insert(
                serde_yaml::Value::String("name".to_string()),
                serde_yaml::Value::String(m.name.clone()),
            );
        }
        map
    };
    let table = match manifest.as_ref().map(|m| m.plugin_kind.as_str()) {
        Some("provider") => &mut config.providers,
        _ => &mut config.plugins,
    };
    table.insert(serde_yaml::Value::String(plugin_name.clone()), serde_yaml::Value::Mapping(entry));
    save_plugins_yaml(&yaml_path, &config)?;

    print_value(
        PluginInstallOutput {
            name: plugin_name,
            installed_path: installed_path.to_string_lossy().to_string(),
            sha256: computed_sha,
            manifest,
            plugins_yaml: yaml_path.to_string_lossy().to_string(),
        },
        json,
    )
}

fn handle_plugin_uninstall(args: PluginUninstallArgs, json: bool) -> Result<()> {
    let plugin_name = args.name.trim().to_string();
    if plugin_name.is_empty() {
        return Err(invalid_input_error("--name must not be empty"));
    }

    let yaml_path = plugins_yaml_path()?;
    let mut config = load_plugins_yaml(&yaml_path)?;
    let key = serde_yaml::Value::String(plugin_name.clone());
    let removed_in_yaml =
        config.plugins.remove(&key).is_some() || config.providers.remove(&key).is_some();
    if removed_in_yaml {
        save_plugins_yaml(&yaml_path, &config)?;
    }

    let install_dir = install_root()?;
    let installed_path = install_dir.join(&plugin_name);
    let removed = if installed_path.exists() {
        std::fs::remove_file(&installed_path).with_context(|| format!("failed to remove {}", installed_path.display()))?;
        Some(installed_path.to_string_lossy().to_string())
    } else {
        None
    };

    if !removed_in_yaml && removed.is_none() {
        return Err(not_found_error(format!("plugin '{plugin_name}' is not installed")));
    }

    print_value(
        PluginUninstallOutput {
            name: plugin_name,
            removed_path: removed,
            plugins_yaml: yaml_path.to_string_lossy().to_string(),
        },
        json,
    )
}
