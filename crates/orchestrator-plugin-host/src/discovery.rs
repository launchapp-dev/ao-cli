use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use orchestrator_plugin_protocol::PluginManifest;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverySource {
    ExplicitConfig,
    ProjectLocal,
    PluginPath,
    SystemPath,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredPlugin {
    pub name: String,
    pub path: PathBuf,
    pub manifest: PluginManifest,
    pub source: DiscoverySource,
}

/// A plugin that was located on disk but could not be loaded — typically because
/// its `--manifest` probe failed. Surfaced alongside successful discoveries so
/// callers can tell users *why* an installed plugin disappeared instead of
/// silently dropping it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryWarning {
    pub name: String,
    pub path: PathBuf,
    pub source: DiscoverySource,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PluginConfigEntry {
    pub binary: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
struct PluginsConfig {
    #[serde(default)]
    plugins: HashMap<String, PluginConfigEntry>,
    #[serde(default)]
    providers: HashMap<String, PluginConfigEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct PluginDiscovery {
    project_root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    include_system_path: bool,
}

impl PluginDiscovery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_project_root(mut self, project_root: impl Into<PathBuf>) -> Self {
        self.project_root = Some(project_root.into());
        self
    }

    pub fn with_config_path(mut self, config_path: impl Into<PathBuf>) -> Self {
        self.config_path = Some(config_path.into());
        self
    }

    /// Opt in to scanning `$PATH` for `animus-plugin-*` / `animus-provider-*` binaries.
    ///
    /// Defaults to `false`. When enabled, [`PluginDiscovery::discover`] will
    /// execute every matching binary on `$PATH` with `--manifest` to fetch its
    /// manifest. This runs arbitrary executables found on the user's `$PATH`
    /// during discovery, so only enable when the caller explicitly trusts that
    /// surface.
    pub fn include_system_path(mut self, include_system_path: bool) -> Self {
        self.include_system_path = include_system_path;
        self
    }

    pub fn discover(&self) -> Result<Vec<DiscoveredPlugin>> {
        Ok(self.discover_with_warnings()?.0)
    }

    /// Like [`PluginDiscovery::discover`], but also returns a list of
    /// [`DiscoveryWarning`]s for plugins that were located but could not be
    /// loaded (e.g. their `--manifest` probe failed). Warnings are also emitted
    /// at `warn` level via `tracing`.
    pub fn discover_with_warnings(&self) -> Result<(Vec<DiscoveredPlugin>, Vec<DiscoveryWarning>)> {
        let mut discovered = Vec::new();
        let mut warnings = Vec::new();
        let mut seen = HashSet::new();

        self.discover_configured(&mut discovered, &mut warnings, &mut seen)?;

        if let Some(project_root) = &self.project_root {
            scan_dir(
                &project_root.join(".animus/plugins"),
                DiscoverySource::ProjectLocal,
                &mut discovered,
                &mut warnings,
                &mut seen,
            );
        }

        if let Ok(plugin_path) = std::env::var("ANIMUS_PLUGIN_PATH") {
            for raw_dir in plugin_path.split(':') {
                if !raw_dir.trim().is_empty() {
                    scan_dir(
                        Path::new(raw_dir),
                        DiscoverySource::PluginPath,
                        &mut discovered,
                        &mut warnings,
                        &mut seen,
                    );
                }
            }
        }

        if self.include_system_path {
            if let Some(path_var) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path_var) {
                    scan_dir(&dir, DiscoverySource::SystemPath, &mut discovered, &mut warnings, &mut seen);
                }
            }
        }

        Ok((discovered, warnings))
    }

    fn discover_configured(
        &self,
        discovered: &mut Vec<DiscoveredPlugin>,
        warnings: &mut Vec<DiscoveryWarning>,
        seen: &mut HashSet<String>,
    ) -> Result<()> {
        let config_path = self.config_path.clone().unwrap_or_else(default_config_path);
        if !config_path.exists() {
            return Ok(());
        }

        let config = load_plugins_config(&config_path)
            .with_context(|| format!("failed to read plugin config at {}", config_path.display()))?;
        for (logical_name, entry) in config.plugins.iter().chain(config.providers.iter()) {
            let Some(path) = find_binary(&expand_home(&entry.binary)) else {
                let reason = format!("configured binary not found: {}", entry.binary);
                tracing::warn!(
                    plugin = %logical_name,
                    binary = %entry.binary,
                    source = "explicit_config",
                    "plugin manifest probe skipped: {reason}"
                );
                warnings.push(DiscoveryWarning {
                    name: entry.name.clone().unwrap_or_else(|| logical_name.clone()),
                    path: PathBuf::from(&entry.binary),
                    source: DiscoverySource::ExplicitConfig,
                    reason,
                });
                continue;
            };
            let name = entry.name.clone().unwrap_or_else(|| logical_name.clone());
            if seen.contains(&name) {
                continue;
            }
            match fetch_manifest(&path) {
                Ok(manifest) => {
                    seen.insert(name.clone());
                    discovered.push(DiscoveredPlugin {
                        name,
                        path,
                        manifest,
                        source: DiscoverySource::ExplicitConfig,
                    });
                }
                Err(error) => {
                    let reason = format!("{error:#}");
                    tracing::warn!(
                        plugin = %name,
                        path = %path.display(),
                        source = "explicit_config",
                        "plugin manifest probe failed: {reason}"
                    );
                    warnings.push(DiscoveryWarning {
                        name,
                        path,
                        source: DiscoverySource::ExplicitConfig,
                        reason,
                    });
                }
            }
        }

        Ok(())
    }
}

