use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum LogsCommand {
    /// Tail recent log entries from the active log storage backend.
    ///
    /// Reads from whichever backend is active for this project — the
    /// in-tree `events.jsonl` writer by default, or an installed
    /// `log_storage_backend` plugin when one is discovered. Set
    /// `ANIMUS_DAEMON_DISABLE_LOG_STORAGE_PLUGIN=1` to force the in-tree
    /// fallback even when a plugin is installed.
    Tail(LogsTailArgs),
}

#[derive(Debug, Args)]
pub(crate) struct LogsTailArgs {
    /// Filter entries to the named source plugin. When using the in-tree
    /// fallback this matches against the `provider` field on the
    /// structured log entry.
    #[arg(long, value_name = "NAME")]
    pub plugin: Option<String>,

    /// Minimum log level to include. One of `debug`, `info`, `warn`,
    /// `error`. Defaults to `info`.
    #[arg(long, value_name = "LEVEL", default_value = "info")]
    pub level: String,

    /// Only return entries newer than this duration (e.g. `1h`, `30m`,
    /// `15s`). Defaults to the last 1 hour.
    #[arg(long, value_name = "DURATION", default_value = "1h")]
    pub since: String,

    /// Maximum number of entries to return.
    #[arg(long, default_value_t = 100)]
    pub limit: usize,

    /// Reserved for future streaming support; currently a no-op. When the
    /// active backend is the in-tree fallback `--follow` returns the
    /// requested entries and exits (tail-and-quit). Plugin backends that
    /// advertise streaming will deliver follow notifications via the
    /// stdio protocol; that wiring lands in a v0.4.1+ commit.
    #[arg(long, default_value_t = false)]
    pub follow: bool,
}
