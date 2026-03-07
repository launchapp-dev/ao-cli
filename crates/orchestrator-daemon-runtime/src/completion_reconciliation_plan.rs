use crate::{CompletedProcess, SubjectExecutionFact};

#[derive(Debug, Clone, Default)]
pub struct CompletionReconciliationPlan {
    pub executed_workflow_phases: usize,
    pub failed_workflow_phases: usize,
    pub execution_facts: Vec<SubjectExecutionFact>,
}

pub fn build_completion_reconciliation_plan(
    completed_processes: Vec<CompletedProcess>,
) -> CompletionReconciliationPlan {
    let mut plan = CompletionReconciliationPlan::default();

    for completed in completed_processes {
        let failure_reason = if completed.success {
            None
        } else {
            Some(format!(
                "workflow runner failed: {}",
                completion_reason(&completed)
            ))
        };

        if completed.success {
            plan.executed_workflow_phases = plan.executed_workflow_phases.saturating_add(1);
        } else {
            plan.failed_workflow_phases = plan.failed_workflow_phases.saturating_add(1);
        }

        plan.execution_facts.push(SubjectExecutionFact {
            subject_id: completed.subject_id,
            task_id: completed.task_id,
            schedule_id: completed.schedule_id,
            exit_code: completed.exit_code,
            success: completed.success,
            failure_reason,
            runner_events: completed.events,
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
        assert_eq!(plan.execution_facts.len(), 1);
        assert_eq!(plan.execution_facts[0].task_id.as_deref(), Some("TASK-123"));
        assert!(plan.execution_facts[0].schedule_id.is_none());
        assert!(plan.execution_facts[0].failure_reason.is_none());
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
        assert_eq!(plan.execution_facts[0].task_id.as_deref(), Some("TASK-999"));
        assert_eq!(
            plan.execution_facts[0].schedule_id.as_deref(),
            Some("nightly")
        );
        assert_eq!(plan.execution_facts[0].completion_status(), "failed");
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

        assert!(plan.execution_facts[0].task_id.is_none());
        assert_eq!(
            plan.execution_facts[0].schedule_id.as_deref(),
            Some("daily-review")
        );
        assert_eq!(plan.execution_facts[0].completion_status(), "completed");
    }
}
