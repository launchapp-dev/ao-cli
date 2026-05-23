pub mod control;
mod daemon;
mod dispatch;
mod inproc_subject_backend;
mod log_storage;
mod queue;
mod schedule;
mod subject_dispatch;
mod tick;

pub use daemon::{
    discover_installed_plugins, run_daemon, run_plugin_preflight, DaemonEventLog, DaemonEventsPollResponse,
    DaemonRunEvent, DaemonRunGuard, DaemonRunHooks, DaemonRuntimeOptions, DaemonRuntimeState, DiscoveredPluginSummary,
    PreflightOutcome,
};
pub use dispatch::{
    active_workflow_subject_ids, active_workflow_task_ids, build_completion_reconciliation_plan, build_runner_command,
    build_runner_command_from_dispatch, execute_dispatch_plan_via_runner, is_terminally_completed_workflow,
    ready_dispatch_limit, ready_dispatch_limit_for_options, schedule_headroom, workflow_current_phase_id,
    CompletedProcess, CompletionReconciliationPlan, DispatchNotice, DispatchNoticeSink, DispatchSelectionSource,
    DispatchWorkflowStart, DispatchWorkflowStartSummary, PlannedDispatchStart, ProcessManager,
};
pub use inproc_subject_backend::{
    add_kind_prefix, build_inproc_adapters_for_project, build_inproc_subject_adapters, env_truthy,
    requirements_adapter_enabled, spawn_inproc_requirements_backend, spawn_inproc_task_backend, strip_kind_prefix,
    task_adapter_enabled, BuiltinAdapterOpts, BUILTIN_REQUIREMENTS_PLUGIN_NAME, BUILTIN_TASK_PLUGIN_NAME,
    DISABLE_BUILTIN_REQUIREMENTS_ADAPTER_ENV, DISABLE_BUILTIN_TASK_ADAPTER_ENV, REQUIREMENT_KIND, TASK_KIND,
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
pub use subject_dispatch::{
    discover_subject_backends, resolve_subject_dispatch, subject_plugins_disable_env_set, SubjectDispatchResolution,
    SubjectPluginDispatch, SUBJECT_PLUGINS_DISABLE_ENV,
};
pub use tick::{
    default_slim_project_tick_driver, run_project_tick, run_project_tick_at, DefaultProjectTickServices,
    DefaultSlimProjectTickDriver, ProjectTickContext, ProjectTickExecutionOutcome, ProjectTickHooks, ProjectTickPlan,
    ProjectTickPreparation, ProjectTickRunMode, ProjectTickSnapshot, ProjectTickSummary, ProjectTickSummaryInput,
    ProjectTickTime, TaskStateChangeEvent, TickSummaryBuilder,
};
