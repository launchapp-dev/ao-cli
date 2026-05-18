use crate::ProjectTickSummary;

#[derive(Debug, Clone)]
pub struct DiscoveredPluginSummary {
    pub name: String,
    pub version: String,
    pub plugin_kind: String,
    pub source: &'static str,
    pub path: String,
}

#[derive(Debug, Clone)]
pub enum DaemonRunEvent {
    Startup {
        project_root: String,
        daemon_pid: u32,
    },
    Status {
        project_root: String,
        status: String,
    },
    StartupCleanup {
        project_root: String,
    },
    PluginsDiscovered {
        project_root: String,
        plugins: Vec<DiscoveredPluginSummary>,
    },
    PluginsDiscoveryFailed {
        project_root: String,
        error: String,
    },
    OrphanDetection {
        project_root: String,
        orphaned_workflows_recovered: usize,
    },
    YamlCompileSucceeded {
        project_root: String,
        source_files: usize,
        output_path: String,
        phase_definitions: usize,
        agent_profiles: usize,
    },
    YamlCompileFailed {
        project_root: String,
        error: String,
    },
    TickSummary {
        summary: ProjectTickSummary,
    },
    TickError {
        project_root: String,
        message: String,
    },
    GracefulShutdown {
        project_root: String,
        timeout_secs: Option<u64>,
    },
    Draining {
        project_root: String,
        trigger: String,
    },
    NotificationRuntimeError {
        project_root: Option<String>,
        stage: String,
        message: String,
    },
    ConfigReloaded {
        project_root: String,
        setting: String,
    },
    TriggerPluginsStarted {
        project_root: String,
        plugin_count: usize,
    },
    TriggerPluginStartFailed {
        project_root: String,
        plugin_name: String,
        error: String,
    },
    TriggerPluginEvent {
        project_root: String,
        plugin_name: String,
        event_id: String,
        trigger_id: Option<String>,
        routed: bool,
    },
    TriggerPluginRestart {
        project_root: String,
        plugin_name: String,
        attempt: u32,
        delay_ms: u64,
    },
    TriggerPluginCrashed {
        project_root: String,
        plugin_name: String,
        attempts: u32,
        error: String,
    },
    /// Resolved which log storage backend the daemon will route through
    /// for this run. Emitted once at startup, after plugin discovery
    /// completes. `plugin_name` is `None` when the in-tree fallback is
    /// active.
    LogStorageDispatchResolved {
        project_root: String,
        plugin_name: Option<String>,
        candidate_count: usize,
        disable_env_set: bool,
        warnings: Vec<String>,
    },
    /// Resolved which subject-backend plugins the daemon will route
    /// `<kind>/<verb>` calls through for this run. Emitted once at
    /// startup, after subject-plugin discovery completes. `plugin_count`
    /// is the number of installed subject_backend plugins that
    /// initialized successfully; `kinds` summarizes the manifest-declared
    /// subject_kind capabilities seen across them. Unlike log storage,
    /// there is no in-tree fallback — when `plugin_count == 0`, every
    /// `animus subject <verb>` call returns `NotFound` with the missing
    /// kind named.
    SubjectRouterResolved {
        project_root: String,
        plugin_count: usize,
        kinds: Vec<String>,
        disable_env_set: bool,
        warnings: Vec<String>,
    },
    Shutdown {
        project_root: String,
        daemon_pid: u32,
    },
}
