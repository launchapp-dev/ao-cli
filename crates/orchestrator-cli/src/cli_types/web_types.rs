use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum WebCommand {
    /// Spawn installed transport_backend + web_ui plugins and report bound URLs.
    /// Requires plugins from `animus plugin install-defaults --include-transports`.
    Serve(WebServeArgs),
    /// Open the Animus web UI URL in a browser. Resolves the URL from an
    /// installed web_ui or transport_backend plugin unless --url is supplied.
    Open(WebOpenArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WebServeArgs {
    /// Open the resolved web UI URL in a browser after the transport plugins start.
    #[arg(long, default_value_t = false)]
    pub(crate) open: bool,
}

#[derive(Debug, Args)]
pub(crate) struct WebOpenArgs {
    /// Override the resolved URL. When set, the installed plugins are not consulted.
    #[arg(long, value_name = "URL")]
    pub(crate) url: Option<String>,
    /// Sub-path appended to the resolved URL, such as `/runs`. Ignored when `--url` is set.
    #[arg(long, value_name = "PATH", default_value = "/")]
    pub(crate) path: String,
}
