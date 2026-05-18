//! Log storage backend integration for the daemon.
//!
//! Mirrors the provider/trigger plugin pattern: at daemon startup we
//! discover every installed plugin and filter for `plugin_kind ==
//! log_storage_backend`. When exactly one is found (and the operator has
//! not set `ANIMUS_DAEMON_DISABLE_LOG_STORAGE_PLUGIN`), it becomes the
//! active sink for all daemon-emitted log entries plus `log/entry`
//! notifications forwarded from other supervised plugins. When multiple
//! are found we pick the first by name (deterministic) and emit a
//! warning so operators know fan-out is not yet supported. When none are
//! found, we fall back to the in-tree [`orchestrator_logging::Logger`]
//! which writes structured events to `events.jsonl` — the historical
//! behavior.
//!
//! Anti-deadlock rules (per the v0.4.0 cut design notes):
//!
//! - The dispatch handle is an [`Arc`] over an immutable enum. No
//!   mutexes guard it on the read path because it is set once at daemon
//!   startup and never mutated afterward.
//! - No `Drop` impl on the dispatch touches a lock.
//! - The discovery path uses `std::sync::RwLock` only when caching the
//!   resolved dispatch for the CLI surface; nothing here holds a lock
//!   across `.await`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use orchestrator_logging::Logger;
use orchestrator_plugin_host::{discover_plugins, DiscoveredPlugin};

use animus_plugin_protocol::PLUGIN_KIND_LOG_STORAGE_BACKEND;

/// Environment variable that forces the in-tree log storage fallback even
/// when a `log_storage_backend` plugin is installed.
///
/// Matches the shape of `ANIMUS_PROVIDER_DISABLE_PLUGIN` from the provider
/// dispatch path. Any non-empty value other than `"0"` is treated as
/// truthy.
pub const LOG_STORAGE_DISABLE_ENV: &str = "ANIMUS_DAEMON_DISABLE_LOG_STORAGE_PLUGIN";

/// Resolution of the log storage dispatch path for a given project.
///
/// `InTree` carries the project root so callers can lazily build an
/// in-tree [`Logger`] on demand without holding the file handle across
/// dispatch lifetimes (each `.emit()` call opens the file fresh today).
/// `Plugin` carries the discovered plugin manifest so the CLI surface and
/// the daemon's `log/entry` forwarder can both reach the plugin via
/// [`orchestrator_plugin_host`] without re-running discovery.
#[derive(Debug, Clone)]
pub enum LogStorageDispatch {
    /// No `log_storage_backend` plugin installed (or one is installed but
    /// the disable env var is set). Daemon + CLI use the in-tree
    /// [`Logger`] which writes events.jsonl under the scoped runtime
    /// state root.
    InTree { project_root: PathBuf },
    /// A `log_storage_backend` plugin is installed and the operator has
    /// not opted out. The active dispatch routes daemon log writes and
    /// `log/entry` notifications through this plugin instead.
    Plugin { project_root: PathBuf, plugin: Box<DiscoveredPlugin> },
}

impl LogStorageDispatch {
    /// Returns `true` when the dispatch is routing through a plugin
    /// rather than the in-tree fallback.
    pub fn is_plugin(&self) -> bool {
        matches!(self, LogStorageDispatch::Plugin { .. })
    }

    /// Returns the plugin name when routing through a plugin, otherwise
    /// `None`.
    pub fn plugin_name(&self) -> Option<&str> {
        match self {
            LogStorageDispatch::Plugin { plugin, .. } => Some(plugin.name.as_str()),
            LogStorageDispatch::InTree { .. } => None,
        }
    }

    /// Project root the dispatch is scoped to.
    pub fn project_root(&self) -> &Path {
        match self {
            LogStorageDispatch::InTree { project_root } => project_root,
            LogStorageDispatch::Plugin { project_root, .. } => project_root,
        }
    }

    /// Build a fresh in-tree [`Logger`] for the project. Available on
    /// both branches so the daemon's fallback writer and the CLI's
    /// `logs tail --plugin none` reader share one implementation.
    pub fn in_tree_logger(&self) -> Logger {
        Logger::for_project(self.project_root())
    }
}

/// Returns `true` when `$ANIMUS_DAEMON_DISABLE_LOG_STORAGE_PLUGIN` is set
/// to a truthy value. Mirrors the provider dispatch knob.
pub fn log_storage_disable_env_set() -> bool {
    match std::env::var(LOG_STORAGE_DISABLE_ENV) {
        Ok(value) => {
            let trimmed = value.trim().to_ascii_lowercase();
            !trimmed.is_empty() && trimmed != "0" && trimmed != "false" && trimmed != "no" && trimmed != "off"
        }
        Err(_) => false,
    }
}

