use super::runner::{PluginInstaller, PluginPreflightRunner};
use super::{InstalledPluginSummary, PluginPreflightSpec, RequiredRole};
use anyhow::Result;
use async_trait::async_trait;
use std::cell::RefCell;

struct FakeInstaller {
    install_calls: RefCell<Vec<String>>,
    next_after_install: RefCell<Vec<InstalledPluginSummary>>,
}

impl FakeInstaller {
    fn new(initial_after_install: Vec<InstalledPluginSummary>) -> Self {
        Self { install_calls: RefCell::new(Vec::new()), next_after_install: RefCell::new(initial_after_install) }
    }
}

#[async_trait(?Send)]
impl PluginInstaller for FakeInstaller {
    async fn install(&self, repo_spec: &str) -> Result<String> {
        self.install_calls.borrow_mut().push(repo_spec.to_string());
        Ok(repo_spec.to_string())
    }

    async fn rediscover(&self) -> Result<Vec<InstalledPluginSummary>> {
        Ok(self.next_after_install.borrow().clone())
    }
}

fn provider_plugin(name: &str) -> InstalledPluginSummary {
    InstalledPluginSummary { name: name.to_string(), plugin_kind: "provider".to_string(), subject_kinds: Vec::new() }
}

fn subject_plugin(name: &str, kinds: &[&str]) -> InstalledPluginSummary {
    InstalledPluginSummary {
        name: name.to_string(),
        plugin_kind: "subject_backend".to_string(),
        subject_kinds: kinds.iter().map(|k| (*k).to_string()).collect(),
    }
}

#[tokio::test]
async fn preflight_with_no_plugins_and_no_auto_install_reports_missing_with_fix_commands() {
    let spec = PluginPreflightSpec::daemon_default();
    let result = PluginPreflightRunner::run(&spec, Vec::new(), None).await.expect("preflight run");

    assert!(!result.is_ok(), "preflight should fail when no plugins installed");
    assert_eq!(result.missing.len(), 3, "all three roles should be missing");

    let provider_missing = result.missing.iter().find(|m| m.role == "at_least_one_provider").expect("provider role");
    assert!(provider_missing.fix_command.contains("animus plugin install"));
    assert!(provider_missing.fix_command.contains("animus-provider-claude"));

    let task_missing = result.missing.iter().find(|m| m.role == "subject_kind:task").expect("task subject role");
    assert!(task_missing.fix_command.contains("subject_kind:task"));

    let message = result.render_missing_message();
    assert!(message.contains("plugin preflight failed"));
    assert!(message.contains("at_least_one_provider"));
    assert!(message.contains("--auto-install"));
}

#[tokio::test]
async fn preflight_with_provider_installed_marks_provider_role_satisfied() {
    let spec = PluginPreflightSpec::daemon_default();
    let installed = vec![provider_plugin("animus-provider-claude")];
    let result = PluginPreflightRunner::run(&spec, installed, None).await.expect("preflight run");

    assert!(result.satisfied.contains(&"at_least_one_provider".to_string()));
    assert!(!result.is_ok(), "subject backends still missing");
    let missing_labels: Vec<&str> = result.missing.iter().map(|m| m.role.as_str()).collect();
    assert!(missing_labels.contains(&"subject_kind:task"));
    assert!(missing_labels.contains(&"subject_kind:requirement"));
}

#[tokio::test]
async fn preflight_with_auto_install_installs_missing_plugin_and_marks_satisfied() {
    let spec = PluginPreflightSpec {
        required_roles: vec![RequiredRole::AtLeastOneProvider],
        auto_install: true,
        auto_install_defaults: vec![(
            "at_least_one_provider".to_string(),
            "launchapp-dev/animus-provider-claude@v0.1.0".to_string(),
        )],
    };
    let installer = FakeInstaller::new(vec![provider_plugin("animus-provider-claude")]);

    let result = PluginPreflightRunner::run(&spec, Vec::new(), Some(&installer)).await.expect("preflight run");

    assert!(result.is_ok(), "auto-install should resolve missing role");
    assert_eq!(installer.install_calls.borrow().len(), 1);
    assert_eq!(installer.install_calls.borrow()[0], "launchapp-dev/animus-provider-claude@v0.1.0");
    assert_eq!(result.auto_installed.len(), 1);
    assert_eq!(result.auto_installed[0].role, "at_least_one_provider");
}

#[tokio::test]
async fn preflight_with_auto_install_but_install_still_does_not_cover_role_reports_missing() {
    let spec = PluginPreflightSpec {
        required_roles: vec![RequiredRole::SubjectKind("task".to_string())],
        auto_install: true,
        auto_install_defaults: vec![(
            "subject_kind:task".to_string(),
            "launchapp-dev/animus-subject-broken@v0.1.0".to_string(),
        )],
    };
    let installer = FakeInstaller::new(vec![subject_plugin("animus-subject-broken", &["unrelated"])]);

    let result = PluginPreflightRunner::run(&spec, Vec::new(), Some(&installer)).await.expect("preflight run");

    assert!(!result.is_ok(), "preflight should still fail when installed plugin doesn't claim the kind");
    assert_eq!(result.missing.len(), 1);
    assert!(result.missing[0].fix_command.contains("auto-install ran"));
}

#[tokio::test]
async fn preflight_satisfied_when_subject_backend_covers_all_required_kinds() {
    let spec = PluginPreflightSpec::daemon_default();
    let installed = vec![
        provider_plugin("animus-provider-claude"),
        subject_plugin("animus-subject-native", &["task", "requirement"]),
    ];
    let result = PluginPreflightRunner::run(&spec, installed, None).await.expect("preflight run");

    assert!(result.is_ok(), "all roles satisfied");
    assert_eq!(result.missing.len(), 0);
    assert_eq!(result.satisfied.len(), 3);
}

#[test]
fn install_target_for_resolves_role_labels_to_repo_specs() {
    let spec = PluginPreflightSpec::daemon_default();
    assert_eq!(spec.install_target_for("at_least_one_provider"), Some("launchapp-dev/animus-provider-claude@v0.1.0"));
    assert!(spec.install_target_for("subject_kind:task").is_some());
    assert_eq!(spec.install_target_for("nonexistent_role"), None);
}
