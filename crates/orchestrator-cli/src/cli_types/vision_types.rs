use clap::{ArgAction, Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum VisionCommand {
    /// Draft a project vision.
    Draft(VisionDraftArgs),
    /// Refine the existing project vision.
    Refine(VisionRefineArgs),
    /// Read the current project vision.
    Get,
}

#[derive(Debug, Args)]
pub(crate) struct VisionDraftArgs {
    #[arg(long)]
    pub(crate) project_name: Option<String>,
    #[arg(long, default_value = "")]
    pub(crate) problem: String,
    #[arg(long)]
    pub(crate) target_user: Vec<String>,
    #[arg(long)]
    pub(crate) goal: Vec<String>,
    #[arg(long)]
    pub(crate) constraint: Vec<String>,
    #[arg(long)]
    pub(crate) value_proposition: Option<String>,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    pub(crate) use_ai_complexity: bool,
    #[arg(long, default_value = "codex")]
    pub(crate) tool: String,
    #[arg(long, default_value = "gpt-5.3-codex")]
    pub(crate) model: String,
    #[arg(long)]
    pub(crate) timeout_secs: Option<u64>,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    pub(crate) start_runner: bool,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) allow_heuristic_fallback: bool,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct VisionRefineArgs {
    #[arg(long)]
    pub(crate) focus: Option<String>,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    pub(crate) use_ai: bool,
    #[arg(long, default_value = "codex")]
    pub(crate) tool: String,
    #[arg(long, default_value = "gpt-5.3-codex")]
    pub(crate) model: String,
    #[arg(long)]
    pub(crate) timeout_secs: Option<u64>,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    pub(crate) start_runner: bool,
    #[arg(long, action = ArgAction::Set, default_value_t = false)]
    pub(crate) allow_heuristic_fallback: bool,
    #[arg(long, action = ArgAction::Set, default_value_t = true)]
    pub(crate) preserve_core: bool,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}
