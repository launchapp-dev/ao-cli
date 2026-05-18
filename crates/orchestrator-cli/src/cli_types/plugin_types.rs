use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum PluginCommand {
    /// Discover plugins on PATH, in `.animus/plugins/`, in `$ANIMUS_PLUGIN_PATH`, and via plugins.yaml.
    List(PluginListArgs),
    /// Print a plugin's manifest plus initialize-time capabilities.
    Info(PluginInfoArgs),
    /// Send a JSON-RPC request to a plugin and print its response.
    Call(PluginCallArgs),
    /// Health-check a plugin by spawning it, completing the handshake, and pinging.
    Ping(PluginPingArgs),
    /// Install a plugin binary from a public GitHub release (OWNER/REPO[@TAG]),
    /// a local path, or a direct URL into ~/.animus/plugins/ (override with
    /// --plugin-dir or $ANIMUS_PLUGIN_DIR).
    Install(PluginInstallArgs),
    /// Remove a previously installed plugin from ~/.animus/plugins/ (override
    /// with --plugin-dir or $ANIMUS_PLUGIN_DIR) and ~/.animus/plugins.yaml.
    Uninstall(PluginUninstallArgs),
    /// Scaffold a new plugin project from the launchapp-dev/animus-plugin-template scaffold.
    New(PluginNewArgs),
    /// Search the public Animus plugin registry by substring + filters.
    Search(PluginSearchArgs),
    /// Browse the public Animus plugin registry, grouped by kind.
    Browse(PluginBrowseArgs),
    /// Update one or all installed release-source plugins to the latest tag.
    Update(PluginUpdateArgs),
}

/// Default URL for the public Animus plugin registry index.
pub(crate) const DEFAULT_PLUGIN_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/launchapp-dev/animus-plugin-registry/main/plugins.json";

