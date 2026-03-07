use crate::{CompletedProcess, RunnerEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskCompletionAction {
    MarkDone { task_id: String },
    MarkBlocked { task_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleCompletionUpdate {
    pub schedule_id: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct CompletedProcessDisposition {
    pub subject_id: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub runner_events: Vec<RunnerEvent>,
    pub task_action: Option<TaskCompletionAction>,
    pub schedule_update: Option<ScheduleCompletionUpdate>,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionReconciliationPlan {
    pub executed_workflow_phases: usize,
    pub failed_workflow_phases: usize,
    pub dispositions: Vec<CompletedProcessDisposition>,
}

pub fn build_completion_reconciliation_plan(
    completed_processes: Vec<CompletedProcess>,
) -> CompletionReconciliationPlan {
    let mut plan = CompletionReconciliationPlan::default();

    for completed in completed_processes {
        let task_action = completed.task_id.as_ref().map(|task_id| {
            if completed.success {
                TaskCompletionAction::MarkDone {
                    task_id: task_id.clone(),
                }
            } else {
                TaskCompletionAction::MarkBlocked {
                    task_id: task_id.clone(),
                    reason: format!("workflow runner failed: {}", completion_reason(&completed)),
                }
            }
        });

        let schedule_update =
            completed
                .schedule_id
                .as_ref()
                .map(|schedule_id| ScheduleCompletionUpdate {
                    schedule_id: schedule_id.clone(),
                    status: if completed.success {
                        "completed".to_string()
                    } else {
                        "failed".to_string()
                    },
                });

        if completed.success {
            plan.executed_workflow_phases = plan.executed_workflow_phases.saturating_add(1);
        } else {
            plan.failed_workflow_phases = plan.failed_workflow_phases.saturating_add(1);
        }

        plan.dispositions.push(CompletedProcessDisposition {
            subject_id: completed.subject_id,
            exit_code: completed.exit_code,
            success: completed.success,
            runner_events: completed.events,
            task_action,
            schedule_update,
        });
    }

    plan
}

fn completion_reason(completed: &CompletedProcess) -> String {
    completed.failure_reason.clone().unwrap_or_else(|| {
        format!(
            "workflow runner exited with status {:?}",
            completed.exit_code
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CompletedProcess;

    #[test]
    fn builds_task_success_plan() {
        let plan = build_completion_reconciliation_plan(vec![CompletedProcess {
            subject_id: "TASK-123".to_string(),
            task_id: Some("TASK-123".to_string()),
            schedule_id: None,
            exit_code: Some(0),
            success: true,
            failure_reason: None,
            events: Vec::new(),
        }]);

        assert_eq!(plan.executed_workflow_phases, 1);
        assert_eq!(plan.failed_workflow_phases, 0);
        assert_eq!(plan.dispositions.len(), 1);
        assert_eq!(
            plan.dispositions[0].task_action,
            Some(TaskCompletionAction::MarkDone {
                task_id: "TASK-123".to_string()
            })
        );
        assert!(plan.dispositions[0].schedule_update.is_none());
    }

    #[test]
    fn builds_failure_plan_with_schedule_update() {
        let plan = build_completion_reconciliation_plan(vec![CompletedProcess {
            subject_id: "schedule:nightly".to_string(),
            task_id: Some("TASK-999".to_string()),
            schedule_id: Some("nightly".to_string()),
            exit_code: Some(17),
            success: false,
            failure_reason: None,
            events: Vec::new(),
        }]);

        assert_eq!(plan.executed_workflow_phases, 0);
        assert_eq!(plan.failed_workflow_phases, 1);
        assert_eq!(
            plan.dispositions[0].task_action,
            Some(TaskCompletionAction::MarkBlocked {
                task_id: "TASK-999".to_string(),
                reason: "workflow runner failed: workflow runner exited with status Some(17)"
                    .to_string(),
            })
        );
        assert_eq!(
            plan.dispositions[0].schedule_update,
            Some(ScheduleCompletionUpdate {
                schedule_id: "nightly".to_string(),
                status: "failed".to_string(),
            })
        );
    }

    #[test]
    fn preserves_non_task_subjects_without_task_actions() {
        let plan = build_completion_reconciliation_plan(vec![CompletedProcess {
            subject_id: "schedule:daily-review".to_string(),
            task_id: None,
            schedule_id: Some("daily-review".to_string()),
            exit_code: Some(0),
            success: true,
            failure_reason: None,
            events: Vec::new(),
        }]);

        assert!(plan.dispositions[0].task_action.is_none());
        assert_eq!(
            plan.dispositions[0].schedule_update,
            Some(ScheduleCompletionUpdate {
                schedule_id: "daily-review".to_string(),
                status: "completed".to_string(),
            })
        );
    }
}
