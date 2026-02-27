use clap::{Args, Subcommand};

use super::IdArgs;

#[derive(Debug, Subcommand)]
pub(crate) enum ErrorsCommand {
    /// List recorded errors.
    List(ErrorsListArgs),
    /// Get an error by id.
    Get(IdArgs),
    /// Show error summary stats.
    Stats,
    /// Retry an error by id.
    Retry(IdArgs),
    /// Remove old error records.
    Cleanup(ErrorsCleanupArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ErrorsListArgs {
    #[arg(long)]
    pub(crate) category: Option<String>,
    #[arg(long)]
    pub(crate) severity: Option<String>,
    #[arg(long)]
    pub(crate) task_id: Option<String>,
    #[arg(long)]
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Args)]
pub(crate) struct ErrorsCleanupArgs {
    #[arg(long, default_value_t = 30)]
    pub(crate) days: u32,
}
