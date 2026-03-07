use std::sync::Arc;

use orchestrator_core::{services::ServiceHub, TaskStatus};

use crate::{
    build_completion_reconciliation_plan, remove_terminal_em_work_queue_entry_non_fatal,
    set_task_blocked_with_reason, CompletedProcess, ScheduleDispatch,
};

pub async fn reconcile_completed_processes(
    hub: Arc<dyn ServiceHub>,
    root: &str,
    completed_processes: Vec<CompletedProcess>,
) -> (usize, usize) {
    let plan = build_completion_reconciliation_plan(completed_processes);

    for fact in plan.execution_facts {
        for event in &fact.runner_events {
            eprintln!(
                "{}: runner event: {} subject={} pipeline={:?} exit={:?}",
                protocol::ACTOR_DAEMON,
                event.event,
                fact.subject_id,
                event.pipeline,
                event.exit_code,
            );
        }

        if let Some(task_id) = fact.task_id.clone() {
            if fact.success {
                remove_terminal_em_work_queue_entry_non_fatal(root, &task_id, None);
                let _ = hub
                    .tasks()
                    .set_status(&task_id, TaskStatus::Done, false)
                    .await;
            } else if let Some(reason) = fact.failure_reason.clone() {
                if let Ok(task) = hub.tasks().get(&task_id).await {
                    let _ = set_task_blocked_with_reason(hub.clone(), &task, reason, None).await;
                } else {
                    let _ = hub
                        .tasks()
                        .set_status(&task_id, TaskStatus::Blocked, false)
                        .await;
                }
            } else {
                let _ = hub
                    .tasks()
                    .set_status(&task_id, TaskStatus::Blocked, false)
                    .await;
            }
        } else {
            eprintln!(
                "{}: workflow runner {} for subject '{}' (exit={:?})",
                protocol::ACTOR_DAEMON,
                if fact.success { "succeeded" } else { "failed" },
                fact.subject_id,
                fact.exit_code,
            );
        }

        if let Some(schedule_id) = fact.schedule_id.as_deref() {
            ScheduleDispatch::update_completion_state(root, schedule_id, fact.completion_status());
        }
    }

    (plan.executed_workflow_phases, plan.failed_workflow_phases)
}
