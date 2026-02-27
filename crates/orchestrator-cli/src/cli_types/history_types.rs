use clap::{Args, Subcommand};

use super::IdArgs;

#[derive(Debug, Subcommand)]
pub(crate) enum HistoryCommand {
    /// List history records for a task.
    Task(HistoryTaskArgs),
    /// Get a history record by id.
    Get(IdArgs),
    /// List recent history records.
    Recent(HistoryRecentArgs),
    /// Search history records.
    Search(HistorySearchArgs),
    /// Remove old history records.
    Cleanup(HistoryCleanupArgs),
}

#[derive(Debug, Args)]
pub(crate) struct HistoryTaskArgs {
    #[arg(long)]
    pub(crate) task_id: String,
    #[arg(long)]
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Args)]
pub(crate) struct HistoryRecentArgs {
    #[arg(long)]
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Args)]
pub(crate) struct HistorySearchArgs {
    #[arg(long)]
    pub(crate) task_id: Option<String>,
    #[arg(long)]
    pub(crate) workflow_id: Option<String>,
    #[arg(long)]
    pub(crate) status: Option<String>,
    #[arg(long)]
    pub(crate) started_after: Option<String>,
    #[arg(long)]
    pub(crate) started_before: Option<String>,
    #[arg(long)]
    pub(crate) limit: Option<usize>,
    #[arg(long)]
    pub(crate) offset: Option<usize>,
}

#[derive(Debug, Args)]
pub(crate) struct HistoryCleanupArgs {
    #[arg(long, default_value_t = 30)]
    pub(crate) days: i64,
}
