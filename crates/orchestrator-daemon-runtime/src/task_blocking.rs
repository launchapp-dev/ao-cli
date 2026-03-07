use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use orchestrator_core::{services::ServiceHub, DependencyType, OrchestratorTask, TaskStatus};
use orchestrator_providers::{BuiltinGitProvider, GitProvider};

pub const DEPENDENCY_GATE_PREFIX: &str = "dependency gate:";
pub const MERGE_GATE_PREFIX: &str = "merge gate:";

pub fn dependency_blocked_reason(issues: &[String]) -> String {
    format!("{DEPENDENCY_GATE_PREFIX} {}", issues.join("; "))
}

pub fn merge_blocked_reason(branch_name: &str) -> String {
    format!("{MERGE_GATE_PREFIX} branch `{branch_name}` is not merged into default branch")
}

pub fn is_dependency_gate_block(task: &OrchestratorTask) -> bool {
    task.blocked_reason
        .as_deref()
        .map(|reason| reason.starts_with(DEPENDENCY_GATE_PREFIX))
        .unwrap_or(false)
}

pub fn is_merge_gate_block(task: &OrchestratorTask) -> bool {
    task.blocked_reason
        .as_deref()
        .map(|reason| reason.starts_with(MERGE_GATE_PREFIX))
        .unwrap_or(false)
}

pub async fn set_task_blocked_with_reason(
    hub: Arc<dyn ServiceHub>,
    task: &OrchestratorTask,
    reason: String,
    blocked_by: Option<String>,
) -> Result<()> {
    let mut updated = task.clone();
    updated.status = TaskStatus::Blocked;
    updated.paused = true;
    updated.blocked_reason = Some(reason);
    updated.blocked_at = Some(Utc::now());
    updated.blocked_phase = None;
    updated.blocked_by = blocked_by;
    updated.metadata.updated_at = Utc::now();
    updated.metadata.updated_by = protocol::ACTOR_DAEMON.to_string();
    updated.metadata.version = updated.metadata.version.saturating_add(1);
    hub.tasks().replace(updated).await?;
    Ok(())
}

pub async fn dependency_gate_issues_for_task(
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    task: &OrchestratorTask,
) -> Vec<String> {
    let mut issues = Vec::new();

    for dependency in &task.dependencies {
        if dependency.dependency_type != DependencyType::BlockedBy {
            continue;
        }

        let dependency_task = match hub.tasks().get(&dependency.task_id).await {
            Ok(task) => task,
            Err(_) => {
                issues.push(format!("dependency {} does not exist", dependency.task_id));
                continue;
            }
        };

        if dependency_task.status != TaskStatus::Done {
            issues.push(format!(
                "dependency {} is {}",
                dependency.task_id, dependency_task.status
            ));
            continue;
        }

        if let Some(branch_name) = dependency_task
            .branch_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            match BuiltinGitProvider::new(project_root)
                .is_branch_merged(project_root, branch_name)
                .await
            {
                Ok(Some(true)) | Ok(None) => {}
                Ok(Some(false)) => {
                    issues.push(format!(
                        "dependency {} branch `{}` is not merged",
                        dependency.task_id, branch_name
                    ));
                }
                Err(error) => {
                    issues.push(format!(
                        "unable to verify dependency {} merge status: {}",
                        dependency.task_id, error
                    ));
                }
            }
        }
    }

    issues
}
