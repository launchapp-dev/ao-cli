use super::*;
use crate::services::runtime::runtime_daemon::daemon_process_manager::{
    ProcessManager, WorkflowSubjectArgs,
};

async fn dispatch_ready_tasks_via_runner(
    hub: Arc<dyn ServiceHub>,
    root: &str,
    process_manager: &mut ProcessManager,
    limit: usize,
) -> Result<ReadyTaskWorkflowStartSummary> {
    let workflows = hub.workflows().list().await.unwrap_or_default();
    let active_task_ids = active_workflow_task_ids(&workflows);
    let candidates = hub.tasks().list_prioritized().await?;
    let mut started_workflows = Vec::new();

    for task in candidates {
        if started_workflows.len() >= limit {
            break;
        }

        if task.paused || task.cancelled {
            continue;
        }
        if task.status != TaskStatus::Ready {
            continue;
        }
        if active_task_ids.contains(&task.id) {
            continue;
        }
        if should_skip_dispatch(&task) {
            continue;
        }

        let dependency_issues = dependency_gate_issues_for_task(hub.clone(), root, &task).await;
        if !dependency_issues.is_empty() {
            let reason = dependency_blocked_reason(&dependency_issues);
            let _ = set_task_blocked_with_reason(hub.clone(), &task, reason, None).await;
            continue;
        }

        let pipeline_id = super::pipeline_for_task(&task);
        let subject = WorkflowSubjectArgs::Task {
            task_id: task.id.clone(),
        };
        match process_manager.spawn_workflow_runner(&subject, &pipeline_id, root) {
            Ok(_) => {
                let _ = hub
                    .tasks()
                    .set_status(&task.id, TaskStatus::InProgress, false)
                    .await;
                started_workflows.push(ReadyTaskWorkflowStart {
                    task_id: task.id.clone(),
                    workflow_id: task.id.clone(),
                    selection_source: TaskSelectionSource::FallbackPicker,
                });
            }
            Err(error) => {
                let reason = format!("failed to start workflow runner: {error}");
                let _ = set_task_blocked_with_reason(hub.clone(), &task, reason, None).await;
            }
        }
    }

    Ok(ReadyTaskWorkflowStartSummary {
        started: started_workflows.len(),
        started_workflows,
    })
}

#[cfg(test)]
pub(super) struct FullProjectTickExecutor<'a> {
    pub(super) hub: Arc<dyn ServiceHub>,
    pub(super) root: &'a str,
    pub(super) args: &'a DaemonRuntimeOptions,
}