pub fn discover_plugins(project_root: impl Into<PathBuf>) -> Result<Vec<DiscoveredPlugin>> {
    PluginDiscovery::new().with_project_root(project_root).discover()
}

pub fn fetch_manifest(path: &Path) -> Result<PluginManifest> {
    let output =
        Command::new(path).arg("--manifest").output().with_context(|| format!("failed to run {}", path.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let trimmed = stderr.trim();
        if trimmed.is_empty() {
            anyhow::bail!(
                "plugin manifest command failed for {} (exit={:?})",
                path.display(),
                output.status.code()
            );
        }
        anyhow::bail!(
            "plugin manifest command failed for {} (exit={:?}): {}",
            path.display(),
            output.status.code(),
            trimmed
        );
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("plugin {} returned malformed --manifest JSON", path.display()))
}

fn scan_dir(
    dir: &Path,
    source: DiscoverySource,
    discovered: &mut Vec<DiscoveredPlugin>,
    warnings: &mut Vec<DiscoveryWarning>,
    seen: &mut HashSet<String>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !is_scanned_plugin_name(file_name) || seen.contains(file_name) {
            continue;
        }
        match fetch_manifest(&path) {
            Ok(manifest) => {
                seen.insert(file_name.to_string());
                discovered.push(DiscoveredPlugin { name: file_name.to_string(), path, manifest, source });
            }
            Err(error) => {
                let reason = format!("{error:#}");
                tracing::warn!(
                    plugin = %file_name,
                    path = %path.display(),
                    source = ?source,
                    "plugin manifest probe failed: {reason}"
                );
                warnings.push(DiscoveryWarning {
                    name: file_name.to_string(),
                    path: path.clone(),
                    source,
                    reason,
                });
            }
        }
    }
}

fn is_scanned_plugin_name(name: &str) -> bool {
    name.starts_with("animus-plugin-") || name.starts_with("animus-provider-")
}

fn load_plugins_config(path: &Path) -> Result<PluginsConfig> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

fn default_config_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/animus/plugins.yaml"))
        .unwrap_or_else(|| PathBuf::from(".config/animus/plugins.yaml"))
}

fn expand_home(value: &str) -> String {
    let Some(rest) = value.strip_prefix("~/") else {
        return value.to_string();
    };
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(rest).to_string_lossy().to_string())
        .unwrap_or_else(|| value.to_string())
}

