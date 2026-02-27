use clap::{ArgAction, Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum ExecuteCommand {
    /// Generate task execution plan from requirements.
    Plan(ExecutePlanArgs),
    /// Generate and immediately run workflows from requirements.
    Run(ExecuteRunArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ExecutePlanArgs {
    #[arg(long = "id")]
    pub(crate) requirement_ids: Vec<String>,
    #[arg(long)]
    pub(crate) pipeline_id: Option<String>,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) ai_task_generation: bool,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) include_wont: bool,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ExecuteRunArgs {
    #[arg(long = "id")]
    pub(crate) requirement_ids: Vec<String>,
    #[arg(long)]
    pub(crate) pipeline_id: Option<String>,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) ai_task_generation: bool,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) include_wont: bool,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}
