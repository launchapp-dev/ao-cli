use super::*;
use orchestrator_core::{services::ServiceHub, WorkflowMachineState, WorkflowStatus};
use orchestrator_daemon_runtime::remove_terminal_dispatch_queue_entry_non_fatal;

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
        if let Ok(_updated) = hub.workflows().cancel(&workflow.id).await {
            remove_terminal_dispatch_queue_entry_non_fatal(
                project_root,
                workflow.subject.id(),
                workflow.workflow_ref.as_deref(),
                Some(workflow.id.as_str()),
            );
        }
        recovered = recovered.saturating_add(1);
    }

    recovered
}
