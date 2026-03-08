#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTickAction {
    BootstrapFromVision,
    ResumeInterrupted,
    RecoverOrphanedRunningWorkflows,
    ReconcileStaleTasks,
    ReconcileCompletedProcesses,
    DispatchReadyTasks { limit: usize },
    RefreshRuntimeBinaries,
}
