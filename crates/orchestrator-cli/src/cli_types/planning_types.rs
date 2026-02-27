use clap::{ArgAction, Args, Subcommand};

use super::{
    IdArgs, RequirementsDraftArgs, RequirementsRefineArgs, VisionDraftArgs, VisionRefineArgs,
};

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningCommand {
    /// Vision planning commands.
    Vision {
        #[command(subcommand)]
        command: PlanningVisionCommand,
    },
    /// Requirements planning commands.
    Requirements {
        #[command(subcommand)]
        command: PlanningRequirementsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningVisionCommand {
    /// Draft a project vision.
    Draft(VisionDraftArgs),
    /// Refine existing project vision.
    Refine(VisionRefineArgs),
    /// Read current project vision.
    Get,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PlanningRequirementsCommand {
    /// Draft requirements from project context.
    Draft(RequirementsDraftArgs),
    /// List requirements.
    List,
    /// Get a requirement by id.
    Get(IdArgs),
    /// Refine requirements.
    Refine(RequirementsRefineArgs),
    /// Execute requirements planning into tasks/workflows.
    Execute(PlanningExecuteArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PlanningExecuteArgs {
    #[arg(long = "id")]
    pub(crate) requirement_ids: Vec<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) start_workflows: bool,
    #[arg(long)]
    pub(crate) pipeline_id: Option<String>,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) ai_task_generation: bool,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) include_wont: bool,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}
