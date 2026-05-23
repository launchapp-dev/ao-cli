//! llm-cli-wrapper — session backend layer for Animus CLI-backed agents.
//!
//! This crate hosts the canonical session backend contract, the in-tree
//! subprocess session backends used by `animus-provider-*` plugins, and the
//! launch-invocation helpers consumed by `agent-runner` and `orchestrator-cli`.

pub mod cli;
pub mod error;
pub mod parser;
pub mod session;

pub use cli::{
    codex_exec_insert_index_json, ensure_codex_config_override, ensure_codex_config_override_json, ensure_flag,
    ensure_flag_value, ensure_flag_value_json, ensure_machine_json_output, is_ai_cli_tool, is_binary_on_path,
    launch_prompt_insert_index_json, lookup_binary_in_path, parse_cli_type, parse_launch_from_runtime_contract,
    CliCapability, CliStatus, CliType, LaunchInvocation,
};
pub use error::{Error, Result};
pub use parser::{extract_text_from_line, NormalizedTextEvent};
pub use session::{
    ClaudeSessionBackend, CodexSessionBackend, GeminiSessionBackend, SessionBackend, SessionBackendInfo,
    SessionBackendKind, SessionCapabilities, SessionEvent, SessionRequest, SessionRun, SessionStability,
    SubprocessSessionBackend,
};
