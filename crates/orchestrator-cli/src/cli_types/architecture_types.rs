use clap::{Args, Subcommand};

use super::IdArgs;

#[derive(Debug, Subcommand)]
pub(crate) enum ArchitectureCommand {
    /// Read architecture graph and metadata.
    Get,
    /// Replace architecture graph JSON.
    Set(ArchitectureSetArgs),
    /// Suggest architecture links for a task.
    Suggest(ArchitectureSuggestArgs),
    /// Manage architecture entities.
    Entity {
        #[command(subcommand)]
        command: ArchitectureEntityCommand,
    },
    /// Manage architecture edges.
    Edge {
        #[command(subcommand)]
        command: ArchitectureEdgeCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ArchitectureSetArgs {
    #[arg(long)]
    pub(crate) input_json: String,
}

#[derive(Debug, Args)]
pub(crate) struct ArchitectureSuggestArgs {
    #[arg(long)]
    pub(crate) task_id: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ArchitectureEntityCommand {
    /// List architecture entities.
    List,
    /// Get an architecture entity by id.
    Get(IdArgs),
    /// Create an architecture entity.
    Create(ArchitectureEntityCreateArgs),
    /// Update an architecture entity.
    Update(ArchitectureEntityUpdateArgs),
    /// Delete an architecture entity.
    Delete(IdArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ArchitectureEntityCreateArgs {
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) kind: Option<String>,
    #[arg(long)]
    pub(crate) description: Option<String>,
    #[arg(long = "code-path")]
    pub(crate) code_path: Vec<String>,
    #[arg(long = "tag")]
    pub(crate) tag: Vec<String>,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ArchitectureEntityUpdateArgs {
    #[arg(long)]
    pub(crate) id: String,
    #[arg(long)]
    pub(crate) name: Option<String>,
    #[arg(long)]
    pub(crate) kind: Option<String>,
    #[arg(long)]
    pub(crate) description: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) clear_description: bool,
    #[arg(long = "code-path")]
    pub(crate) code_path: Vec<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) replace_code_paths: bool,
    #[arg(long = "tag")]
    pub(crate) tag: Vec<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) replace_tags: bool,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ArchitectureEdgeCommand {
    /// List architecture edges.
    List,
    /// Create an architecture edge.
    Create(ArchitectureEdgeCreateArgs),
    /// Delete an architecture edge.
    Delete(IdArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ArchitectureEdgeCreateArgs {
    #[arg(long)]
    pub(crate) id: Option<String>,
    #[arg(long)]
    pub(crate) from: String,
    #[arg(long)]
    pub(crate) to: String,
    #[arg(long)]
    pub(crate) relation: String,
    #[arg(long)]
    pub(crate) rationale: Option<String>,
    #[arg(long)]
    pub(crate) input_json: Option<String>,
}
