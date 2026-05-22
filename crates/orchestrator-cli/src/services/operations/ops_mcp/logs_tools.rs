//! `animus.logs.*` MCP tools.
//!
//! v0.4.7 Item 2: surface the CLI's `animus logs tail` through MCP so
//! agents can pull the daemon's log tail without shelling out. Mirrors
//! the subject_tools pattern — typed input struct, args builder,
//! `run_tool` shell-out — so the wire/local fallback logic in
//! `ops_logs::handle_logs_tail` is shared between CLI and MCP callers.

use super::*;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct LogsTailInput {
    /// Filter entries to the named source plugin (matches the `provider`
    /// field on each structured entry). Omit to include every emitter.
    #[serde(default)]
    pub(super) plugin: Option<String>,
    /// Minimum severity. One of `debug`, `info`, `warn`, `error`. Defaults
    /// to `info` when omitted.
    #[serde(default)]
    pub(super) level: Option<String>,
    /// Only return entries newer than this duration. Accepts `1h`, `30m`,
    /// `15s`, `2d`. Defaults to `1h` when omitted.
    #[serde(default)]
    pub(super) since: Option<String>,
    /// Maximum number of entries to return. Defaults to 100 when omitted.
    #[serde(default)]
    pub(super) limit: Option<u32>,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

pub(super) fn build_logs_tail_args(input: &LogsTailInput) -> Vec<String> {
    let mut args = vec!["logs".to_string(), "tail".to_string()];
    push_opt(&mut args, "--plugin", input.plugin.clone());
    push_opt(&mut args, "--level", input.level.clone());
    push_opt(&mut args, "--since", input.since.clone());
    if let Some(limit) = input.limit {
        args.push("--limit".to_string());
        args.push(limit.to_string());
    }
    args
}

#[tool_router(router = logs_tool_router, vis = "pub(super)")]
impl AoMcpServer {
    #[tool(
        name = "animus.logs.tail",
        description = "Tail recent daemon and plugin log entries through the active log_storage_backend. Purpose: Inspect what the daemon and its supervised plugins have been logging without shelling out. Prerequisites: None — falls back to the in-tree events.jsonl reader when the daemon is not running. Example: {\"limit\": 25} or {\"level\": \"warn\", \"plugin\": \"kimi-code\", \"since\": \"30m\"}. Sequencing: Use animus.daemon.status to confirm the daemon is up if you want the wire transport instead of the local fallback.",
        input_schema = ao_schema_for_type::<LogsTailInput>()
    )]
    async fn ao_logs_tail(&self, params: Parameters<LogsTailInput>) -> Result<CallToolResult, McpError> {
        let input = params.0;
        let project_root = input.project_root.clone();
        let args = build_logs_tail_args(&input);
        self.run_tool("animus.logs.tail", args, project_root).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_args_defaults_to_logs_tail() {
        let input = LogsTailInput { plugin: None, level: None, since: None, limit: None, project_root: None };
        let args = build_logs_tail_args(&input);
        assert_eq!(args, vec!["logs", "tail"]);
    }

    #[test]
    fn tail_args_includes_optional_flags_when_provided() {
        let input = LogsTailInput {
            plugin: Some("kimi-code".to_string()),
            level: Some("warn".to_string()),
            since: Some("30m".to_string()),
            limit: Some(25),
            project_root: None,
        };
        let args = build_logs_tail_args(&input);
        assert_eq!(
            args,
            vec!["logs", "tail", "--plugin", "kimi-code", "--level", "warn", "--since", "30m", "--limit", "25"]
        );
    }

    #[test]
    fn tail_args_project_root_does_not_leak_into_cli_args() {
        // project_root is consumed by run_tool's working-dir override, not
        // emitted as a CLI flag; the args list should be free of it.
        let input = LogsTailInput {
            plugin: None,
            level: None,
            since: None,
            limit: None,
            project_root: Some("/tmp/somewhere".to_string()),
        };
        let args = build_logs_tail_args(&input);
        assert!(!args.iter().any(|a| a.contains("/tmp/somewhere")));
    }
}
