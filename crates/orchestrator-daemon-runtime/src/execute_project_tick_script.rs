use std::sync::Arc;

use anyhow::Result;
use orchestrator_core::services::ServiceHub;

use crate::{
    ProjectTickAction, ProjectTickActionEffect, ProjectTickExecutionOutcome, ProjectTickHooks,
    ProjectTickScript,
};

pub async fn execute_project_tick_script<H>(
    script: &ProjectTickScript,
    hooks: &mut H,
    hub: Arc<dyn ServiceHub>,
    root: &str,
) -> Result<ProjectTickExecutionOutcome>
where
    H: ProjectTickHooks,
{
    let mut outcome = ProjectTickExecutionOutcome::default();

    for action in script.actions() {
        let effect = match action {
            ProjectTickAction::ReconcileCompletedProcesses => {
                let (executed_workflow_phases, failed_workflow_phases) =
                    hooks.reconcile_completed_processes(hub.clone(), root).await?;
                ProjectTickActionEffect::ReconciledCompletedProcesses {
                    executed_workflow_phases,
                    failed_workflow_phases,
                }
            }
            ProjectTickAction::DispatchReadyTasks { limit } => {
                let summary = hooks.dispatch_ready_tasks(hub.clone(), root, *limit).await?;
                ProjectTickActionEffect::ReadyWorkflowStarts { summary }
            }
        };
        outcome.apply_effect(effect);
    }

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use chrono::{DateTime, Utc};
    use orchestrator_core::{services::ServiceHub, InMemoryServiceHub};

    use super::execute_project_tick_script;
    use crate::{
        DaemonRuntimeOptions, DispatchSelectionSource, DispatchWorkflowStart,
        DispatchWorkflowStartSummary, ProjectTickAction, ProjectTickExecutionOutcome,
        ProjectTickHooks, ProjectTickPlan, ProjectTickScript,
    };

    struct FakeHooks {
        calls: Vec<ProjectTickAction>,
    }

    #[async_trait::async_trait(?Send)]
    impl ProjectTickHooks for FakeHooks {
        fn build_hub(&mut self, _root: &str) -> Result<Arc<dyn ServiceHub>> {
            Ok(Arc::new(InMemoryServiceHub::new()))
        }

        fn process_due_schedules(&mut self, _root: &str, _now: DateTime<Utc>) {}

        async fn reconcile_completed_processes(
            &mut self,
            _hub: Arc<dyn ServiceHub>,
            _root: &str,
        ) -> Result<(usize, usize)> {
            self.calls
                .push(ProjectTickAction::ReconcileCompletedProcesses);
            Ok((0, 0))
        }

        async fn dispatch_ready_tasks(
            &mut self,
            _hub: Arc<dyn ServiceHub>,
            _root: &str,
            limit: usize,
        ) -> Result<DispatchWorkflowStartSummary> {
            self.calls
                .push(ProjectTickAction::DispatchReadyTasks { limit });
            Ok(DispatchWorkflowStartSummary {
                started: limit,
                started_workflows: vec![DispatchWorkflowStart {
                    dispatch: protocol::SubjectDispatch::for_task(
                        "TASK-1",
                        orchestrator_core::STANDARD_PIPELINE_ID,
                    ),
                    workflow_id: "wf-1".to_string(),
                    selection_source: DispatchSelectionSource::FallbackPicker,
                }],
            })
        }
    }

    #[tokio::test]
    async fn executes_actions_in_script_order_and_accumulates_outcome() {
        let options = DaemonRuntimeOptions {
            ai_task_generation: false,
            resume_interrupted: true,
            reconcile_stale: false,
            max_tasks_per_tick: 3,
            ..DaemonRuntimeOptions::default()
        };
        let plan = ProjectTickPlan::build(
            &options,
            None,
            chrono::NaiveTime::from_hms_opt(12, 0, 0).expect("time should be valid"),
            false,
            1,
        );
        let script = ProjectTickScript::build(&options, &plan);
        let mut hooks = FakeHooks { calls: Vec::new() };
        let hub: Arc<dyn ServiceHub> = Arc::new(InMemoryServiceHub::new());

        let outcome: ProjectTickExecutionOutcome =
            execute_project_tick_script(&script, &mut hooks, hub, "/tmp/project")
                .await
                .expect("script execution should succeed");

        assert_eq!(hooks.calls, script.actions().to_vec());
        assert_eq!(outcome.cleaned_stale_workflows, 0);
        assert_eq!(outcome.resumed_workflows, 0);
        assert_eq!(outcome.ready_workflow_starts.started, 1);
        assert_eq!(outcome.executed_workflow_phases, 0);
    }
}
