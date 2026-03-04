use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub(crate) enum ScheduleCommand {
    /// List configured workflow schedules.
    List,
    /// Manually fire a schedule now.
    Fire {
        #[arg(long, value_name = "SCHEDULE_ID", help = "Schedule identifier.")]
        id: String,
    },
    /// Show historical status for a schedule.
    History {
        #[arg(long, value_name = "SCHEDULE_ID", help = "Schedule identifier.")]
        id: String,
    },
}

