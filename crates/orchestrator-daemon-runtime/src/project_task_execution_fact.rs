use std::sync::Arc;

use orchestrator_core::{services::ServiceHub, TaskStatus};

use crate::{
    remove_terminal_em_work_queue_entry_non_fatal, set_task_blocked_with_reason,
    SubjectExecutionFact,
};

pub(crate) async fn project_task_execution_fact(
    hub: Arc<dyn ServiceHub>,
    root: &str,
    fact: &SubjectExecutionFact,
) {
    let Some(task_id) = fact.task_id.as_deref() else {
        return;
    };

    if fact.success {
        remove_terminal_em_work_queue_entry_non_fatal(root, task_id, None);
        let _ = hub
            .tasks()
            .set_status(task_id, TaskStatus::Done, false)
            .await;
        return;
    }

    if let Some(reason) = fact.failure_reason.clone() {
        if let Ok(task) = hub.tasks().get(task_id).await {
            let _ = set_task_blocked_with_reason(hub, &task, reason, None).await;
            return;
        }
    }

    let _ = hub
        .tasks()
        .set_status(task_id, TaskStatus::Blocked, false)
        .await;
}
