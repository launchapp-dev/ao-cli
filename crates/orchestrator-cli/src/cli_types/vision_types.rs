use clap::{Args, Subcommand};

use super::INPUT_JSON_PRECEDENCE_HELP;

#[derive(Debug, Subcommand)]
pub(crate) enum VisionCommand {
    /// Draft project vision.
    #[command(hide = true)]
    Draft(VisionDraftArgs),
    /// Refine project vision.
    #[command(hide = true)]
    Refine(VisionRefineArgs),
}

#[derive(Debug, Args)]
pub(crate) struct VisionDraftArgs {
    #[arg(long, value_name = "JSON", help = INPUT_JSON_PRECEDENCE_HELP)]
    pub(crate) input_json: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct VisionRefineArgs {
    #[arg(long, value_name = "JSON", help = INPUT_JSON_PRECEDENCE_HELP)]
    pub(crate) input_json: Option<String>,
}
