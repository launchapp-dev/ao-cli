use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum QaCommand {
    /// Evaluate QA gates for a workflow phase.
    Evaluate(QaEvaluateArgs),
    /// Get QA evaluation result for a workflow phase.
    Get(QaPhaseArgs),
    /// List QA evaluations for a workflow.
    List(QaWorkflowArgs),
    /// Manage QA approvals.
    Approval {
        #[command(subcommand)]
        command: QaApprovalCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct QaEvaluateArgs {
    #[arg(long)]
    pub(crate) workflow_id: String,
    #[arg(long)]
    pub(crate) phase_id: String,
    #[arg(long)]
    pub(crate) task_id: String,
    #[arg(long)]
    pub(crate) worktree_path: Option<String>,
    #[arg(long)]
    pub(crate) gates_json: Option<String>,
    #[arg(long)]
    pub(crate) metrics_json: Option<String>,
    #[arg(long)]
    pub(crate) metadata_json: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct QaPhaseArgs {
    #[arg(long)]
    pub(crate) workflow_id: String,
    #[arg(long)]
    pub(crate) phase_id: String,
}

#[derive(Debug, Args)]
pub(crate) struct QaWorkflowArgs {
    #[arg(long)]
    pub(crate) workflow_id: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum QaApprovalCommand {
    /// Add a QA gate approval.
    Add(QaApprovalAddArgs),
    /// List QA gate approvals.
    List(QaApprovalListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct QaApprovalAddArgs {
    #[arg(long)]
    pub(crate) workflow_id: String,
    #[arg(long)]
    pub(crate) phase_id: String,
    #[arg(long)]
    pub(crate) gate_id: String,
    #[arg(long)]
    pub(crate) approved_by: String,
    #[arg(long)]
    pub(crate) comment: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct QaApprovalListArgs {
    #[arg(long)]
    pub(crate) workflow_id: String,
    #[arg(long)]
    pub(crate) gate_id: String,
}
