use std::sync::Arc;

use anyhow::Result;
use orchestrator_core::{services::ServiceHub, TaskStatus, UI_UX_PIPELINE_ID};

use crate::{
    active_workflow_task_ids, dependency_blocked_reason, dependency_gate_issues_for_task,
    set_task_blocked_with_reason, should_skip_dispatch, ProcessManager, ReadyTaskWorkflowStart,
    ReadyTaskWorkflowStartSummary, SubjectDispatch, TaskSelectionSource,
};

fn pipeline_for_task(task: &orchestrator_core::OrchestratorTask) -> String {
    if task.is_frontend_related() {
        UI_UX_PIPELINE_ID.to_string()
    } else {
        orchestrator_core::STANDARD_PIPELINE_ID.to_string()
    }
}

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
            let _ = set_task_blocked_with_reason(hub.clone(), &task, reason, None).await;
            continue;
        }

        let pipeline_id = pipeline_for_task(&task);
        let dispatch = SubjectDispatch::for_task(task.id.clone(), pipeline_id);
        match process_manager.spawn_workflow_runner(&dispatch, root) {
            Ok(_) => {
                let _ = hub
                    .tasks()
                    .set_status(&task.id, TaskStatus::InProgress, false)
                    .await;
                started_workflows.push(ReadyTaskWorkflowStart {
                    task_id: task.id.clone(),
                    workflow_id: task.id.clone(),
                    selection_source: TaskSelectionSource::FallbackPicker,
                });
            }
            Err(error) => {
                let reason = format!("failed to start workflow runner: {error}");
                let _ = set_task_blocked_with_reason(hub.clone(), &task, reason, None).await;
            }
        }
    }

    Ok(ReadyTaskWorkflowStartSummary {
        started: started_workflows.len(),
        started_workflows,
    })
}
