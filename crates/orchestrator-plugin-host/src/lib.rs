//! Stdio hosting, discovery, and routing for AO-compatible plugins.

mod discovery;
mod host;
mod registry;
mod subject_router;
mod transport;

pub use discovery::{discover_plugins, DiscoveredPlugin, DiscoverySource, PluginConfigEntry, PluginDiscovery};
pub use host::PluginHost;
pub use registry::PluginRegistry;
pub use subject_router::SubjectRouter;
pub use transport::StdioTransport;
