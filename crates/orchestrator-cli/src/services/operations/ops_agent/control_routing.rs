//! CLI-side `AgentRouting` adapter — bridges the daemon's control
//! surface back to the in-tree agent-runner helpers.
//!
//! C6.7 of the v0.4.0 controller-as-plugin migration. The siblings are
//! [`crate::services::operations::ops_workflow::control_routing`]
//! (workflow/*), [`crate::services::operations::ops_queue::control_routing`]
//! (queue/*), and [`crate::services::operations::ops_plugin::control_routing`]
//! (plugin/*).
//!
//! ## Path A — pass-through
//!
//! C5's report noted that the daemon-side `AgentPool` carries
//! `#[allow(dead_code)]` and has no clean `Arc`-shared query surface yet.
//! Rather than refactor `AgentPool` (out of scope for the controller
//! migration), this adapter returns `NotSupported` for every method.
//! That preserves the historical pre-C6.7 behavior on the wire path —
//! CLI callers degrade to the local in-process implementation under
//! `runtime_agent::{run, status}` — while still mounting the routing
//! trait on the control surface. MCP (C7) and WebAPI (C8) can swap in a
//! real `AgentPool`-backed implementation without changing the wire
//! contract.
//!
//! See `docs/architecture/naming-contract.md` for the broader migration
//! plan; see [this module's tests in `runtime_agent`] for the local
//! fallback verification.
//!
//! ## Anti-deadlock notes
//!
//! - The struct is empty; no internal mutex, no shared state.
//! - Every method is `&self` and returns immediately — no `.await`
//!   suspensions inside the routing layer.

use std::path::PathBuf;
use std::sync::Arc;

use animus_control_protocol::{
    types::{
        AgentCancelRequest as WireCancelRequest, AgentRunRequest as WireRunRequest, AgentRunResult as WireRunResult,
        AgentStatus as WireAgentStatus, AgentStatusRequest as WireStatusRequest, Unit,
    },
    ControlError,
};
use async_trait::async_trait;
use orchestrator_daemon_runtime::control::AgentRouting;

/// Build an [`AgentRouting`] handle bound to `project_root`.
///
/// C6.7 lands the wire surface as a pass-through stub: each method
/// returns [`ControlError::NotSupported`] so CLI callers fall back to
/// the local in-process path under `runtime_agent`. The `project_root`
/// argument is captured for symmetry with `build_queue_routing` /
/// `build_workflow_routing` so future implementations (C7/C8) can swap
/// in a real `AgentPool`-backed surface without changing the call site
/// at daemon startup.
pub fn build_agent_routing(project_root: PathBuf) -> Arc<dyn AgentRouting> {
    Arc::new(AgentRoutingImpl { _project_root: project_root })
}

struct AgentRoutingImpl {
    // Reserved for the v0.4.x follow-up that wires `AgentPool` through
    // here. Kept as a struct field rather than a free function so the
    // signature of `build_agent_routing` stays stable when the impl
    // grows. Allowed to be unread today.
    _project_root: PathBuf,
}

#[async_trait]
impl AgentRouting for AgentRoutingImpl {
    async fn agent_run(&self, _request: WireRunRequest) -> Result<WireRunResult, ControlError> {
        // The daemon-side AgentPool is currently `allow(dead_code)` per
        // the existing memory note; there is no clean Arc query surface
        // to wire through yet. Return NotSupported so CLI callers
        // degrade to the local in-process path under runtime_agent.
        Err(ControlError::NotSupported(
            "agent/run wire surface is pass-through pending AgentPool query surface; CLI falls back to local"
                .to_string(),
        ))
    }

    async fn agent_status(&self, _request: WireStatusRequest) -> Result<WireAgentStatus, ControlError> {
        Err(ControlError::NotSupported(
            "agent/status wire surface is pass-through pending AgentPool query surface; CLI falls back to local"
                .to_string(),
        ))
    }

    async fn agent_cancel(&self, _request: WireCancelRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported(
            "agent/cancel wire surface is pass-through pending AgentPool query surface; CLI falls back to local"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructor smoke test — building the routing handle should
    /// always succeed; the project_root is held for the future
    /// `AgentPool`-backed impl.
    #[tokio::test]
    async fn build_returns_handle() {
        let routing = build_agent_routing(PathBuf::from("/tmp/c67-test"));
        let err = routing
            .agent_run(WireRunRequest {
                provider: "claude".to_string(),
                model: "claude-sonnet-4-6".to_string(),
                prompt: "hi".to_string(),
                system: None,
                cwd: None,
                env: Default::default(),
            })
            .await
            .unwrap_err();
        match err {
            ControlError::NotSupported(_) => {}
            other => panic!("expected NotSupported, got {other:?}"),
        }
    }
}
