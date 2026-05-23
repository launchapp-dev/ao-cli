use std::path::Path;

use anyhow::Result;
use orchestrator_core::{
    summarize_discovered_plugins, InstalledPluginSummary, PluginInstaller, PluginPreflightRunner, PluginPreflightSpec,
    PreflightResult,
};
use orchestrator_plugin_host::discover_plugins;

use crate::DaemonRunEvent;
use crate::DaemonRunHooks;

pub struct PreflightOutcome {
    pub result: PreflightResult,
    pub skipped: bool,
    pub auto_install: bool,
}

impl PreflightOutcome {
    pub fn should_abort_startup(&self) -> bool {
        !self.skipped && !self.result.is_ok()
    }
}

pub async fn run_plugin_preflight<H: DaemonRunHooks>(
    project_root: &str,
    primary_root: &str,
    spec: PluginPreflightSpec,
    installer: Option<&dyn PluginInstaller>,
    runtime_skip: bool,
    hooks: &mut H,
) -> Result<PreflightOutcome> {
    let auto_install = spec.auto_install;
    if runtime_skip {
        let _ = hooks.handle_event(DaemonRunEvent::PluginPreflight {
            project_root: primary_root.to_string(),
            satisfied: Vec::new(),
            auto_installed: Vec::new(),
            missing: Vec::new(),
            skipped: true,
            auto_install,
        });
        return Ok(PreflightOutcome { result: PreflightResult::default(), skipped: true, auto_install });
    }

    let installed = discover_installed_plugins(project_root).unwrap_or_default();
    let result = PluginPreflightRunner::run(&spec, installed, installer).await?;

    let _ = hooks.handle_event(DaemonRunEvent::PluginPreflight {
        project_root: primary_root.to_string(),
        satisfied: result.satisfied.clone(),
        auto_installed: result.auto_installed.iter().map(|a| format!("{}={}", a.role, a.repo)).collect(),
        missing: result.missing.iter().map(|m| m.role.clone()).collect(),
        skipped: false,
        auto_install,
    });

    Ok(PreflightOutcome { result, skipped: false, auto_install })
}

pub fn discover_installed_plugins(project_root: &str) -> Result<Vec<InstalledPluginSummary>> {
    let plugins = discover_plugins(Path::new(project_root))?;
    Ok(summarize_discovered_plugins(&plugins))
}
