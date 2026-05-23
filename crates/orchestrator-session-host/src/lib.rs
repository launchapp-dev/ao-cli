//! Host-side glue that wires the upstream `SessionBackend` trait (re-exported from
//! `animus_session_backend::session`) to the AO STDIO plugin protocol.
//!
//! This crate exists to keep the published `animus-session-backend` crate
//! small. The pieces here — `PluginSessionBackend` (per-session map + cancel
//! routing + notification forwarding) and the plugin-discovery-aware
//! `SessionBackendResolver` — are daemon-side adapters that depend on the
//! `orchestrator-plugin-host` transport. They have no business shipping in
//! the published session-backend surface that provider plugins themselves
//! consume.
//!
//! The native CLI backends (Claude, Codex, Gemini, OpenCode, OAI-runner) and
//! the trait itself live in upstream `animus-session-backend`; this crate
//! just re-routes through them via a richer resolver.

pub mod error;
pub mod plugin_backend;
pub mod plugin_supervisor;
pub mod session_backend_resolver;

pub use error::{Error, Result};
pub use plugin_backend::{discover_provider_plugins, DiscoveredProviderPlugin, PluginSessionBackend};
pub use plugin_supervisor::{
    is_death_like_error, is_structured_jsonrpc_error, PluginSupervisor, SupervisorConfig, SupervisorError,
};
pub use session_backend_resolver::{is_reserved_provider_tool, SessionBackendResolver, RESERVED_PROVIDER_TOOLS};
