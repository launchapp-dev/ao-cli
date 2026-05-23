use anyhow::Result;
use async_trait::async_trait;

use super::{
    AutoInstalledPlugin, InstalledPluginSummary, MissingPlugin, PluginPreflightSpec, PreflightResult, RequiredRole,
};

#[async_trait(?Send)]
pub trait PluginInstaller {
    async fn install(&self, repo_spec: &str) -> Result<String>;
    async fn rediscover(&self) -> Result<Vec<InstalledPluginSummary>>;
}

pub struct PluginPreflightRunner;

impl PluginPreflightRunner {
    pub async fn run(
        spec: &PluginPreflightSpec,
        installed: Vec<InstalledPluginSummary>,
        installer: Option<&dyn PluginInstaller>,
    ) -> Result<PreflightResult> {
        let mut current = installed;
        let mut auto_installed: Vec<AutoInstalledPlugin> = Vec::new();
        let mut satisfied: Vec<String> = Vec::new();
        let mut missing: Vec<MissingPlugin> = Vec::new();

        for role in &spec.required_roles {
            if role_is_satisfied(role, &current) {
                satisfied.push(role.label());
                continue;
            }

            let role_label = role.label();
            let auto_target = spec.install_target_for(&role_label).map(str::to_string);

            if spec.auto_install {
                if let (Some(repo_spec), Some(installer)) = (auto_target.as_deref(), installer) {
                    installer.install(repo_spec).await?;
                    current = installer.rediscover().await?;
                    if role_is_satisfied(role, &current) {
                        satisfied.push(role_label.clone());
                        auto_installed.push(AutoInstalledPlugin { role: role_label, repo: repo_spec.to_string() });
                        continue;
                    }
                    missing.push(MissingPlugin {
                        role: role_label,
                        fix_command: format!(
                            "animus plugin install {repo_spec}  # auto-install ran but role still unsatisfied",
                        ),
                    });
                    continue;
                }
            }

            missing.push(MissingPlugin {
                role: role_label.clone(),
                fix_command: fix_command_for(role, auto_target.as_deref()),
            });
        }

        Ok(PreflightResult { satisfied, missing, auto_installed })
    }
}

fn role_is_satisfied(role: &RequiredRole, installed: &[InstalledPluginSummary]) -> bool {
    match role {
        RequiredRole::AtLeastOneProvider => installed.iter().any(|p| p.is_provider()),
        RequiredRole::SubjectKind(kind) => installed.iter().any(|p| p.covers_subject_kind(kind)),
        RequiredRole::TransportEnabled => true,
    }
}

fn fix_command_for(role: &RequiredRole, default_repo: Option<&str>) -> String {
    let target = default_repo.unwrap_or("<owner>/<repo>@<tag>");
    match role {
        RequiredRole::AtLeastOneProvider => format!("animus plugin install {target}  # any provider plugin"),
        RequiredRole::SubjectKind(kind) => {
            format!("animus plugin install {target}  # must claim subject_kind:{kind}")
        }
        RequiredRole::TransportEnabled => format!("animus plugin install {target}  # transport backend"),
    }
}
