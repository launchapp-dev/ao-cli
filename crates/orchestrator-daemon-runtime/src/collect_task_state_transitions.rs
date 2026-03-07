use crate::{ReadyTaskWorkflowStart, TaskStateTransition};
use workflow_runner::executor::PhaseExecutionEvent;

pub fn collect_task_state_transitions(
    before: &[orchestrator_core::OrchestratorTask],
    after: &[orchestrator_core::OrchestratorTask],
    workflows: &[orchestrator_core::OrchestratorWorkflow],
    phase_events: &[PhaseExecutionEvent],
    ready_starts: &[ReadyTaskWorkflowStart],
) -> Vec<TaskStateTransition> {
    let before_lookup: std::collections::HashMap<&str, &orchestrator_core::OrchestratorTask> =
        before.iter().map(|task| (task.id.as_str(), task)).collect();

    let mut phase_context_by_task: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for event in phase_events {
        phase_context_by_task.insert(
            event.task_id.clone(),
            (event.workflow_id.clone(), event.phase_id.clone()),
        );
    }

    let mut workflow_context_by_task: std::collections::HashMap<
        String,
        (String, Option<String>, i64),
    > = std::collections::HashMap::new();
    for workflow in workflows {
        let started_at_unix_ms = workflow.started_at.timestamp_millis();
        let candidate = (
            workflow.id.clone(),
            normalize_optional_id(workflow.current_phase.as_deref()),
            started_at_unix_ms,
        );
        match workflow_context_by_task.get_mut(workflow.task_id.as_str()) {
            Some(existing) if existing.2 >= started_at_unix_ms => {}
            Some(existing) => {
                *existing = candidate;
            }
            None => {
                workflow_context_by_task.insert(workflow.task_id.clone(), candidate);
            }
        }
    }

    let mut selection_source_by_task: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    for start in ready_starts {
        selection_source_by_task.insert(
            start.task_id.clone(),
            (
                start.workflow_id.clone(),
                start.selection_source.as_str().to_string(),
            ),
        );
    }

    let mut transitions = Vec::new();
    for task in after {
        let Some(previous) = before_lookup.get(task.id.as_str()) else {
            continue;
        };
        if previous.status == task.status {
            continue;
        }

        let (mut workflow_id, phase_id) = match phase_context_by_task.get(task.id.as_str()) {
            Some((workflow_id, phase_id)) => (
                Some(workflow_id.clone()),
                normalize_optional_id(Some(phase_id.as_str())),
            ),
            None => workflow_context_by_task
                .get(task.id.as_str())
                .map(|(workflow_id, phase_id, _)| (Some(workflow_id.clone()), phase_id.clone()))
                .unwrap_or((None, None)),
        };
        let selection_source = selection_source_by_task.get(task.id.as_str()).map(
            |(started_workflow_id, selection_source)| {
                if workflow_id.is_none() {
                    workflow_id = Some(started_workflow_id.clone());
                }
                selection_source.clone()
            },
        );

        transitions.push(TaskStateTransition {
            task_id: task.id.clone(),
            from_status: previous.status.to_string(),
            to_status: task.status.to_string(),
            changed_at: task.metadata.updated_at.to_rfc3339(),
            workflow_id,
            phase_id,
            selection_source,
        });
    }

    transitions.sort_by(|a, b| {
        a.changed_at
            .cmp(&b.changed_at)
            .then(a.task_id.cmp(&b.task_id))
    });
    transitions
}

fn normalize_optional_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(|candidate| candidate.to_string())
}
