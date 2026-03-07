use std::sync::Arc;

use orchestrator_core::{services::ServiceHub, TaskStatus};
use orchestrator_daemon_runtime::{build_completion_reconciliation_plan, TaskCompletionAction};

use super::{
    remove_terminal_em_work_queue_entry_non_fatal, schedule_dispatch::ScheduleDispatch,
    set_task_blocked_with_reason,
};
use crate::services::runtime::runtime_daemon::daemon_process_manager::CompletedProcess;

pub(super) struct CompletionReconciler;

impl CompletionReconciler {
    pub(super) async fn reconcile(
        hub: Arc<dyn ServiceHub>,
        root: &str,
        completed_processes: Vec<CompletedProcess>,
    ) -> (usize, usize) {
        let plan = build_completion_reconciliation_plan(completed_processes);

        for disposition in plan.dispositions {
            for event in &disposition.runner_events {
                eprintln!(
                    "{}: runner event: {} subject={} pipeline={:?} exit={:?}",
                    protocol::ACTOR_DAEMON,
                    event.event,
                    disposition.subject_id,
                    event.pipeline,
                    event.exit_code,
                );
            }

            if let Some(task_action) = disposition.task_action {
                match task_action {
                    TaskCompletionAction::MarkDone { task_id } => {
                        remove_terminal_em_work_queue_entry_non_fatal(root, &task_id, None);
                        let _ = hub
                            .tasks()
                            .set_status(&task_id, TaskStatus::Done, false)
                            .await;
                    }
                    TaskCompletionAction::MarkBlocked { task_id, reason } => {
                        if let Ok(task) = hub.tasks().get(&task_id).await {
                            let _ = set_task_blocked_with_reason(hub.clone(), &task, reason, None)
                                .await;
                        } else {
                            let _ = hub
                                .tasks()
                                .set_status(&task_id, TaskStatus::Blocked, false)
                                .await;
                        }
                    }
                }
            } else {
                eprintln!(
                    "{}: workflow runner {} for subject '{}' (exit={:?})",
                    protocol::ACTOR_DAEMON,
                    if disposition.success {
                        "succeeded"
                    } else {
                        "failed"
                    },
                    disposition.subject_id,
                    disposition.exit_code,
                );
            }

            if let Some(schedule_update) = disposition.schedule_update {
                ScheduleDispatch::update_completion_state(
                    root,
                    &schedule_update.schedule_id,
                    &schedule_update.status,
                );
            }
        }

        (plan.executed_workflow_phases, plan.failed_workflow_phases)
    }
}