#[derive(Debug, Args)]
pub(crate) struct PluginSearchArgs {
    /// Optional substring query matched against plugin name and description (case-insensitive).
    #[arg(value_name = "QUERY")]
    pub(crate) query: Option<String>,
    /// Filter by plugin kind (e.g. `provider`, `subject_backend`, `trigger`).
    #[arg(long, value_name = "KIND")]
    pub(crate) kind: Option<String>,
    /// Filter by tag (repeatable, ANDed).
    #[arg(long, value_name = "TAG")]
    pub(crate) tag: Vec<String>,
    /// Filter by the repo owner (e.g. `launchapp-dev`).
    #[arg(long, value_name = "ORG")]
    pub(crate) org: Option<String>,
    /// Filter by stability marker (e.g. `alpha`, `beta`, `stable`).
    #[arg(long, value_name = "STABILITY")]
    pub(crate) stability: Option<String>,
    /// Emit results as JSON.
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    /// Override the registry URL. Defaults to launchapp-dev/animus-plugin-registry main.
    #[arg(long, value_name = "URL", default_value = DEFAULT_PLUGIN_REGISTRY_URL)]
    pub(crate) registry_url: String,
    /// Bypass the local registry cache and force a fresh fetch.
    #[arg(long, default_value_t = false)]
    pub(crate) no_cache: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginBrowseArgs {
    /// Filter by plugin kind (e.g. `provider`, `subject_backend`, `trigger`).
    #[arg(long, value_name = "KIND")]
    pub(crate) kind: Option<String>,
    /// Only show plugins that are currently installed locally.
    #[arg(long, default_value_t = false, conflicts_with = "available")]
    pub(crate) installed: bool,
    /// Only show plugins that are NOT yet installed locally.
    #[arg(long, default_value_t = false, conflicts_with = "installed")]
    pub(crate) available: bool,
    /// Emit results as JSON.
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    /// Override the registry URL.
    #[arg(long, value_name = "URL", default_value = DEFAULT_PLUGIN_REGISTRY_URL)]
    pub(crate) registry_url: String,
    /// Bypass the local registry cache and force a fresh fetch.
    #[arg(long, default_value_t = false)]
    pub(crate) no_cache: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginUpdateArgs {
    /// Optional plugin name. When omitted, all installed release-source plugins are updated.
    #[arg(value_name = "NAME")]
    pub(crate) name: Option<String>,
    /// Pin to a specific tag instead of resolving the latest release.
    #[arg(long, value_name = "TAG")]
    pub(crate) tag: Option<String>,
    /// Show what would change without performing the install.
    #[arg(long, default_value_t = false)]
    pub(crate) dry_run: bool,
    /// Emit results as JSON.
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
    /// Force reinstall even if the installed tag matches the target tag.
    #[arg(long, default_value_t = false)]
    pub(crate) force: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginInstallArgs {
    /// Public GitHub repo slug to install from (e.g.
    /// `launchapp-dev/animus-provider-claude` or
    /// `launchapp-dev/animus-provider-claude@v0.1.0`). Resolves the matching
    /// release asset for the current platform. Mutually exclusive with
    /// `--path` and `--url`.
    #[arg(value_name = "OWNER/REPO[@TAG]", group = "install_source")]
    pub(crate) source: Option<String>,
    /// Local path to the plugin binary to install.
    #[arg(long, value_name = "PATH", group = "install_source")]
    pub(crate) path: Option<String>,
    /// URL to download the plugin binary from (https only).
    #[arg(long, value_name = "URL", group = "install_source")]
    pub(crate) url: Option<String>,
    /// Release tag to install when using the positional OWNER/REPO. Defaults
    /// to the latest release. Conflicts with `@tag` syntax on the positional.
    #[arg(long, value_name = "TAG")]
    pub(crate) tag: Option<String>,
    /// Explicitly opt in to the latest release (this is the default behavior).
    /// Conflicts with `--tag`.
    #[arg(long, default_value_t = false, conflicts_with = "tag")]
    pub(crate) latest: bool,
    /// Optional logical plugin name. Defaults to the binary file name.
    #[arg(long, value_name = "NAME")]
    pub(crate) name: Option<String>,
    /// Expected SHA256 hex digest. Required when installing from `--url`;
    /// optional otherwise. The install fails if the downloaded/copied binary's
    /// checksum does not match.
    #[arg(long, value_name = "HEX")]
    pub(crate) sha256: Option<String>,
    /// Overwrite an existing installed plugin with the same name.
    #[arg(long, default_value_t = false)]
    pub(crate) force: bool,
    /// Skip running `--manifest` against the installed binary to verify it.
    #[arg(long, default_value_t = false)]
    pub(crate) skip_manifest_check: bool,
    /// Override the plugin install directory. Takes precedence over
    /// `$ANIMUS_PLUGIN_DIR`. Defaults to `~/.animus/plugins/`.
    #[arg(long, value_name = "PATH")]
    pub(crate) plugin_dir: Option<String>,
    /// Refuse to install when no cosign signature bundle is present or when
    /// signature verification fails. Defaults to false (verify-if-present).
    #[arg(long, default_value_t = false, conflicts_with = "skip_signature")]
    pub(crate) require_signature: bool,
    /// Skip cosign signature verification entirely (escape hatch).
    #[arg(long, default_value_t = false)]
    pub(crate) skip_signature: bool,
    /// Path to a `trusted-signers.yaml` allowlist. Defaults to
    /// `~/.animus/trusted-signers.yaml`.
    #[arg(long, value_name = "PATH")]
    pub(crate) trusted_signers: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct PluginUninstallArgs {
    #[arg(long, value_name = "NAME", help = "Logical plugin name to uninstall.")]
    pub(crate) name: String,
    /// Override the plugin install directory. Takes precedence over
    /// `$ANIMUS_PLUGIN_DIR`. Defaults to `~/.animus/plugins/`.
    #[arg(long, value_name = "PATH")]
    pub(crate) plugin_dir: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct PluginListArgs {
    #[arg(long, default_value_t = false, help = "Also scan $PATH for animus-provider-* and animus-plugin-* binaries.")]
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginInfoArgs {
    #[arg(long, value_name = "NAME", help = "Plugin name (matches the discovered manifest or filename).")]
    pub(crate) name: String,
    #[arg(long, default_value_t = false, help = "Also scan $PATH while resolving the plugin.")]
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginCallArgs {
    #[arg(long, value_name = "NAME", help = "Plugin name to dispatch the request to.")]
    pub(crate) name: String,
    #[arg(long, value_name = "METHOD", help = "JSON-RPC method, e.g. agent/run, mcp/tool_call, or task/list.")]
    pub(crate) method: String,
    #[arg(
        long,
        value_name = "JSON",
        help = "Optional JSON params object. When omitted, the request is sent without a params field."
    )]
    pub(crate) params: Option<String>,
    #[arg(long, default_value_t = false, help = "Also scan $PATH while resolving the plugin.")]
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginPingArgs {
    #[arg(long, value_name = "NAME", help = "Plugin name to spawn and ping.")]
    pub(crate) name: String,
    #[arg(long, default_value_t = false, help = "Also scan $PATH while resolving the plugin.")]
    pub(crate) include_system_path: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginNewArgs {
    /// Plugin kind (subject, provider, trigger).
    #[arg(long, value_name = "KIND")]
    pub(crate) kind: String,

    /// Plugin short name (kebab-case, e.g. jira).
    #[arg(long, value_name = "NAME")]
    pub(crate) name: String,

    /// GitHub org used in the generated project's repository field.
    #[arg(long, value_name = "ORG", default_value = "launchapp-dev")]
    pub(crate) org: String,

    /// Short description for the plugin. Defaults to "An Animus <kind> backend plugin".
    #[arg(long, value_name = "TEXT")]
    pub(crate) description: Option<String>,

    /// Output directory. Defaults to ./animus-<kind>-<name>.
    #[arg(long, value_name = "PATH")]
    pub(crate) out_dir: Option<PathBuf>,

    /// Template git ref (branch or tag) to clone.
    #[arg(long, value_name = "REF", default_value = "main")]
    pub(crate) template_version: String,

    /// Template git URL. Defaults to launchapp-dev/animus-plugin-template.
    #[arg(long, value_name = "URL", default_value = "https://github.com/launchapp-dev/animus-plugin-template")]
    pub(crate) template_repo: String,

    /// Use a local checkout of the template repo instead of running `git clone`.
    #[arg(long, value_name = "PATH")]
    pub(crate) template_path: Option<PathBuf>,

    /// Override the existing output directory if it already exists.
    #[arg(long, default_value_t = false)]
    pub(crate) force: bool,
}
