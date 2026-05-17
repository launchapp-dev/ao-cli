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

        // Scan the global plugin install dir when `$ANIMUS_PLUGIN_DIR` is set.
        // This makes env-var-driven installs (and existing binaries dropped
        // into that dir manually) discoverable even when the registry yaml is
        // absent. Without the env var we rely on the registry entry written
        // by `animus plugin install`, which avoids surprising tests that
        // expect zero plugins on the developer's real `~/.animus/plugins/`.
        if let Some(install_dir) = plugin_install_dir_from_env() {
            scan_dir(&install_dir, DiscoverySource::PluginPath, &mut discovered, &mut warnings, &mut seen);
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
                    discovered.push(DiscoveredPlugin { name, path, manifest, source: DiscoverySource::ExplicitConfig });
                }
                Err(error) => {
                    let reason = format!("{error:#}");
                    tracing::warn!(
                        plugin = %name,
                        path = %path.display(),
                        source = "explicit_config",
                        "plugin manifest probe failed: {reason}"
                    );
                    warnings.push(DiscoveryWarning { name, path, source: DiscoverySource::ExplicitConfig, reason });
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
            anyhow::bail!("plugin manifest command failed for {} (exit={:?})", path.display(), output.status.code());
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
                warnings.push(DiscoveryWarning { name: file_name.to_string(), path: path.clone(), source, reason });
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

/// Canonical home for Animus state. Mirrors `protocol::Config::global_config_dir()`
/// but duplicated here to avoid a crate-level dep on `protocol`. Honors
/// `ANIMUS_CONFIG_DIR` for tests and overrides.
fn animus_home() -> PathBuf {
    if let Ok(value) = std::env::var("ANIMUS_CONFIG_DIR") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".animus")).unwrap_or_else(|| PathBuf::from(".animus"))
}

/// Returns the canonical plugin install directory.
///
/// Resolution order:
/// 1. `$ANIMUS_PLUGIN_DIR` (when set and non-empty)
/// 2. `<animus_home>/plugins`
pub fn plugin_install_dir() -> PathBuf {
    if let Ok(value) = std::env::var("ANIMUS_PLUGIN_DIR") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    animus_home().join("plugins")
}

/// Returns the install dir only when `$ANIMUS_PLUGIN_DIR` is explicitly set
/// (and non-empty). Used by discovery to scan env-var-driven install dirs
/// without surprising users who never opted into the env var.
fn plugin_install_dir_from_env() -> Option<PathBuf> {
    let value = std::env::var("ANIMUS_PLUGIN_DIR").ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

/// Returns the canonical plugin registry yaml path.
///
/// The new location is `<animus_home>/plugins.yaml`. The legacy location
/// (`~/.config/animus/plugins.yaml`) is consulted automatically by
/// [`default_config_path`] when the new file does not yet exist, and is
/// migrated to the new path on the next write performed by the installer.
pub fn plugins_registry_path() -> PathBuf {
    animus_home().join("plugins.yaml")
}

/// Legacy registry location used before consolidation under `~/.animus/`.
/// Kept for one-shot migration on first read.
pub fn legacy_plugins_registry_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config/animus/plugins.yaml"))
        .unwrap_or_else(|| PathBuf::from(".config/animus/plugins.yaml"))
}

