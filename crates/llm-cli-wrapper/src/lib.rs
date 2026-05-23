//! llm-cli-wrapper — in-tree error types shared with the daemon-side
//! plugin backend adapter (`orchestrator-session-host`).
//!
//! Session DTOs, the `SessionBackend` trait, and the native CLI session
//! backends live in the upstream `animus-session-backend` crate. The
//! launch-contract helpers (`LaunchInvocation`, `ensure_flag*`,
//! `parse_launch_from_runtime_contract`, `is_ai_cli_tool`) moved into
//! `agent-runner::runner::launch`, and binary PATH lookups inline
//! `which::which` at each call site.

pub mod error;

pub use error::{Error, Result};
