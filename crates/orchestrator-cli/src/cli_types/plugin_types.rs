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
    /// Install a plugin binary from a local path or URL into ~/.animus/plugins/.
    Install(PluginInstallArgs),
    /// Remove a previously installed plugin from ~/.animus/plugins/ and plugins.yaml.
    Uninstall(PluginUninstallArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PluginInstallArgs {
    /// Local path to the plugin binary to install.
    #[arg(long, value_name = "PATH", group = "source")]
    pub(crate) path: Option<String>,
    /// URL to download the plugin binary from (https only).
    #[arg(long, value_name = "URL", group = "source")]
    pub(crate) url: Option<String>,
    /// Optional logical plugin name. Defaults to the binary file name.
    #[arg(long, value_name = "NAME")]
    pub(crate) name: Option<String>,
    /// Expected SHA256 hex digest. Required when installing from `--url`;
    /// optional when installing from `--path`. The install fails if the
    /// downloaded/copied binary's checksum does not match.
    #[arg(long, value_name = "HEX")]
    pub(crate) sha256: Option<String>,
    /// Overwrite an existing installed plugin with the same name.
    #[arg(long, default_value_t = false)]
    pub(crate) force: bool,
    /// Skip running `--manifest` against the installed binary to verify it.
    #[arg(long, default_value_t = false)]
    pub(crate) skip_manifest_check: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PluginUninstallArgs {
    #[arg(long, value_name = "NAME", help = "Logical plugin name to uninstall.")]
    pub(crate) name: String,
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
