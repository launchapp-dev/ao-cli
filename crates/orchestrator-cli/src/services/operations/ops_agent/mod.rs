//! CLI-side agent operations module — sibling of the in-tree
//! `runtime_agent` handlers.
//!
//! C6.7 of the v0.4.0 controller-as-plugin migration lands the control-
//! wire routing surface for `agent/*` here. The bulk of the agent
//! command implementations still live under
//! `crate::services::runtime::runtime_agent`; this module hosts the
//! `AgentRouting` adapter the daemon hands to the control surface plus
//! the convenience helpers CLI handlers call before falling back to the
//! local in-process path.
//!
//! ## Why a separate adapter module
//!
//! Sibling pattern from C6 / C6.5 / C6.6: each command group exposes a
//! `build_<group>_routing(project_root)` constructor that returns an
//! `Arc<dyn Routing>`. The CLI's `CliDaemonRunHost` collects one of each
//! at daemon startup and hands them to the daemon-runtime's
//! `InProcessSurface`. Tests substitute mock routings in-process without
//! standing up the full transport stack.

mod control_routing;

pub(crate) use control_routing::build_agent_routing;
