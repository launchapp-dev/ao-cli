use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum PluginCommand {
    /// Discover plugins on PATH, in `.ao/plugins/`, in `$AO_PLUGIN_PATH`, and via plugins.yaml.
    List(PluginListArgs),
    /// Print a plugin's manifest plus initialize-time capabilities.
    Info(PluginInfoArgs),
    /// Send a JSON-RPC request to a plugin and print its response.
    Call(PluginCallArgs),
    /// Health-check a plugin by spawning it, completing the handshake, and pinging.
    Ping(PluginPingArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PluginListArgs {
    #[arg(long, default_value_t = false, help = "Also scan $PATH for ao-provider-* and ao-plugin-* binaries.")]
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
