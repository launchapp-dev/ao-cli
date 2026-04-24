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

    pub fn include_system_path(mut self, include_system_path: bool) -> Self {
        self.include_system_path = include_system_path;
        self
    }

    pub fn discover(&self) -> Result<Vec<DiscoveredPlugin>> {
        let mut discovered = Vec::new();
        let mut seen = HashSet::new();

        self.discover_configured(&mut discovered, &mut seen)?;

        if let Some(project_root) = &self.project_root {
            scan_dir(&project_root.join(".ao/plugins"), DiscoverySource::ProjectLocal, &mut discovered, &mut seen);
        }

        if let Ok(plugin_path) = std::env::var("AO_PLUGIN_PATH") {
            for raw_dir in plugin_path.split(':') {
                if !raw_dir.trim().is_empty() {
                    scan_dir(Path::new(raw_dir), DiscoverySource::PluginPath, &mut discovered, &mut seen);
                }
            }
        }

        if self.include_system_path {
            if let Some(path_var) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path_var) {
                    scan_dir(&dir, DiscoverySource::SystemPath, &mut discovered, &mut seen);
                }
            }
        }

        Ok(discovered)
    }

    fn discover_configured(&self, discovered: &mut Vec<DiscoveredPlugin>, seen: &mut HashSet<String>) -> Result<()> {
        let config_path = self.config_path.clone().unwrap_or_else(default_config_path);
        if !config_path.exists() {
            return Ok(());
        }

        let config = load_plugins_config(&config_path)
            .with_context(|| format!("failed to read plugin config at {}", config_path.display()))?;
        for (logical_name, entry) in config.plugins.iter().chain(config.providers.iter()) {
            let Some(path) = find_binary(&expand_home(&entry.binary)) else {
                continue;
            };
            let name = entry.name.clone().unwrap_or_else(|| logical_name.clone());
            if seen.contains(&name) {
                continue;
            }
            if let Ok(manifest) = fetch_manifest(&path) {
                seen.insert(name.clone());
                discovered.push(DiscoveredPlugin { name, path, manifest, source: DiscoverySource::ExplicitConfig });
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
        anyhow::bail!("plugin manifest command failed for {}", path.display());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn scan_dir(dir: &Path, source: DiscoverySource, discovered: &mut Vec<DiscoveredPlugin>, seen: &mut HashSet<String>) {
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
        if let Ok(manifest) = fetch_manifest(&path) {
            seen.insert(file_name.to_string());
            discovered.push(DiscoveredPlugin { name: file_name.to_string(), path, manifest, source });
        }
    }
}

fn is_scanned_plugin_name(name: &str) -> bool {
    name.starts_with("ao-plugin-") || name.starts_with("ao-provider-")
}

fn load_plugins_config(path: &Path) -> Result<PluginsConfig> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

fn default_config_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/ao/plugins.yaml"))
        .unwrap_or_else(|| PathBuf::from(".config/ao/plugins.yaml"))
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
}
