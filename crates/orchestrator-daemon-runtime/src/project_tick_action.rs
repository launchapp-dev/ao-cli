#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTickAction {
    BootstrapFromVision,
    ResumeInterrupted,
    RecoverOrphanedRunningWorkflows,
    ReconcileStaleTasks,
    ReconcileMergeTasks,
    ReconcileCompletedProcesses,
    DispatchReadyTasks { limit: usize },
    RefreshRuntimeBinaries,
}