fn find_binary(value: &str) -> Option<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute() || value.contains(std::path::MAIN_SEPARATOR) {
        return path.exists().then_some(path);
    }

    std::env::var_os("PATH").and_then(|path_var| {
        std::env::split_paths(&path_var).map(|dir| dir.join(value)).find(|candidate| candidate.exists())
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn default_discovery_does_not_scan_system_path() {
        let discovery = PluginDiscovery::new();
        assert!(!discovery.include_system_path, "PluginDiscovery::new() must not opt into $PATH scanning by default");
        assert!(
            !PluginDiscovery::default().include_system_path,
            "PluginDiscovery::default() must not opt into $PATH scanning"
        );
    }

    #[test]
    fn configured_plugin_can_use_non_prefixed_binary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plugin = temp.path().join("compatible-plugin");
        let manifest = serde_json::json!({
            "name": "compatible",
            "version": "0.1.0",
            "plugin_kind": "custom",
            "description": "test",
            "protocol_version": "1.0.0",
            "capabilities": []
        });
        fs::write(&plugin, format!("#!/bin/sh\nprintf '{}\\n'\n", manifest)).expect("write plugin");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&plugin).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&plugin, permissions).expect("chmod");
        }

        let config_path = temp.path().join("plugins.yaml");
        fs::write(&config_path, format!("plugins:\n  compatible:\n    binary: {}\n", plugin.to_string_lossy()))
            .expect("write config");

        let discovered = PluginDiscovery::new().with_config_path(config_path).discover().expect("discover");

        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].name, "compatible");
    }

    #[cfg(unix)]
    #[test]
    fn failed_manifest_probe_surfaces_warning_instead_of_silent_drop() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let plugin = temp.path().join("animus-provider-explode");
        // Plugin script that fails when --manifest is invoked. Simulates the
        // oai/linear regression where a missing env var aborted the manifest
        // probe and the plugin silently disappeared from `animus plugin list`.
        fs::write(
            &plugin,
            "#!/bin/sh\necho 'OPENAI_API_KEY not set' >&2\nexit 1\n",
        )
        .expect("write plugin");
        let mut permissions = fs::metadata(&plugin).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&plugin, permissions).expect("chmod");

        let config_path = temp.path().join("plugins.yaml");
        fs::write(
            &config_path,
            format!("providers:\n  explode:\n    binary: {}\n", plugin.to_string_lossy()),
        )
        .expect("write config");

        let (discovered, warnings) = PluginDiscovery::new()
            .with_config_path(config_path)
            .discover_with_warnings()
            .expect("discover");

        assert!(discovered.is_empty(), "plugin with failed manifest must not appear in discovered list");
        assert_eq!(warnings.len(), 1, "expected exactly one discovery warning, got {warnings:?}");
        let warning = &warnings[0];
        assert_eq!(warning.name, "explode");
        assert_eq!(warning.path, plugin);
        assert_eq!(warning.source, DiscoverySource::ExplicitConfig);
        assert!(
            warning.reason.contains("manifest"),
            "warning reason should mention the manifest failure, got: {}",
            warning.reason
        );
    }

    #[cfg(unix)]
    #[test]
    fn missing_configured_binary_surfaces_warning() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("plugins.yaml");
        fs::write(
            &config_path,
            "plugins:\n  ghost:\n    binary: /tmp/definitely-not-a-real-plugin-binary-xyz123\n",
        )
        .expect("write config");

        let (discovered, warnings) = PluginDiscovery::new()
            .with_config_path(config_path)
            .discover_with_warnings()
            .expect("discover");

        assert!(discovered.is_empty());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].name, "ghost");
        assert_eq!(warnings[0].source, DiscoverySource::ExplicitConfig);
        assert!(warnings[0].reason.contains("not found"));
    }

    #[cfg(unix)]
    #[test]
    fn scan_dir_failed_manifest_surfaces_warning() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let plugins_dir = temp.path().join(".animus/plugins");
        fs::create_dir_all(&plugins_dir).expect("mkdir");
        let plugin = plugins_dir.join("animus-plugin-broken");
        fs::write(&plugin, "#!/bin/sh\nexit 2\n").expect("write plugin");
        let mut permissions = fs::metadata(&plugin).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&plugin, permissions).expect("chmod");

        // Point discovery at an empty config so only the project-local scan runs.
        let empty_config = temp.path().join("plugins.yaml");
        fs::write(&empty_config, "plugins: {}\n").expect("write empty config");

        let (discovered, warnings) = PluginDiscovery::new()
            .with_project_root(temp.path())
            .with_config_path(empty_config)
            .discover_with_warnings()
            .expect("discover");

        assert!(discovered.is_empty());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].name, "animus-plugin-broken");
        assert_eq!(warnings[0].source, DiscoverySource::ProjectLocal);
    }
}
