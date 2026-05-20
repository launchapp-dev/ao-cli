//! Daemon-side control RPC server.
//!
//! C5 of the v0.4.0 controller-as-plugin migration. The daemon exposes a
//! Unix-domain socket at `~/.animus/<repo-scope>/control.sock` (0700 perms)
//! that speaks the [`animus_control_protocol`] wire format —
//! newline-delimited JSON-RPC 2.0 frames.
//!
//! Layout:
//!
//! - [`server`] — [`ControlServer`] binds the socket, runs an accept loop,
//!   spawns per-connection tasks. Owns the broadcast channel used to fan
//!   out daemon events to streaming subscribers.
//! - [`connection`] — [`ControlConnection`] is the per-client JSON-RPC
//!   dispatch loop. Handles request/response framing and streaming
//!   notifications.
//! - [`dispatch`] — [`InProcessSurface`] implements
//!   [`animus_control_protocol::ControlSurface`] by translating each
//!   protocol method into an existing in-process service call (mostly
//!   [`SubjectPluginDispatch::route_call`] for subject ops; daemon status
//!   surfaces direct from process state).
//! - [`streaming`] — broadcast subscriber adapters that turn the daemon's
//!   internal event channels into the protocol stream types.
//!
//! # Auth
//!
//! Filesystem permissions (mode 0700 on the socket file). Anything that
//! can `open(2)` the socket is trusted. Stronger auth (per-client
//! capability tokens, signed handshakes) is deferred to v0.4.x.
//!
//! # Opt-out
//!
//! Setting `ANIMUS_DAEMON_DISABLE_CONTROL_SERVER` to a truthy value
//! (anything that isn't `0`/`false`/`no`/`off`/empty) keeps the daemon
//! from starting the server. Useful for tests of the in-process fallback
//! path while C6/C7/C8 land.

pub mod connection;
pub mod dispatch;
pub mod server;
pub mod streaming;

#[cfg(test)]
mod tests;

pub use connection::ControlConnection;
pub use dispatch::InProcessSurface;
pub use server::{
    control_server_disable_env_set, control_socket_path, ControlServer, ControlServerHandle, CONTROL_SERVER_DISABLE_ENV,
};
pub use streaming::{DaemonEventBus, DaemonLogBus};
