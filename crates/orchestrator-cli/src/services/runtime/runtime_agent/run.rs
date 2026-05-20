use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use orchestrator_core::services::ServiceHub;
use protocol::{AgentRunEvent, AgentRunRequest, ModelId, RunId, PROTOCOL_VERSION};
use tokio::io::{AsyncBufReadExt, BufReader};
use uuid::Uuid;

use crate::{
    build_agent_context, event_matches_run, persist_agent_event, persist_json_output, print_agent_event, print_value,
    run_dir, write_json_line, AgentRunArgs,
};

use super::connection::connect_runner_for_agent_command;

pub(super) async fn handle_agent_run(
    args: AgentRunArgs,
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    let run_id = RunId(args.run_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()));

    // C6.7: prefer the control wire when daemon is running, --detach,
    // and --json. Detach mode is the only shape that maps cleanly onto
    // the wire's `AgentRunResult` envelope (single response, no
    // streaming). Streaming + non-JSON renders stay on the local path
    // so the rich `AgentRunEvent` stream survives unchanged.
    if json && args.detach {
        if let Some(result) = try_agent_run_via_control(project_root, &args).await? {
            return print_value(
                serde_json::json!({
                    "run_id": result.session_id,
                    "status": "submitted",
                    "model": result.model,
                }),
                true,
            );
        }
    }

    let context = build_agent_context(&args, project_root)?;
    let request = AgentRunRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        run_id: run_id.clone(),
        model: ModelId(args.model.clone().unwrap_or_else(|| {
            protocol::default_model_for_tool(&args.tool).unwrap_or("claude-sonnet-4-6").to_string()
        })),
        context,
        timeout_secs: args.timeout_secs,
    };

    let stream = connect_runner_for_agent_command(&hub, project_root, args.start_runner).await?;
    let (read_half, mut write_half) = tokio::io::split(stream);
    write_json_line(&mut write_half, &request).await?;

    if args.detach {
        return print_value(
            serde_json::json!({
                "run_id": run_id.0,
                "status": "submitted",
            }),
            json,
        );
    }

    let mut lines = BufReader::new(read_half).lines();
    let run_dir = if args.save_jsonl { Some(run_dir(project_root, &run_id, args.jsonl_dir.as_deref())) } else { None };

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let Ok(event) = serde_json::from_str::<AgentRunEvent>(line) else {
            continue;
        };

        if !event_matches_run(&event, &run_id) {
            continue;
        }

        if let Some(path) = &run_dir {
            persist_agent_event(path, &event)?;
            if let AgentRunEvent::OutputChunk { stream_type, text, .. } = &event {
                persist_json_output(path, *stream_type, text)?;
            }
        }

        if args.stream || json {
            print_agent_event(&event, json, &args.tool)?;
        }

        match event {
            AgentRunEvent::Finished { exit_code, .. } => {
                if !args.stream && !json {
                    println!("run {} finished (exit_code={:?})", run_id.0, exit_code);
                }
                if exit_code.unwrap_or_default() != 0 {
                    return Err(anyhow!("agent run exited with code {:?}", exit_code));
                }
                return Ok(());
            }
            AgentRunEvent::Error { error, .. } => return Err(anyhow!(error)),
            _ => {}
        }
    }

    Err(anyhow!("runner connection closed before run {} completed", run_id.0))
}

// =====================================================================
// C6.7 — control-wire routing helper for agent/run
// =====================================================================
//
// Opens the control socket (returns Ok(None) when the daemon isn't
// running so the caller falls back to the local in-process runner
// path), issues the wire-shaped `agent/run` JSON-RPC call, and returns
// the wire response. The daemon-side `AgentRouting` impl is currently
// a pass-through stub (see `ops_agent::control_routing`) so this helper
// will most often return `Ok(None)` via the not-supported degradation —
// the wire surface is in place for MCP (C7) and WebAPI (C8) to swap in
// a real implementation without changing the CLI call sites.

async fn try_agent_run_via_control(
    project_root: &str,
    args: &AgentRunArgs,
) -> Result<Option<animus_control_protocol::types::AgentRunResult>> {
    use crate::services::control_client::{is_method_unavailable, ControlClient};
    use animus_control_protocol::types::AgentRunRequest as WireRequest;

    let project_root_path = Path::new(project_root);
    let Some(client) = ControlClient::try_connect(project_root_path).await? else {
        return Ok(None);
    };
    let request = WireRequest {
        provider: args.tool.clone(),
        model: args
            .model
            .clone()
            .unwrap_or_else(|| protocol::default_model_for_tool(&args.tool).unwrap_or("claude-sonnet-4-6").to_string()),
        prompt: args.prompt.clone().unwrap_or_default(),
        system: None,
        cwd: args.cwd.as_ref().map(std::path::PathBuf::from),
        env: Default::default(),
    };
    match client.agent_run(request).await {
        Ok(response) => Ok(Some(response)),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "agent/run wire returned unavailable; falling back to local runner");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}
