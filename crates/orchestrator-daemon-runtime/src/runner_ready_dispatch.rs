use std::sync::Arc;

use anyhow::Result;
use orchestrator_core::{services::ServiceHub, TaskStatus};

use crate::{
    active_workflow_task_ids, dependency_blocked_reason, dependency_gate_issues_for_task,
    pipeline_for_task, project_task_blocked_with_reason, project_task_status, should_skip_dispatch,
    ProcessManager, ReadyTaskWorkflowStart, ReadyTaskWorkflowStartSummary, SubjectDispatch,
    TaskSelectionSource,
};

pub async fn dispatch_ready_tasks_via_runner(
    hub: Arc<dyn ServiceHub>,
    root: &str,
    process_manager: &mut ProcessManager,
    limit: usize,
) -> Result<ReadyTaskWorkflowStartSummary> {
    let workflows = hub.workflows().list().await.unwrap_or_default();
    let active_task_ids = active_workflow_task_ids(&workflows);
    let candidates = hub.tasks().list_prioritized().await?;
    let mut started_workflows = Vec::new();

    for task in candidates {
        if started_workflows.len() >= limit {
            break;
        }

        if task.paused || task.cancelled {
            continue;
        }
        if task.status != TaskStatus::Ready {
            continue;
        }
        if active_task_ids.contains(&task.id) {
            continue;
        }
        if should_skip_dispatch(&task) {
            continue;
        }

        let dependency_issues = dependency_gate_issues_for_task(hub.clone(), root, &task).await;
        if !dependency_issues.is_empty() {
            let reason = dependency_blocked_reason(&dependency_issues);
            let _ = project_task_blocked_with_reason(hub.clone(), &task, reason, None).await;
            continue;
        }

        let pipeline_id = pipeline_for_task(&task);
        let dispatch = SubjectDispatch::for_task(task.id.clone(), pipeline_id);
        match process_manager.spawn_workflow_runner(&dispatch, root) {
            Ok(_) => {
                let _ = project_task_status(hub.clone(), &task.id, TaskStatus::InProgress).await;
                started_workflows.push(ReadyTaskWorkflowStart {
                    task_id: task.id.clone(),
                    workflow_id: task.id.clone(),
                    selection_source: TaskSelectionSource::FallbackPicker,
                });
            }
            Err(error) => {
                let reason = format!("failed to start workflow runner: {error}");
                let _ = project_task_blocked_with_reason(hub.clone(), &task, reason, None).await;
            }
        }
    }

    Ok(ReadyTaskWorkflowStartSummary {
        started: started_workflows.len(),
        started_workflows,
    })
}
