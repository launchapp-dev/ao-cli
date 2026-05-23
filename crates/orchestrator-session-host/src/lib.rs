//! Host-side glue that wires the in-tree `SessionBackend` trait (re-exported from
//! `cli_wrapper::session`) to the AO STDIO plugin protocol.
//!
//! This crate exists to keep the eventual published `animus-session-backend`
//! crate small. The pieces here — `PluginSessionBackend` (per-session map +
//! cancel routing + notification forwarding) and the plugin-discovery-aware
//! `SessionBackendResolver` — are daemon-side adapters that depend on the
//! `orchestrator-plugin-host` transport. They have no business shipping in
//! the published session-backend surface that provider plugins themselves
//! consume.
//!
//! The native CLI backends (Claude, Codex, Gemini, OpenCode, OAI-runner) and
//! the trait itself still live in `llm-cli-wrapper::session`; this crate just
//! re-routes through them via a richer resolver.

pub mod plugin_backend;
pub mod session_backend_resolver;

pub use plugin_backend::{discover_provider_plugins, DiscoveredProviderPlugin, PluginSessionBackend};
pub use session_backend_resolver::{is_reserved_provider_tool, SessionBackendResolver, RESERVED_PROVIDER_TOOLS};
