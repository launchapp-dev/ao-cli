//! llm-cli-wrapper — launch-invocation helpers and in-tree error variants
//! consumed by `agent-runner`, `orchestrator-cli`, and the daemon-side
//! plugin backend adapter.
//!
//! Session DTOs, the `SessionBackend` trait, and the native CLI session
//! backends now live in the upstream `animus-session-backend` crate.

pub mod cli;
pub mod error;

pub use cli::{
    codex_exec_insert_index_json, ensure_codex_config_override, ensure_codex_config_override_json, ensure_flag,
    ensure_flag_value, ensure_flag_value_json, ensure_machine_json_output, is_ai_cli_tool, is_binary_on_path,
    launch_prompt_insert_index_json, lookup_binary_in_path, parse_cli_type, parse_launch_from_runtime_contract,
    CliCapability, CliStatus, CliType, LaunchInvocation,
};
pub use error::{Error, Result};
