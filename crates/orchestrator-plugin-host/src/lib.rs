//! Stdio hosting, discovery, and routing for AO-compatible plugins.

mod discovery;
mod host;
mod registry;
mod subject_router;
mod transport;

pub use discovery::{
    discover_plugins, legacy_plugins_registry_path, plugin_install_dir, plugins_registry_path,
    registered_skip_manifest_check_at_install, DiscoveredPlugin, DiscoverySource, DiscoveryWarning, PluginConfigEntry,
    PluginDiscovery,
};
pub use host::{
    check_protocol_compat, HostError, PluginHost, PluginHostInner, PluginNotificationRx, PluginSpawnOptions,
    PluginStderrSink, DEFAULT_NOTIFICATION_BROADCAST_CAPACITY, NOTIFICATION_BROADCAST_CAPACITY_ENV,
    PLUGIN_BASE_ENV_ALLOWLIST,
};
pub use registry::PluginRegistry;
pub use subject_router::SubjectRouter;
pub use transport::StdioTransport;
