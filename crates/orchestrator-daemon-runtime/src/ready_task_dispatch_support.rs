use std::collections::HashSet;

use chrono::Utc;
use orchestrator_core::{
    Complexity, DaemonHealth, OrchestratorTask, OrchestratorWorkflow, WorkflowStatus,
};

pub fn ready_task_dispatch_limit(max_tasks_per_tick: usize, health: &DaemonHealth) -> usize {
    if max_tasks_per_tick == 0 {
        return 0;
    }
    match health.max_agents {
        Some(max_agents) => {
            let available_agent_slots = max_agents.saturating_sub(health.active_agents);
            max_tasks_per_tick.min(available_agent_slots)
        }
        None => max_tasks_per_tick,
    }
}

pub fn normalize_optional_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(|candidate| candidate.to_string())
}

pub fn is_terminally_completed_workflow(workflow: &OrchestratorWorkflow) -> bool {
    workflow.status == WorkflowStatus::Completed
        && workflow.machine_state == orchestrator_core::WorkflowMachineState::Completed
        && workflow.completed_at.is_some()
}

pub fn active_workflow_task_ids(workflows: &[OrchestratorWorkflow]) -> HashSet<String> {
    workflows
        .iter()
        .filter(|workflow| {
            matches!(
                workflow.status,
                WorkflowStatus::Running | WorkflowStatus::Paused | WorkflowStatus::Pending
            ) && workflow.machine_state != orchestrator_core::WorkflowMachineState::MergeConflict
        })
        .map(|workflow| workflow.task_id.clone())
        .collect()
}

pub fn workflow_current_phase_id(workflow: &OrchestratorWorkflow) -> Option<String> {
    workflow
        .current_phase
        .as_deref()
        .map(str::to_string)
        .or_else(|| {
            workflow
                .phases
                .get(workflow.current_phase_index)
                .map(|phase| phase.phase_id.clone())
        })
        .and_then(|phase_id| normalize_optional_id(Some(phase_id.as_str())))
}

pub fn routing_complexity_for_task(
    task: &OrchestratorTask,
) -> Option<protocol::ModelRoutingComplexity> {
    match task.complexity {
        Complexity::Low => Some(protocol::ModelRoutingComplexity::Low),
        Complexity::Medium => Some(protocol::ModelRoutingComplexity::Medium),
        Complexity::High => Some(protocol::ModelRoutingComplexity::High),
    }
}

pub fn should_skip_dispatch(task: &OrchestratorTask) -> bool {
    const MAX_DISPATCH_RETRIES: u32 = 3;
    const MIN_RETRY_DELAY_SECS: i64 = 60;

    if let Some(count) = task.consecutive_dispatch_failures {
        if count >= MAX_DISPATCH_RETRIES {
            return true;
        }
    }
    if let Some(ref last_failure) = task.last_dispatch_failure_at {
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(last_failure) {
            let elapsed = Utc::now().signed_duration_since(parsed.with_timezone(&Utc));
            if elapsed.num_seconds() < MIN_RETRY_DELAY_SECS {
                return true;
            }
        }
    }
    false
}
