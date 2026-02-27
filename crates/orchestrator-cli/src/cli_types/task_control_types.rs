use clap::{Args, Subcommand};

use super::{TaskIdArgs, TASK_PRIORITY_HELP};

#[derive(Debug, Subcommand)]
pub(crate) enum TaskControlCommand {
    /// Pause a task.
    Pause(TaskIdArgs),
    /// Resume a paused task.
    Resume(TaskIdArgs),
    /// Cancel a task (confirmation required).
    Cancel(TaskControlCancelArgs),
    /// Set task priority.
    SetPriority(TaskControlPriorityArgs),
    /// Set or clear task deadline.
    SetDeadline(TaskControlDeadlineArgs),
}

#[derive(Debug, Args)]

pub(crate) struct TaskControlCancelArgs {
    #[arg(long, value_name = "TASK_ID", help = "Task identifier.")]
    pub(crate) task_id: String,
    #[arg(
        long,
        value_name = "TASK_ID",
        help = "Confirmation token; must match --task-id."
    )]
    pub(crate) confirm: Option<String>,
    #[arg(
        long,
        default_value_t = false,
        help = "Preview cancellation payload without mutating task state."
    )]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct TaskControlPriorityArgs {
    #[arg(long, value_name = "TASK_ID", help = "Task identifier.")]
    pub(crate) task_id: String,
    #[arg(long, value_name = "PRIORITY", help = TASK_PRIORITY_HELP)]
    pub(crate) priority: String,
}

#[derive(Debug, Args)]
pub(crate) struct TaskControlDeadlineArgs {
    #[arg(long, value_name = "TASK_ID", help = "Task identifier.")]
    pub(crate) task_id: String,
    #[arg(
        long,
        value_name = "RFC3339",
        help = "Deadline timestamp (RFC 3339), for example 2026-03-01T09:30:00Z."
    )]
    pub(crate) deadline: Option<String>,
}
