use std::path::Path;

use anyhow::Result;
use orchestrator_core::{
    load_daemon_project_config, write_daemon_project_config, DoctorCheckResult, DoctorCheckStatus, DoctorReport,
    FileServiceHub,
};
use serde::Serialize;

use crate::{print_value, DoctorArgs};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DoctorFixAction {
    pub id: String,
    pub status: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct RepositoryReadinessState {
    status: String,
    healthy: bool,
    blocked_count: usize,
    remediatable_count: usize,
    next_steps: Vec<String>,
}

pub(crate) async fn handle_doctor(project_root: &str, args: DoctorArgs, json: bool) -> Result<()> {
    let before = DoctorReport::run_for_project(Path::new(project_root));
    let readiness_before = build_readiness_state(&before);

    if !args.fix {
        return print_value(
            serde_json::json!({
                "doctor": before,
                "readiness": readiness_before,
                "fix": {
                    "requested": false,
                    "applied": false,
                    "actions": [],
                }
            }),
            json,
        );
    }

    let actions = apply_doctor_fixes(project_root, &before);
    let applied = actions.iter().any(|action| action.status == "applied");
    let after = DoctorReport::run_for_project(Path::new(project_root));
    let readiness_after = build_readiness_state(&after);

    print_value(
        serde_json::json!({
            "doctor_before": before,
            "readiness_before": readiness_before,
            "fix": {
                "requested": true,
                "applied": applied,
                "actions": actions,
            },
            "doctor_after": after,
            "readiness_after": readiness_after,
        }),
        json,
    )
}

pub(crate) fn apply_doctor_fixes(project_root: &str, report: &DoctorReport) -> Vec<DoctorFixAction> {
    let mut actions = Vec::new();
    let project_root_path = Path::new(project_root);

    // Skip bootstrap remediation if .ao path is blocked (not a directory)
    let ao_path = project_root_path.join(".ao");
    let ao_blocked = ao_path.exists() && !ao_path.is_dir();

    if ao_blocked {
        actions.push(skipped_action(
            "bootstrap_project_state",
            ".ao path is blocked (not a directory) - manual repair required",
        ));
    } else if remediation_needed(report, "bootstrap_project_state") {
        match FileServiceHub::new(project_root_path) {
            Ok(_) => actions.push(applied_action(
                "bootstrap_project_state",
                "created/validated baseline AO state and config files",
            )),
            Err(error) => actions.push(failed_action("bootstrap_project_state", error.to_string())),
        }
    } else {
        actions.push(skipped_action("bootstrap_project_state", "project bootstrap checks already passed"));
    }

    if remediation_needed(report, "create_default_daemon_config") {
        let result = load_daemon_project_config(project_root_path)
            .and_then(|config| write_daemon_project_config(project_root_path, &config));
        match result {
            Ok(_) => actions
                .push(applied_action("create_default_daemon_config", "created daemon config with default values")),
            Err(error) => actions.push(failed_action("create_default_daemon_config", error.to_string())),
        }
    } else {
        actions.push(skipped_action("create_default_daemon_config", "daemon config remediation not required"));
    }

    actions
}

fn remediation_needed(report: &DoctorReport, remediation_id: &str) -> bool {
    report.checks.iter().any(|check| {
        check.remediation.id == remediation_id && check.remediation.available && check.status != DoctorCheckStatus::Ok
    })
}

fn applied_action(id: &str, details: &str) -> DoctorFixAction {
    DoctorFixAction { id: id.to_string(), status: "applied".to_string(), details: details.to_string() }
}

fn skipped_action(id: &str, details: &str) -> DoctorFixAction {
    DoctorFixAction { id: id.to_string(), status: "skipped".to_string(), details: details.to_string() }
}

fn failed_action(id: &str, details: String) -> DoctorFixAction {
    DoctorFixAction { id: id.to_string(), status: "failed".to_string(), details }
}

fn build_readiness_state(report: &DoctorReport) -> RepositoryReadinessState {
    let blocked_count = report.checks.iter().filter(|c| c.status == DoctorCheckStatus::Fail).count();
    let remediatable_count = report
        .checks
        .iter()
        .filter(|c| c.status == DoctorCheckStatus::Warn && c.remediation.available)
        .count();

    let status = match report.result {
        DoctorCheckResult::Healthy => "healthy".to_string(),
        DoctorCheckResult::Degraded => "degraded".to_string(),
        DoctorCheckResult::Unhealthy => "unhealthy".to_string(),
    };

    let healthy = report.result == DoctorCheckResult::Healthy;

    let mut next_steps = Vec::new();
    if !healthy {
        if blocked_count > 0 {
            next_steps.push("resolve critical issues blocking repo activation".to_string());
        }
        if remediatable_count > 0 {
            next_steps.push("run 'ao doctor --fix' to apply available remediations".to_string());
        }
    }

    RepositoryReadinessState {
        status,
        healthy,
        blocked_count,
        remediatable_count,
        next_steps,
    }
}

#[cfg(test)]
mod tests {
    use protocol::test_utils::EnvVarGuard;

    use super::*;

    #[test]
    fn doctor_fix_creates_default_daemon_config_when_missing() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let _lock = crate::shared::test_env_lock().lock().unwrap_or_else(|p| p.into_inner());
        let _home = EnvVarGuard::set("HOME", Some(temp.path().to_string_lossy().as_ref()));
        let report = DoctorReport::run_for_project(temp.path());
        let actions = apply_doctor_fixes(temp.path().to_string_lossy().as_ref(), &report);
        assert!(actions.iter().any(|action| action.id == "create_default_daemon_config" && action.status == "applied"));
        assert!(orchestrator_core::daemon_project_config_path(temp.path()).exists());
    }
}
