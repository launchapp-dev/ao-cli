mod daemon;
mod dispatch;
mod log_storage;
mod queue;
mod schedule;
mod tick;

pub use daemon::{
    run_daemon, DaemonEventLog, DaemonEventsPollResponse, DaemonRunEvent, DaemonRunGuard, DaemonRunHooks,
    DaemonRuntimeOptions, DaemonRuntimeState, DiscoveredPluginSummary,
};
pub use dispatch::{
    active_workflow_subject_ids, active_workflow_task_ids, build_completion_reconciliation_plan, build_runner_command,
    build_runner_command_from_dispatch, execute_dispatch_plan_via_runner, is_terminally_completed_workflow,
    ready_dispatch_limit, ready_dispatch_limit_for_options, schedule_headroom, workflow_current_phase_id,
    CompletedProcess, CompletionReconciliationPlan, DispatchNotice, DispatchNoticeSink, DispatchSelectionSource,
    DispatchWorkflowStart, DispatchWorkflowStartSummary, PlannedDispatchStart, ProcessManager,
};
pub use log_storage::{
    discover_log_storage_backends, log_storage_disable_env_set, resolve_log_storage_dispatch, LogStorageDispatch,
    LogStorageResolution, LOG_STORAGE_DISABLE_ENV,
};
pub use protocol::{RunnerEvent, SubjectDispatch, SubjectExecutionFact};
pub use queue::{
    dispatch_queue_state_path, drop_subject, enqueue_subject_dispatch, hold_subject, load_dispatch_queue_state,
    mark_dispatch_queue_entry_assigned, queue_snapshot, queue_stats, release_subject,
    remove_terminal_dispatch_queue_entry_non_fatal, reorder_subjects, save_dispatch_queue_state, DispatchQueueEntry,
    DispatchQueueEntryStatus, DispatchQueueState, QueueEnqueueResult, QueueEntrySnapshot, QueueSnapshot, QueueStats,
};
pub use schedule::{
    discover_trigger_plugins, ScheduleDispatch, ScheduleDispatchOutcome, TriggerDispatch, TriggerDispatchOutcome,
    TriggerSupervisor, TriggerSupervisorEvent, TriggerSupervisorSink, MAX_RESTART_ATTEMPTS,
};
pub use tick::{
    default_slim_project_tick_driver, run_project_tick, run_project_tick_at, DefaultProjectTickServices,
    DefaultSlimProjectTickDriver, ProjectTickContext, ProjectTickExecutionOutcome, ProjectTickHooks, ProjectTickPlan,
    ProjectTickPreparation, ProjectTickRunMode, ProjectTickSnapshot, ProjectTickSummary, ProjectTickSummaryInput,
    ProjectTickTime, TaskStateChangeEvent, TickSummaryBuilder,
};
