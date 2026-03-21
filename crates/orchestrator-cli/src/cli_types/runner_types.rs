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
    /// Run IDs to clean (from cli-tracker.json). Use "agent-runner-all" to clean
    /// all orphaned agent-runner processes, or "--kill-agent-runners" flag.
    #[arg(long = "run-id")]
    pub(crate) run_id: Vec<String>,

    /// Kill all orphaned agent-runner daemon processes (those with PPID=1).
    /// This is useful when agent-runner processes become orphaned after their
    /// parent ao-workflow-runner processes die.
    #[arg(long = "kill-agent-runners", default_value = "false")]
    pub(crate) kill_agent_runners: bool,
}
