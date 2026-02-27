use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct TuiArgs {
    #[arg(
        long,
        value_name = "MODEL_ID",
        help = "Model id to use for interactive runs."
    )]
    pub(crate) model: Option<String>,
    #[arg(
        long,
        value_name = "TOOL",
        help = "CLI provider, such as codex or claude."
    )]
    pub(crate) tool: Option<String>,
    #[arg(
        long,
        default_value_t = false,
        help = "Run without full-screen UI rendering."
    )]
    pub(crate) headless: bool,
    #[arg(
        long,
        value_name = "TEXT",
        help = "Optional initial prompt to pre-fill in the UI."
    )]
    pub(crate) prompt: Option<String>,
}
