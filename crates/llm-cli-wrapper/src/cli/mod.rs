//! Launch-invocation helpers and shared CLI type definitions consumed by
//! `agent-runner` and `orchestrator-cli`.

pub mod launch;
pub mod types;

pub use launch::{
    codex_exec_insert_index_json, ensure_codex_config_override, ensure_codex_config_override_json, ensure_flag,
    ensure_flag_value, ensure_flag_value_json, ensure_machine_json_output, is_ai_cli_tool, is_binary_on_path,
    launch_prompt_insert_index_json, lookup_binary_in_path, parse_cli_type, parse_launch_from_runtime_contract,
    LaunchInvocation,
};
pub use types::{CliCapability, CliMetadata, CliStatus, CliType};
