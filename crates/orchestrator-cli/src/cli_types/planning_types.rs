use clap::Subcommand;

use super::{IdArgs, RequirementsDraftArgs, RequirementsExecuteArgs, RequirementsRefineArgs, VisionDraftArgs, VisionRefineArgs};

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningVisionCommand {
    /// Draft a project vision.
    Draft(VisionDraftArgs),
    /// Refine the existing project vision.
    Refine(VisionRefineArgs),
    /// Read the current project vision.
    Get,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningCommand {
    /// Planning facade for vision commands.
    Vision {
        #[command(subcommand)]
        command: PlanningVisionCommand,
    },
    /// Planning facade for requirements commands.
    Requirements {
        #[command(subcommand)]
        command: PlanningRequirementsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningRequirementsCommand {
    /// Draft requirements from current project context.
    Draft(RequirementsDraftArgs),
    /// Refine existing requirements.
    Refine(RequirementsRefineArgs),
    /// Execute requirements into tasks and optional workflows.
    Execute(RequirementsExecuteArgs),
    /// List requirements.
    List,
    /// Get a requirement by id.
    Get(IdArgs),
}