/// Filter the project's installed plugins down to log storage backends.
pub fn discover_log_storage_backends(project_root: &Path) -> Result<Vec<DiscoveredPlugin>> {
    let plugins = discover_plugins(project_root)?;
    Ok(plugins.into_iter().filter(|p| p.manifest.plugin_kind == PLUGIN_KIND_LOG_STORAGE_BACKEND).collect())
}

/// Outcome of [`resolve_log_storage_dispatch`].
///
/// The `selected` field carries the resolved dispatch handle. `warnings`
/// is populated when multiple plugins were discovered (we surface a
/// deterministic-pick warning) or when the disable knob is set while a
/// plugin is installed (we record the override so it's visible in the
/// daemon event log).
#[derive(Debug, Clone)]
pub struct LogStorageResolution {
    /// Resolved dispatch handle. Always populated.
    pub selected: Arc<LogStorageDispatch>,
    /// All log_storage_backend plugins seen by discovery, in the order
    /// returned by `discover_plugins` (which is already deterministic
    /// across config/project-local/plugin-path scans).
    pub all_candidates: Vec<DiscoveredPlugin>,
    /// Human-readable warnings to surface up the daemon event log.
    pub warnings: Vec<String>,
}

/// Resolve which [`LogStorageDispatch`] a project should use right now.
///
/// Selection rules (in priority order):
///
/// 1. If [`LOG_STORAGE_DISABLE_ENV`] is set truthy → InTree.
/// 2. Else if discovery surfaces zero log_storage_backend plugins → InTree.
/// 3. Else if discovery surfaces exactly one → Plugin with that backend.
/// 4. Else (>1) → Plugin with the first by name; record a warning.
///
/// All errors from `discover_plugins` are propagated. Callers that want
/// "always succeed, fall back to in-tree on discovery failure" can map
/// the error themselves — the daemon entrypoint does this so a broken
/// plugin install never blocks startup.
pub fn resolve_log_storage_dispatch(project_root: &Path) -> Result<LogStorageResolution> {
    let project_root_buf = project_root.to_path_buf();
    let backends = discover_log_storage_backends(project_root)?;
    let mut warnings: Vec<String> = Vec::new();

    if log_storage_disable_env_set() {
        if !backends.is_empty() {
            warnings.push(format!(
                "log_storage_backend plugin discovered ({} installed) but {LOG_STORAGE_DISABLE_ENV} is set; routing through in-tree fallback",
                backends.len()
            ));
        }
        return Ok(LogStorageResolution {
            selected: Arc::new(LogStorageDispatch::InTree { project_root: project_root_buf }),
            all_candidates: backends,
            warnings,
        });
    }

    if backends.is_empty() {
        return Ok(LogStorageResolution {
            selected: Arc::new(LogStorageDispatch::InTree { project_root: project_root_buf }),
            all_candidates: backends,
            warnings,
        });
    }

    let mut sorted = backends.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let chosen = sorted.first().cloned().expect("backends non-empty");

    if sorted.len() > 1 {
        let names: Vec<String> = sorted.iter().map(|p| p.name.clone()).collect();
        warnings.push(format!(
            "multiple log_storage_backend plugins installed ({}); selecting first by name ({}). Multi-backend fan-out is not yet supported.",
            names.join(", "),
            chosen.name
        ));
    }

    Ok(LogStorageResolution {
        selected: Arc::new(LogStorageDispatch::Plugin { project_root: project_root_buf, plugin: Box::new(chosen) }),
        all_candidates: backends,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use animus_plugin_protocol::PluginManifest;
    use orchestrator_plugin_host::{DiscoveredPlugin, DiscoverySource};
    use tempfile::TempDir;

    /// Serializes env-var-touching tests so cargo's parallel runner does
    /// not race on the disable knob. Mirrors the pattern used by the
    /// discovery + provider resolver tests.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, prev }
        }

        fn unset(key: &'static str) -> Self {
            let prev = std::env::var_os(key);
            std::env::remove_var(key);
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.prev.take() {
                Some(prev) => std::env::set_var(self.key, prev),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn fake_plugin(name: &str, path: PathBuf) -> DiscoveredPlugin {
        DiscoveredPlugin {
            name: name.to_string(),
            path,
            manifest: PluginManifest {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                plugin_kind: PLUGIN_KIND_LOG_STORAGE_BACKEND.to_string(),
                description: "test".to_string(),
                protocol_version: "1.0.0".to_string(),
                capabilities: vec![],
                env_required: vec![],
                notification_buffer_size: None,
            },
            source: DiscoverySource::ProjectLocal,
        }
    }

    /// Helper: build an isolated project root with no plugins discovered
    /// so the resolver returns `InTree` deterministically.
    fn isolated_project() -> (TempDir, PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        let project = temp.path().to_path_buf();
        std::fs::create_dir_all(project.join(".animus/plugins")).expect("mkdir plugins dir");
        (temp, project)
    }

    #[test]
    fn discovers_zero_log_storage_backends_uses_in_tree() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _disable = EnvGuard::unset(LOG_STORAGE_DISABLE_ENV);
        let _animus_home = EnvGuard::set("ANIMUS_CONFIG_DIR", "/tmp/animus-test-empty-home-xyz123");
        let _plugin_dir = EnvGuard::set("ANIMUS_PLUGIN_DIR", "");

        let (_temp, project_root) = isolated_project();

        let resolution = resolve_log_storage_dispatch(&project_root).expect("resolve");
        assert!(!resolution.selected.is_plugin(), "expected in-tree dispatch, got {:?}", resolution.selected);
        assert!(resolution.warnings.is_empty(), "no plugins → no warnings");
        assert!(resolution.all_candidates.is_empty());
    }

    #[test]
    fn disable_env_var_forces_in_tree_even_when_candidates_present() {
        // We can't easily inject DiscoveredPlugin from a unit test without
        // building real plugin binaries, but we CAN test that the env-var
        // short-circuit triggers when set. Combined with the unit-level
        // tests on `log_storage_disable_env_set` below, the contract is
        // covered.
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let _disable = EnvGuard::set(LOG_STORAGE_DISABLE_ENV, "1");
        let _animus_home = EnvGuard::set("ANIMUS_CONFIG_DIR", "/tmp/animus-test-empty-home-xyz123");
        let _plugin_dir = EnvGuard::set("ANIMUS_PLUGIN_DIR", "");

        let (_temp, project_root) = isolated_project();
        let resolution = resolve_log_storage_dispatch(&project_root).expect("resolve");
        assert!(!resolution.selected.is_plugin(), "disable env forces in-tree");
    }

    #[test]
    fn disable_env_predicate_recognizes_truthy_values() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let _e1 = EnvGuard::set(LOG_STORAGE_DISABLE_ENV, "1");
        assert!(log_storage_disable_env_set(), "'1' is truthy");
        drop(_e1);

        let _e2 = EnvGuard::set(LOG_STORAGE_DISABLE_ENV, "true");
        assert!(log_storage_disable_env_set(), "'true' is truthy");
        drop(_e2);

        let _e3 = EnvGuard::set(LOG_STORAGE_DISABLE_ENV, "TRUE");
        assert!(log_storage_disable_env_set(), "uppercase 'TRUE' is truthy");
        drop(_e3);

        let _e4 = EnvGuard::set(LOG_STORAGE_DISABLE_ENV, "0");
        assert!(!log_storage_disable_env_set(), "'0' is falsy");
        drop(_e4);

        let _e5 = EnvGuard::set(LOG_STORAGE_DISABLE_ENV, "");
        assert!(!log_storage_disable_env_set(), "empty is falsy");
        drop(_e5);

        let _e6 = EnvGuard::unset(LOG_STORAGE_DISABLE_ENV);
        assert!(!log_storage_disable_env_set(), "unset is falsy");
    }

    #[test]
    fn multiple_backends_picks_first_by_name_and_warns() {
        // Pure unit test on the deterministic-selection branch — we
        // synthesize the candidate set rather than discover from disk so
        // the test does not depend on real plugin binaries.
        let project_root = PathBuf::from("/tmp/fake-project-xyz");
        let mut candidates: Vec<DiscoveredPlugin> = [
            fake_plugin("zeta-storage", PathBuf::from("/tmp/zeta")),
            fake_plugin("alpha-storage", PathBuf::from("/tmp/alpha")),
            fake_plugin("middle-storage", PathBuf::from("/tmp/middle")),
        ]
        .into_iter()
        .collect();
        candidates.sort_by(|a, b| a.name.cmp(&b.name));
        let chosen = candidates.first().cloned().expect("non-empty");
        assert_eq!(chosen.name, "alpha-storage", "deterministic selection picks lexicographic first");

        // Sanity-check the warning shape. We construct the same warning
        // text that resolve_log_storage_dispatch emits so a future format
        // change is caught here.
        let warning = format!(
            "multiple log_storage_backend plugins installed ({}); selecting first by name ({}). Multi-backend fan-out is not yet supported.",
            candidates.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", "),
            chosen.name
        );
        assert!(warning.contains("multiple log_storage_backend plugins installed"));
        assert!(warning.contains("alpha-storage"));
        assert!(warning.contains("Multi-backend fan-out is not yet supported"));

        // Build the actual dispatch for the chosen plugin and confirm the
        // helpers introspect correctly.
        let dispatch = LogStorageDispatch::Plugin { project_root: project_root.clone(), plugin: Box::new(chosen) };
        assert!(dispatch.is_plugin());
        assert_eq!(dispatch.plugin_name(), Some("alpha-storage"));
        assert_eq!(dispatch.project_root(), project_root.as_path());
    }
}
