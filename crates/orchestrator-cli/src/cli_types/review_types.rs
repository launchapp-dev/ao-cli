use clap::{Args, Subcommand};

use super::{IdArgs, TaskIdArgs};

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewCommand {
    /// Compute review status for an entity.
    Entity(ReviewEntityArgs),
    /// Record a review decision.
    Record(ReviewRecordArgs),
    /// Compute review status for a task.
    TaskStatus(TaskIdArgs),
    /// Compute review status for a requirement.
    RequirementStatus(IdArgs),
    /// Record a handoff between roles for a run.
    Handoff(ReviewHandoffArgs),
    /// Record dual-approval for a task.
    DualApprove(ReviewDualApproveArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ReviewEntityArgs {
    #[arg(long)]
    pub(crate) entity_type: String,
    #[arg(long)]
    pub(crate) entity_id: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewRecordArgs {
    #[arg(long)]
    pub(crate) entity_type: String,
    #[arg(long)]
    pub(crate) entity_id: String,
    #[arg(long)]
    pub(crate) reviewer_role: String,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) rationale: Option<String>,
    #[arg(long)]
    pub(crate) source: Option<String>,
    #[arg(long)]
    pub(crate) content_hash: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewHandoffArgs {
    #[arg(long)]
    pub(crate) run_id: String,
    #[arg(long)]
    pub(crate) target_role: String,
    #[arg(long)]
    pub(crate) question: String,
    #[arg(long)]
    pub(crate) context_json: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewDualApproveArgs {
    #[arg(long)]
    pub(crate) task_id: String,
    #[arg(long)]
    pub(crate) rationale: Option<String>,
}
