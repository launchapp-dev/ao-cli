use std::sync::Arc;

use anyhow::Result;
use orchestrator_core::{
    services::ServiceHub, DaemonHealth, DaemonStatus, OrchestratorTask, RequirementItem,
};
use serde_json::Value;

use crate::{ProjectTickExecutionOutcome, ProjectTickSummaryInput};

#[derive(Debug, Clone)]
pub struct ProjectTickSnapshot {
    pub requirements_before: Vec<RequirementItem>,
    pub tasks_before: Vec<OrchestratorTask>,
    pub started_daemon: bool,
    pub daemon_health: Option<DaemonHealth>,
}

impl ProjectTickSnapshot {
    pub async fn capture(hub: Arc<dyn ServiceHub>) -> Result<Self> {
        let requirements_before = hub.planning().list_requirements().await.unwrap_or_default();
        let tasks_before = hub.tasks().list().await.unwrap_or_default();
        let daemon = hub.daemon();
        let status = daemon.status().await?;
        let mut started_daemon = false;
        if !matches!(status, DaemonStatus::Running | DaemonStatus::Paused) {
            daemon.start().await?;
            started_daemon = true;
        }
        let daemon_health = daemon.health().await.ok();

        Ok(Self {
            requirements_before,
            tasks_before,
            started_daemon,
            daemon_health,
        })
    }

    pub fn into_summary_input(
        self,
        project_root: String,
        health: Value,
        execution_outcome: ProjectTickExecutionOutcome,
        phase_execution_events: bool,
    ) -> ProjectTickSummaryInput {
        ProjectTickSummaryInput {
            project_root,
            started_daemon: self.started_daemon,
            health,
            requirements_before: self.requirements_before,
            tasks_before: self.tasks_before,
            resumed_workflows: execution_outcome.resumed_workflows,
            cleaned_stale_workflows: execution_outcome.cleaned_stale_workflows,
            reconciled_stale_tasks: execution_outcome.reconciled_stale_tasks,
            reconciled_dependency_tasks: execution_outcome.reconciled_dependency_tasks,
            reconciled_merge_tasks: execution_outcome.reconciled_merge_tasks,
            ready_started_count: execution_outcome.ready_workflow_starts.started,
            ready_started_workflows: execution_outcome.ready_workflow_starts.started_workflows,
            executed_workflow_phases: execution_outcome.executed_workflow_phases,
            failed_workflow_phases: execution_outcome.failed_workflow_phases,
            phase_execution_events: if phase_execution_events {
                execution_outcome.phase_execution_events
            } else {
                Vec::new()
            },
        }
    }
}
