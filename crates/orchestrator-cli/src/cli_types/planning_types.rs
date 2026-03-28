use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningCommand {
    /// Manage project vision planning.
    Vision {
        #[command(subcommand)]
        command: PlanningVisionCommand,
    },
    /// Manage requirements planning.
    Requirements {
        #[command(subcommand)]
        command: PlanningRequirementsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningVisionCommand {
    /// Draft project vision.
    Draft(crate::VisionDraftArgs),
    /// Refine project vision.
    Refine(crate::VisionRefineArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningRequirementsCommand {
    /// Draft requirements.
    Draft(crate::RequirementsDraftArgs),
    /// Refine requirements.
    Refine(crate::RequirementsRefineArgs),
    /// Execute a requirement into implementation tasks.
    Execute(crate::RequirementsExecuteArgs),
}
