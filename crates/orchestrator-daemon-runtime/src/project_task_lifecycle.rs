use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use orchestrator_core::{services::ServiceHub, OrchestratorTask, TaskStatus};

pub async fn project_task_status(
    hub: Arc<dyn ServiceHub>,
    task_id: &str,
    status: TaskStatus,
) -> Result<()> {
    hub.tasks().set_status(task_id, status, false).await?;
    Ok(())
}

pub async fn project_task_blocked_with_reason(
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

pub async fn project_task_dispatch_failure(
    hub: Arc<dyn ServiceHub>,
    task_id: &str,
    max_dispatch_retries: u32,
) -> Result<()> {
    let task = match hub.tasks().get(task_id).await {
        Ok(task) => task,
        Err(_) => {
            return project_task_status(hub, task_id, TaskStatus::Blocked).await;
        }
    };

    let count = task
        .consecutive_dispatch_failures
        .unwrap_or(0)
        .saturating_add(1);

    if count >= max_dispatch_retries {
        let reason = format!("auto-blocked after {} consecutive dispatch failures", count);
        return project_task_blocked_with_reason(hub, &task, reason, None).await;
    }

    let mut updated = task;
    updated.consecutive_dispatch_failures = Some(count);
    updated.last_dispatch_failure_at = Some(Utc::now().to_rfc3339());
    hub.tasks().replace(updated).await?;
    project_task_status(hub, task_id, TaskStatus::Blocked).await
}
