use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub(crate) enum SessionCommand {
    /// List active oai-runner sessions with metadata.
    List,
}
