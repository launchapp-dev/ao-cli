//! Routing traits that the daemon's [`super::InProcessSurface`] uses to
//! delegate plugin/* and daemon/* operations back to in-tree CLI logic.
//!
//! Architectural background
//! ------------------------
//!
//! The daemon and the CLI live in the same Rust binary today (`animus`).
//! The CLI's `ops_plugin::run_plugin_*` and `runtime_daemon::*` helpers
//! already implement every operation the control wire needs. C6 wires
//! the daemon's [`super::InProcessSurface`] to *call those same helpers*
//! when a control request arrives over the socket — rather than
//! duplicating the logic.
//!
//! To keep the daemon-runtime crate free of a circular dependency on the
//! orchestrator-cli crate, the wiring goes through these `Arc<dyn Trait>`
//! handles. The CLI binary builds one of each at daemon startup, hands
//! them to the [`super::InProcessSurfaceBuilder`], and the surface
//! delegates blindly. Tests can substitute mock implementations for both
//! routing-via-wire and routing-via-local paths.
//!
//! Anti-deadlock rules
//! -------------------
//!
//! - All methods are `&self` + `Send`. Implementors must not hold a lock
//!   across `.await`; the trait surface awaits each method exactly once.
//! - Handles are `Arc`-cloned at injection, never wrapped in
//!   `tokio::sync::Mutex` by the surface. Implementations are responsible
//!   for any internal synchronization they need.

use async_trait::async_trait;
use serde_json::Value;

use animus_control_protocol::{
    types::{
        DaemonAgentsResponse, DaemonHealthResponse, DaemonStatusResponse, PluginBrowseRequest, PluginCallRequest,
        PluginCallResponse, PluginInfo, PluginInfoRequest, PluginInstallRequest, PluginInstallResponse,
        PluginListRequest, PluginListResponse, PluginPingRequest, PluginPingResponse, PluginSearchRequest,
        PluginSearchResponse, PluginUninstallRequest, PluginUpdateRequest, PluginUpdateResponse, Unit,
    },
    ControlError,
};

/// Plugin/* dispatcher used by [`super::InProcessSurface`].
///
/// Wraps the CLI's existing `run_plugin_*` helpers behind a transport-
/// agnostic interface. The daemon owns one Arc'd instance for its lifetime;
/// the CLI binary constructs it at startup and hands it in via
/// [`super::InProcessSurfaceBuilder::plugin_routing`].
#[async_trait]
pub trait PluginRouting: Send + Sync {
    /// `plugin/list` — enumerate installed plugins.
    async fn plugin_list(&self, request: PluginListRequest) -> Result<PluginListResponse, ControlError>;

    /// `plugin/info` — describe one installed plugin.
    async fn plugin_info(&self, request: PluginInfoRequest) -> Result<PluginInfo, ControlError>;

    /// `plugin/install` — fetch + install a plugin.
    async fn plugin_install(&self, request: PluginInstallRequest) -> Result<PluginInstallResponse, ControlError>;

    /// `plugin/uninstall` — remove an installed plugin.
    async fn plugin_uninstall(&self, request: PluginUninstallRequest) -> Result<Unit, ControlError>;

    /// `plugin/ping` — lifecycle-ping into a plugin.
    async fn plugin_ping(&self, request: PluginPingRequest) -> Result<PluginPingResponse, ControlError>;

    /// `plugin/call` — opaque pass-through to a plugin domain method.
    async fn plugin_call(&self, request: PluginCallRequest) -> Result<PluginCallResponse, ControlError>;

    /// `plugin/search` — registry free-text search.
    async fn plugin_search(&self, request: PluginSearchRequest) -> Result<PluginSearchResponse, ControlError>;

    /// `plugin/browse` — registry listing by kind / install state.
    async fn plugin_browse(&self, request: PluginBrowseRequest) -> Result<PluginSearchResponse, ControlError>;

    /// `plugin/update` — check / apply plugin upgrades.
    async fn plugin_update(&self, request: PluginUpdateRequest) -> Result<PluginUpdateResponse, ControlError>;
}

/// `daemon/*` dispatcher used by [`super::InProcessSurface`] for the
/// observability surface (`status`, `health`, `agents`).
///
/// `daemon/start`, `daemon/stop`, and `daemon/restart` remain CLI-local —
/// the daemon controlling itself over its own socket is the wrong model.
#[async_trait]
pub trait DaemonOpsRouting: Send + Sync {
    /// `daemon/status` — live process status snapshot.
    async fn daemon_status(&self) -> Result<DaemonStatusResponse, ControlError>;

    /// `daemon/health` — health snapshot incl. per-plugin health.
    async fn daemon_health(&self) -> Result<DaemonHealthResponse, ControlError>;

    /// `daemon/agents` — currently active agent sessions.
    async fn daemon_agents(&self) -> Result<DaemonAgentsResponse, ControlError>;
}

/// Marker used by integration tests that need to assert "the surface
/// received this exact JSON params payload" without spinning up a full
/// transport stack.
///
/// Not used in production code paths.
#[derive(Debug, Clone, Default)]
pub struct LastCallSpy {
    /// JSON-RPC method that was last invoked, if any.
    pub method: Option<String>,
    /// Params payload from the last invocation.
    pub params: Option<Value>,
}
