#[cfg(test)]
use super::{reconciliation_test_support::*, *};
#[cfg(test)]
use orchestrator_core::FileServiceHub;
#[cfg(test)]
use orchestrator_daemon_runtime::ProjectTickSummary;
#[cfg(test)]
use orchestrator_git_ops as git_ops;
#[cfg(test)]
use std::collections::HashSet;

#[cfg(test)]
#[derive(Default)]
pub(super) struct CliPreTickOutcome {
    pub cleaned_stale_workflows: usize,
    pub resumed_workflows: usize,
    pub reconciled_tasks: usize,
}

#[cfg(test)]
pub(super) async fn run_cli_pre_tick(
    root: &str,
    args: &DaemonRuntimeOptions,
    active_subject_ids: Option<&HashSet<String>>,
) -> Result<CliPreTickOutcome> {
    let hub = Arc::new(FileServiceHub::new(root)?);

    let (cleaned_stale_workflows, resumed_workflows) = if args.resume_interrupted {
        resume_interrupted_workflows_for_project(hub.clone(), root).await?
    } else {
        (0, 0)
    };

    let empty_active_ids = HashSet::new();
    let active_ids = active_subject_ids.unwrap_or(&empty_active_ids);
    let _ = recover_orphaned_running_workflows_with_active_ids(hub.clone(), root, active_ids).await;

    let reconciled_stale_tasks = if args.reconcile_stale {
        reconcile_stale_in_progress_tasks_for_project(hub.clone(), root, args.stale_threshold_hours)
            .await?
    } else {
        0
    };

    let reconciled_dependency_tasks =
        reconcile_dependency_gate_tasks_for_project(hub.clone(), root).await?;
    let reconciled_merge_tasks = reconcile_merge_gate_tasks_for_project(hub, root).await?;

    Ok(CliPreTickOutcome {
        cleaned_stale_workflows,
        resumed_workflows,
        reconciled_tasks: reconciled_stale_tasks
            .saturating_add(reconciled_dependency_tasks)
            .saturating_add(reconciled_merge_tasks),
    })
}

#[cfg(test)]
pub(super) fn apply_cli_pre_tick(summary: &mut ProjectTickSummary, pre_tick: CliPreTickOutcome) {
    summary.cleaned_stale_workflows = summary
        .cleaned_stale_workflows
        .saturating_add(pre_tick.cleaned_stale_workflows);
    summary.resumed_workflows = summary
        .resumed_workflows
        .saturating_add(pre_tick.resumed_workflows);
    summary.reconciled_stale_tasks = summary
        .reconciled_stale_tasks
        .saturating_add(pre_tick.reconciled_tasks);
}

#[cfg(test)]
pub(super) fn flush_git_outbox_for_project(root: &str) {
    let _ = git_ops::flush_git_integration_outbox(root);
}

#[cfg(test)]
pub(super) async fn refresh_runtime_binaries_for_project(root: &str) -> Result<()> {
    let hub = Arc::new(FileServiceHub::new(root)?);
    let _ = git_ops::refresh_runtime_binaries_if_main_advanced(
        hub,
        root,
        git_ops::RuntimeBinaryRefreshTrigger::Tick,
    )
    .await;
    Ok(())
}