#[cfg(test)]
#[async_trait::async_trait(?Send)]
impl ProjectTickActionExecutor for FullProjectTickExecutor<'_> {
    async fn execute_action(
        &mut self,
        action: &ProjectTickAction,
    ) -> Result<ProjectTickActionEffect> {
        match action {
            ProjectTickAction::BootstrapFromVision => {
                bootstrap_from_vision_if_needed(
                    self.hub.clone(),
                    self.args.startup_cleanup,
                    self.args.ai_task_generation,
                )
                .await?;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::EnsureAiGeneratedTasks => {
                let _ = ensure_tasks_for_unplanned_requirements(self.hub.clone(), self.root).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::ResumeInterrupted => {
                let (cleaned_stale_workflows, resumed_workflows) =
                    resume_interrupted_workflows_for_project(self.hub.clone(), self.root).await?;
                Ok(ProjectTickActionEffect::ResumedInterrupted {
                    cleaned_stale_workflows,
                    resumed_workflows,
                })
            }
            ProjectTickAction::RecoverOrphanedRunningWorkflows => {
                let _ = recover_orphaned_running_workflows(self.hub.clone(), self.root).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::ReconcileStaleTasks => {
                let count = reconcile_stale_in_progress_tasks_for_project(
                    self.hub.clone(),
                    self.root,
                    self.args.stale_threshold_hours,
                )
                .await?;
                Ok(ProjectTickActionEffect::ReconciledStaleTasks { count })
            }
            ProjectTickAction::ReconcileDependencyTasks => {
                let count =
                    reconcile_dependency_gate_tasks_for_project(self.hub.clone(), self.root)
                        .await?;
                Ok(ProjectTickActionEffect::ReconciledDependencyTasks { count })
            }
            ProjectTickAction::ReconcileMergeTasks => {
                let count =
                    reconcile_merge_gate_tasks_for_project(self.hub.clone(), self.root).await?;
                Ok(ProjectTickActionEffect::ReconciledMergeTasks { count })
            }
            ProjectTickAction::ReconcileCompletedProcesses => Ok(ProjectTickActionEffect::Noop),
            ProjectTickAction::RetryFailedTaskWorkflows => {
                let _ = retry_failed_task_workflows(self.hub.clone()).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::PromoteBacklogTasksToReady => {
                let _ = promote_backlog_tasks_to_ready(self.hub.clone(), self.root).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::DispatchReadyTasks { limit } => {
                let summary =
                    run_ready_task_workflows_for_project(self.hub.clone(), self.root, *limit)
                        .await?;
                Ok(ProjectTickActionEffect::ReadyWorkflowStarts { summary })
            }
            ProjectTickAction::RefreshRuntimeBinaries => {
                let _ = git_ops::refresh_runtime_binaries_if_main_advanced(
                    self.hub.clone(),
                    self.root,
                    git_ops::RuntimeBinaryRefreshTrigger::Tick,
                )
                .await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::ExecuteRunningWorkflowPhases { limit } => {
                let (executed_workflow_phases, failed_workflow_phases, phase_execution_events) =
                    execute_running_workflow_phases_for_project(
                        self.hub.clone(),
                        self.root,
                        *limit,
                    )
                    .await?;
                Ok(ProjectTickActionEffect::ExecutedRunningWorkflowPhases {
                    executed_workflow_phases,
                    failed_workflow_phases,
                    phase_execution_events,
                })
            }
        }
    }
}

pub(super) struct SlimProjectTickExecutor<'a> {
    pub(super) hub: Arc<dyn ServiceHub>,
    pub(super) root: &'a str,
    pub(super) args: &'a DaemonRuntimeOptions,
    pub(super) process_manager: &'a mut ProcessManager,
}

#[async_trait::async_trait(?Send)]
impl ProjectTickActionExecutor for SlimProjectTickExecutor<'_> {
    async fn execute_action(
        &mut self,
        action: &ProjectTickAction,
    ) -> Result<ProjectTickActionEffect> {
        match action {
            ProjectTickAction::BootstrapFromVision => {
                bootstrap_from_vision_if_needed(
                    self.hub.clone(),
                    self.args.startup_cleanup,
                    self.args.ai_task_generation,
                )
                .await?;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::EnsureAiGeneratedTasks => {
                let _ = ensure_tasks_for_unplanned_requirements(self.hub.clone(), self.root).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::ResumeInterrupted => {
                let (cleaned_stale_workflows, resumed_workflows) =
                    resume_interrupted_workflows_for_project(self.hub.clone(), self.root).await?;
                Ok(ProjectTickActionEffect::ResumedInterrupted {
                    cleaned_stale_workflows,
                    resumed_workflows,
                })
            }
            ProjectTickAction::RecoverOrphanedRunningWorkflows => {
                let _ = recover_orphaned_running_workflows(self.hub.clone(), self.root).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::ReconcileStaleTasks => {
                let count = reconcile_stale_in_progress_tasks_for_project(
                    self.hub.clone(),
                    self.root,
                    self.args.stale_threshold_hours,
                )
                .await?;
                Ok(ProjectTickActionEffect::ReconciledStaleTasks { count })
            }
            ProjectTickAction::ReconcileDependencyTasks => {
                let count =
                    reconcile_dependency_gate_tasks_for_project(self.hub.clone(), self.root)
                        .await?;
                Ok(ProjectTickActionEffect::ReconciledDependencyTasks { count })
            }
            ProjectTickAction::ReconcileMergeTasks => {
                let count =
                    reconcile_merge_gate_tasks_for_project(self.hub.clone(), self.root).await?;
                Ok(ProjectTickActionEffect::ReconciledMergeTasks { count })
            }
            ProjectTickAction::ReconcileCompletedProcesses => {
                let completed_processes = self.process_manager.check_running();
                let (executed_workflow_phases, failed_workflow_phases) =
                    CompletionReconciler::reconcile(
                        self.hub.clone(),
                        self.root,
                        completed_processes,
                    )
                    .await;
                Ok(ProjectTickActionEffect::ReconciledCompletedProcesses {
                    executed_workflow_phases,
                    failed_workflow_phases,
                })
            }
            ProjectTickAction::RetryFailedTaskWorkflows => {
                let _ = retry_failed_task_workflows(self.hub.clone()).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::PromoteBacklogTasksToReady => {
                let _ = promote_backlog_tasks_to_ready(self.hub.clone(), self.root).await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::DispatchReadyTasks { limit } => {
                let summary = dispatch_ready_tasks_via_runner(
                    self.hub.clone(),
                    self.root,
                    self.process_manager,
                    *limit,
                )
                .await?;
                Ok(ProjectTickActionEffect::ReadyWorkflowStarts { summary })
            }
            ProjectTickAction::RefreshRuntimeBinaries => {
                let _ = git_ops::refresh_runtime_binaries_if_main_advanced(
                    self.hub.clone(),
                    self.root,
                    git_ops::RuntimeBinaryRefreshTrigger::Tick,
                )
                .await;
                Ok(ProjectTickActionEffect::Noop)
            }
            ProjectTickAction::ExecuteRunningWorkflowPhases { .. } => {
                Ok(ProjectTickActionEffect::Noop)
            }
        }
    }
}
