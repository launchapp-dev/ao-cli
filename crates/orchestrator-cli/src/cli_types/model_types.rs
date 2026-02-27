use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum ModelCommand {
    /// Check model availability for one or more model ids.
    Availability(ModelAvailabilityArgs),
    /// Show configured model and API-key status.
    Status(ModelStatusArgs),
    /// Validate model selection for a task or explicit list.
    Validate(ModelValidateArgs),
    /// Manage cached model roster metadata.
    Roster {
        #[command(subcommand)]
        command: ModelRosterCommand,
    },
    /// Run and inspect model evaluations.
    Eval {
        #[command(subcommand)]
        command: ModelEvalCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ModelAvailabilityArgs {
    #[arg(long = "model")]
    pub(crate) model: Vec<String>,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ModelStatusArgs {
    #[arg(long)]
    pub(crate) model_id: String,
    #[arg(long)]
    pub(crate) cli_tool: String,
}

#[derive(Debug, Args)]
pub(crate) struct ModelValidateArgs {
    #[arg(long)]
    pub(crate) task_id: Option<String>,
    #[arg(long = "model")]
    pub(crate) model: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ModelRosterCommand {
    /// Refresh model roster from providers.
    Refresh,
    /// Get current model roster snapshot.
    Get,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ModelEvalCommand {
    /// Run model evaluation.
    Run(ModelEvalRunArgs),
    /// Show latest model evaluation report.
    Report,
}

#[derive(Debug, Args)]
pub(crate) struct ModelEvalRunArgs {
    #[arg(long = "model")]
    pub(crate) model: Vec<String>,
}
