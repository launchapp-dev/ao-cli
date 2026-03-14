use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum RunnerCommand {
    /// Show runner process health.
    Health,
    /// Detect and clean orphaned runner processes.
    Orphans {
        #[command(subcommand)]
        command: RunnerOrphanCommand,
    },
    /// Show runner restart statistics.
    RestartStats,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RunnerOrphanCommand {
    /// Detect orphaned runner processes.
    Detect,
    /// Clean orphaned runner processes.
    Cleanup(RunnerOrphanCleanupArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RunnerOrphanCleanupArgs {
    #[arg(long = "run-id")]
    pub(crate) run_id: Vec<String>,
}