fn default_config_path() -> PathBuf {
    let canonical = plugins_registry_path();
    if canonical.exists() {
        return canonical;
    }
    // When the caller has explicitly redirected `$ANIMUS_CONFIG_DIR`
    // (typically tests), respect that isolation and skip the legacy
    // `~/.config/animus/plugins.yaml` fallback — otherwise stale entries
    // from a developer's real home would leak into isolated runs.
    let config_dir_overridden = std::env::var("ANIMUS_CONFIG_DIR").map(|v| !v.trim().is_empty()).unwrap_or(false);
    if !config_dir_overridden {
        let legacy = legacy_plugins_registry_path();
        if legacy.exists() {
            return legacy;
        }
    }
    canonical
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
        let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _clear_plugin_dir = EnvVarGuard::set("ANIMUS_PLUGIN_DIR", "");
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

        let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _clear_plugin_dir = EnvVarGuard::set("ANIMUS_PLUGIN_DIR", "");
        let temp = tempfile::tempdir().expect("tempdir");
        let plugin = temp.path().join("animus-provider-explode");
        // Plugin script that fails when --manifest is invoked. Simulates the
        // oai/linear regression where a missing env var aborted the manifest
        // probe and the plugin silently disappeared from `animus plugin list`.
        fs::write(&plugin, "#!/bin/sh\necho 'OPENAI_API_KEY not set' >&2\nexit 1\n").expect("write plugin");
        let mut permissions = fs::metadata(&plugin).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&plugin, permissions).expect("chmod");

        let config_path = temp.path().join("plugins.yaml");
        fs::write(&config_path, format!("providers:\n  explode:\n    binary: {}\n", plugin.to_string_lossy()))
            .expect("write config");

        let (discovered, warnings) =
            PluginDiscovery::new().with_config_path(config_path).discover_with_warnings().expect("discover");

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
        let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _clear_plugin_dir = EnvVarGuard::set("ANIMUS_PLUGIN_DIR", "");
        let temp = tempfile::tempdir().expect("tempdir");
        let config_path = temp.path().join("plugins.yaml");
        fs::write(&config_path, "plugins:\n  ghost:\n    binary: /tmp/definitely-not-a-real-plugin-binary-xyz123\n")
            .expect("write config");

        let (discovered, warnings) =
            PluginDiscovery::new().with_config_path(config_path).discover_with_warnings().expect("discover");

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

        let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _clear_plugin_dir = EnvVarGuard::set("ANIMUS_PLUGIN_DIR", "");
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

    // ---- env-var-driven path resolution ---------------------------------
    //
    // The helpers below read `$ANIMUS_PLUGIN_DIR` / `$ANIMUS_CONFIG_DIR` /
    // `$HOME`. Cargo runs tests on multiple threads in the same process, so we
    // serialize the env-touching tests behind a mutex to avoid races with
    // other tests in this module that may also read these vars.
    static ENV_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(prev) => std::env::set_var(self.key, prev),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn plugin_install_dir_honors_animus_plugin_dir_env_var() {
        let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let custom = temp.path().join("custom-plugins");
        let _env = EnvVarGuard::set("ANIMUS_PLUGIN_DIR", &custom);

        let resolved = plugin_install_dir();

        assert_eq!(resolved, custom, "$ANIMUS_PLUGIN_DIR must drive plugin_install_dir()");
    }

    #[cfg(unix)]
    #[test]
    fn discovery_uses_animus_plugin_dir_env_var() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let install_dir = temp.path().join("env-install-dir");
        fs::create_dir_all(&install_dir).expect("mkdir install dir");

        let manifest = serde_json::json!({
            "name": "animus-provider-envoy",
            "version": "0.1.0",
            "plugin_kind": "provider",
            "description": "test plugin",
            "protocol_version": "1.0.0",
            "capabilities": []
        });
        let plugin_path = install_dir.join("animus-provider-envoy");
        fs::write(&plugin_path, format!("#!/bin/sh\nprintf '{}\\n'\n", manifest)).expect("write plugin");
        let mut permissions = fs::metadata(&plugin_path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&plugin_path, permissions).expect("chmod");

        let empty_config = temp.path().join("empty-plugins.yaml");
        fs::write(&empty_config, "plugins: {}\n").expect("write empty config");

        let _plugin_dir = EnvVarGuard::set("ANIMUS_PLUGIN_DIR", &install_dir);

        let (discovered, warnings) =
            PluginDiscovery::new().with_config_path(&empty_config).discover_with_warnings().expect("discover");

        assert!(warnings.is_empty(), "expected zero warnings, got {warnings:?}");
        assert_eq!(discovered.len(), 1, "$ANIMUS_PLUGIN_DIR install dir must be scanned, got {discovered:?}");
        assert_eq!(discovered[0].name, "animus-provider-envoy");
        assert_eq!(discovered[0].path, plugin_path);
        assert_eq!(discovered[0].source, DiscoverySource::PluginPath);
    }

    #[test]
    fn plugin_registry_path_falls_back_to_legacy_when_canonical_missing() {
        let _guard = ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_home = temp.path().join("fake-home");
        fs::create_dir_all(&fake_home).expect("mkdir fake home");
        let legacy_dir = fake_home.join(".config/animus");
        fs::create_dir_all(&legacy_dir).expect("mkdir legacy");
        let legacy_file = legacy_dir.join("plugins.yaml");
        fs::write(&legacy_file, "plugins: {}\n").expect("write legacy");

        let _home = EnvVarGuard::set("HOME", &fake_home);
        let animus_home_dir = fake_home.join(".animus");
        let _config = EnvVarGuard::set("ANIMUS_CONFIG_DIR", &animus_home_dir);

        // Ensure ANIMUS_PLUGIN_DIR doesn't bleed through from other tests.
        let _plugin_dir_clear = EnvVarGuard::set("ANIMUS_PLUGIN_DIR", "");

        let canonical = plugins_registry_path();
        assert_eq!(canonical, animus_home_dir.join("plugins.yaml"));
        assert!(!canonical.exists(), "canonical registry path should not exist yet in this test");

        let resolved = default_config_path();
        assert_eq!(
            resolved, legacy_file,
            "default_config_path() must fall back to the legacy location when canonical is absent"
        );
    }
}
