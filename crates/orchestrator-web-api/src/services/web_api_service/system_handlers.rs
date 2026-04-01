use orchestrator_core::{DaemonStatus, DoctorCheckResult, DoctorCheckStatus, DoctorReport};
use serde_json::{json, Value};
use std::path::Path;

use super::{parsing::enum_as_string, WebApiError, WebApiService};

impl WebApiService {
    pub async fn system_info(&self) -> Result<Value, WebApiError> {
        let status = self.context.hub.daemon().status().await?;
        let daemon_running = matches!(
            status,
            DaemonStatus::Starting | DaemonStatus::Running | DaemonStatus::Paused | DaemonStatus::Stopping
        );
        let daemon_status = enum_as_string(&status)?;

        Ok(json!({
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "version": self.context.app_version,
            "daemon_running": daemon_running,
            "daemon_status": daemon_status,
            "project_root": self.context.project_root,
        }))
    }

    pub async fn repository_readiness(&self) -> Result<Value, WebApiError> {
        let project_root = Path::new(&self.context.project_root);
        let report = DoctorReport::run_for_project(project_root);

        let blocked_count = report.checks.iter().filter(|c| c.status == DoctorCheckStatus::Fail).count();
        let remediatable_count = report
            .checks
            .iter()
            .filter(|c| c.status == DoctorCheckStatus::Warn && c.remediation.available)
            .count();
        let ok_count = report.checks.iter().filter(|c| c.status == DoctorCheckStatus::Ok).count();

        let status = match report.result {
            DoctorCheckResult::Healthy => "healthy",
            DoctorCheckResult::Degraded => "degraded",
            DoctorCheckResult::Unhealthy => "unhealthy",
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
        } else {
            next_steps.push("repository is ready for agent orchestration".to_string());
        }

        Ok(json!({
            "status": status,
            "healthy": healthy,
            "checks_ok": ok_count,
            "checks_warn": report.checks.iter().filter(|c| c.status == DoctorCheckStatus::Warn).count(),
            "checks_fail": blocked_count,
            "blocked_count": blocked_count,
            "remediatable_count": remediatable_count,
            "next_steps": next_steps,
            "checks": report.checks,
        }))
    }
}
