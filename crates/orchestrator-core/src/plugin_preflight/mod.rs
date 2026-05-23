mod runner;

#[cfg(test)]
mod tests;

pub use runner::{PluginInstaller, PluginPreflightRunner};

use animus_plugin_protocol::{PLUGIN_KIND_PROVIDER, PLUGIN_KIND_SUBJECT_BACKEND};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PROVIDER_REPO: &str = "launchapp-dev/animus-provider-claude@v0.1.0";
pub const DEFAULT_TASK_BACKEND_REPO: &str = "launchapp-dev/animus-subject-linear@v0.1.0";
pub const DEFAULT_REQUIREMENT_BACKEND_REPO: &str = "launchapp-dev/animus-subject-linear@v0.1.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum RequiredRole {
    AtLeastOneProvider,
    SubjectKind(String),
    TransportEnabled,
}

impl RequiredRole {
    pub fn label(&self) -> String {
        match self {
            RequiredRole::AtLeastOneProvider => "at_least_one_provider".to_string(),
            RequiredRole::SubjectKind(kind) => format!("subject_kind:{kind}"),
            RequiredRole::TransportEnabled => "transport_enabled".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPreflightSpec {
    pub required_roles: Vec<RequiredRole>,
    pub auto_install: bool,
    pub auto_install_defaults: Vec<(String, String)>,
}

impl PluginPreflightSpec {
    pub fn daemon_default() -> Self {
        Self {
            required_roles: vec![
                RequiredRole::AtLeastOneProvider,
                RequiredRole::SubjectKind("task".to_string()),
                RequiredRole::SubjectKind("requirement".to_string()),
            ],
            auto_install: false,
            auto_install_defaults: vec![
                ("at_least_one_provider".to_string(), DEFAULT_PROVIDER_REPO.to_string()),
                ("subject_kind:task".to_string(), DEFAULT_TASK_BACKEND_REPO.to_string()),
                ("subject_kind:requirement".to_string(), DEFAULT_REQUIREMENT_BACKEND_REPO.to_string()),
            ],
        }
    }

    pub fn with_auto_install(mut self) -> Self {
        self.auto_install = true;
        self
    }

    pub fn install_target_for(&self, role_label: &str) -> Option<&str> {
        self.auto_install_defaults.iter().find(|(label, _)| label == role_label).map(|(_, repo)| repo.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreflightResult {
    pub satisfied: Vec<String>,
    pub missing: Vec<MissingPlugin>,
    pub auto_installed: Vec<AutoInstalledPlugin>,
}

impl PreflightResult {
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty()
    }

    pub fn render_missing_message(&self) -> String {
        if self.missing.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        out.push_str("plugin preflight failed: the daemon requires plugins that are not installed.\n");
        for missing in &self.missing {
            out.push_str(&format!("  - role `{}` unsatisfied; fix: `{}`\n", missing.role, missing.fix_command));
        }
        out.push_str(
            "Re-run with `--auto-install` to install defaults, or run `animus plugin install <repo>@<tag>` manually.\n",
        );
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingPlugin {
    pub role: String,
    pub fix_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoInstalledPlugin {
    pub role: String,
    pub repo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstalledPluginSummary {
    pub name: String,
    pub plugin_kind: String,
    pub subject_kinds: Vec<String>,
}

impl InstalledPluginSummary {
    pub fn is_provider(&self) -> bool {
        self.plugin_kind == PLUGIN_KIND_PROVIDER
    }

    pub fn is_subject_backend(&self) -> bool {
        self.plugin_kind == PLUGIN_KIND_SUBJECT_BACKEND
    }

    pub fn covers_subject_kind(&self, kind: &str) -> bool {
        self.is_subject_backend() && self.subject_kinds.iter().any(|k| k == kind)
    }
}

pub fn summarize_discovered_plugins(
    plugins: &[orchestrator_plugin_host::DiscoveredPlugin],
) -> Vec<InstalledPluginSummary> {
    plugins
        .iter()
        .map(|plugin| {
            let subject_kinds = plugin
                .manifest
                .capabilities
                .iter()
                .filter_map(|cap| cap.strip_prefix("subject_kind:").map(|rest| rest.trim().to_string()))
                .filter(|k| !k.is_empty())
                .collect::<Vec<_>>();
            InstalledPluginSummary {
                name: plugin.name.clone(),
                plugin_kind: plugin.manifest.plugin_kind.clone(),
                subject_kinds,
            }
        })
        .collect()
}
