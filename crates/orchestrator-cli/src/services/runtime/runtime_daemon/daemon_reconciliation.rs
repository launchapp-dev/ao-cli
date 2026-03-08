use super::*;
use orchestrator_core::{
    project_task_blocked_with_reason, services::ServiceHub, WorkflowMachineState, WorkflowStatus,
};

pub async fn recover_orphaned_running_workflows_on_startup(
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
) -> usize {
    let workflows = match hub.workflows().list().await {
        Ok(w) => w,
        Err(_) => return 0,
    };

    let mut recovered = 0usize;
    for workflow in workflows {
        if workflow.status != WorkflowStatus::Running {
            continue;
        }
        if workflow.machine_state == WorkflowMachineState::MergeConflict {
            continue;
        }

        eprintln!(
            "{}: startup orphan detection — cancelling orphaned workflow {} (task {})",
            protocol::ACTOR_DAEMON,
            workflow.id,
            workflow.task_id
        );
        let task_id = workflow.task_id.clone();
        if let Ok(_updated) = hub.workflows().cancel(&workflow.id).await {
            sync_task_status_for_workflow_result(
                hub.clone(),
                project_root,
                &task_id,
                WorkflowStatus::Cancelled,
                Some(workflow.id.as_str()),
            )
            .await;
        }
        let task = match hub.tasks().get(&task_id).await {
            Ok(task) => task,
            Err(_) => {
                recovered = recovered.saturating_add(1);
                continue;
            }
        };
        let _ = project_task_blocked_with_reason(
            hub.clone(),
            &task,
            "orphaned_after_daemon_restart".to_string(),
            Some(workflow.id.clone()),
        )
        .await;
        recovered = recovered.saturating_add(1);
    }

    recovered
}
