mod render;
mod state;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use orchestrator_core::services::ServiceHub;
use protocol::{
    AgentRunEvent, OutputStreamType as ProtocolOutputStreamType, RunId, ToolResultInfo,
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::services::operations::get_run_jsonl_entries;
use crate::WorkflowMonitorArgs;
use state::{OutputStreamType, WorkflowMonitorState};

pub(crate) async fn handle_workflow_monitor(
    args: WorkflowMonitorArgs,
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    if json {
        return Err(anyhow!(
            "`ao workflow-monitor` does not support --json output"
        ));
    }

    let refresh_interval = Duration::from_secs(args.refresh_interval);
    let mut state = WorkflowMonitorState::new(args.buffer_lines);

    match hub.workflows().list().await {
        Ok(workflows) => {
            state.workflows = if let Some(ref id) = args.workflow_id {
                workflows.into_iter().filter(|w| &w.id == id).collect()
            } else {
                workflows
            };
            state.status_line = format!("{} workflow(s) loaded", state.workflows.len());
        }
        Err(e) => {
            state.status_line = format!("Failed to load workflows: {e}");
        }
    }

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = run_event_loop(
        &mut terminal,
        &mut state,
        &hub,
        project_root,
        refresh_interval,
        args.workflow_id.as_deref(),
    )
    .await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut WorkflowMonitorState,
    hub: &Arc<dyn ServiceHub>,
    project_root: &str,
    refresh_interval: Duration,
    workflow_id_filter: Option<&str>,
) -> Result<()> {
    let mut last_refresh = Instant::now();
    let mut last_output_poll = Instant::now();

    loop {
        terminal.draw(|frame| render::render(frame, state))?;

        if event::poll(Duration::from_millis(100))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if state.filter_mode {
                match key.code {
                    KeyCode::Esc => {
                        state.filter_mode = false;
                    }
                    KeyCode::Enter => {
                        state.filter_mode = false;
                        state.clamp_selection();
                    }
                    KeyCode::Backspace => {
                        state.filter.pop();
                        state.clamp_selection();
                    }
                    KeyCode::Char(ch) => {
                        state.filter.push(ch);
                        state.clamp_selection();
                    }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.move_up();
                        state.detach_output();
                        state.push_output(
                            "[ selection changed ]".to_string(),
                            OutputStreamType::System,
                        );
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.move_down();
                        state.detach_output();
                        state.push_output(
                            "[ selection changed ]".to_string(),
                            OutputStreamType::System,
                        );
                    }
                    KeyCode::Enter => {
                        attach_selected_workflow_output(state, project_root).await;
                    }
                    KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        state.clear_output();
                    }
                    KeyCode::Char('r') => {
                        refresh_workflows(state, hub, workflow_id_filter).await;
                        last_refresh = Instant::now();
                    }
                    KeyCode::Char('/') => {
                        state.filter_mode = true;
                    }
                    KeyCode::Char('s') => {
                        state.scroll_lock = !state.scroll_lock;
                    }
                    KeyCode::PageUp => {
                        for _ in 0..10 {
                            state.scroll_up();
                        }
                    }
                    KeyCode::PageDown => {
                        let max = state.output_buffer.len();
                        for _ in 0..10 {
                            state.scroll_down(max);
                        }
                    }
                    _ => {}
                }
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            refresh_workflows(state, hub, workflow_id_filter).await;
            last_refresh = Instant::now();
        }

        if last_output_poll.elapsed() >= Duration::from_millis(250) {
            sync_attached_output(state, project_root).await;
            last_output_poll = Instant::now();
        }
    }

    Ok(())
}

async fn refresh_workflows(
    state: &mut WorkflowMonitorState,
    hub: &Arc<dyn ServiceHub>,
    workflow_id_filter: Option<&str>,
) {
    match hub.workflows().list().await {
        Ok(workflows) => {
            state.workflows = if let Some(id) = workflow_id_filter {
                workflows.into_iter().filter(|w| w.id == id).collect()
            } else {
                workflows
            };
            state.last_refresh = chrono::Utc::now();
            state.clamp_selection();
            state.status_line = format!("{} workflow(s)", state.workflows.len());
        }
        Err(e) => {
            state.status_line = format!("Refresh failed: {e}");
            state.push_output(
                format!("[ Workflow refresh failed: {e} ]"),
                OutputStreamType::System,
            );
        }
    }
}

async fn attach_selected_workflow_output(state: &mut WorkflowMonitorState, project_root: &str) {
    let Some((workflow_id, current_phase)) = state
        .selected_workflow()
        .map(|workflow| (workflow.id.clone(), workflow.current_phase.clone()))
    else {
        state.status_line = "No workflow selected".to_string();
        return;
    };

    state.detach_output();
    state.attached_workflow_id = Some(workflow_id.clone());
    state.attached_phase_id = current_phase.clone();
    state.push_output(
        format!(
            "[ attaching to workflow {} phase {} ]",
            workflow_id,
            current_phase.as_deref().unwrap_or("none")
        ),
        OutputStreamType::System,
    );
    state.status_line = format!("attaching to workflow {workflow_id}");
    sync_attached_output(state, project_root).await;
}

async fn sync_attached_output(state: &mut WorkflowMonitorState, project_root: &str) {
    let Some(workflow_id) = state.attached_workflow_id.clone() else {
        return;
    };

    let Some((run_id, entries)) = resolve_workflow_output_entries(project_root, &workflow_id)
    else {
        state.status_line = format!("waiting for run output for {workflow_id}");
        return;
    };

    let attached_run_changed = state.attached_run_id.as_deref() != Some(run_id.as_str());
    if attached_run_changed {
        state.clear_output();
        state.attached_run_id = Some(run_id.clone());
        state.attached_entry_count = 0;
        state.push_output(
            format!("[ attached run {} ]", run_id),
            OutputStreamType::System,
        );
    }

    if entries.len() < state.attached_entry_count {
        state.attached_entry_count = 0;
    }

    for entry in entries.iter().skip(state.attached_entry_count) {
        let (text, stream_type) = normalize_output_entry(entry);
        if text.is_empty() {
            continue;
        }
        state.push_output(text, stream_type);
    }
    state.attached_entry_count = entries.len();
    state.status_line = format!(
        "attached to {}",
        state.attached_run_id.as_deref().unwrap_or("")
    );
}

fn resolve_workflow_output_entries(
    project_root: &str,
    workflow_id: &str,
) -> Option<(String, Vec<crate::services::operations::RunJsonlEntryCli>)> {
    let (run_id, _) = resolve_latest_workflow_run_dir(project_root, workflow_id).ok()??;
    let entries = get_run_jsonl_entries(project_root, run_id.as_str()).ok()?;
    Some((run_id, entries))
}

fn normalize_output_entry(
    entry: &crate::services::operations::RunJsonlEntryCli,
) -> (String, OutputStreamType) {
    match entry.source_file.as_str() {
        "stdout.jsonl" => (entry.line.clone(), OutputStreamType::Stdout),
        "stderr.jsonl" => (entry.line.clone(), OutputStreamType::Stderr),
        "system.jsonl" | "signals.jsonl" => (entry.line.clone(), OutputStreamType::System),
        "events.jsonl" => normalize_event_line(entry.line.as_str()),
        _ => (entry.line.clone(), OutputStreamType::Stdout),
    }
}

fn normalize_event_line(line: &str) -> (String, OutputStreamType) {
    let Ok(event) = serde_json::from_str::<AgentRunEvent>(line) else {
        return (line.to_string(), OutputStreamType::System);
    };

    match event {
        AgentRunEvent::OutputChunk {
            text, stream_type, ..
        } => (
            text,
            match stream_type {
                ProtocolOutputStreamType::Stdout => OutputStreamType::Stdout,
                ProtocolOutputStreamType::Stderr => OutputStreamType::Stderr,
                ProtocolOutputStreamType::System => OutputStreamType::System,
            },
        ),
        AgentRunEvent::Error { error, .. } => {
            (format!("[error] {error}"), OutputStreamType::Stderr)
        }
        AgentRunEvent::Thinking { content, .. } => {
            (format!("[thinking] {content}"), OutputStreamType::System)
        }
        AgentRunEvent::Started { run_id, .. } => (
            format!("[run started] {}", run_id.0),
            OutputStreamType::System,
        ),
        AgentRunEvent::Metadata {
            cost,
            tokens,
            run_id,
        } => (
            format!("[metadata] {} cost={cost:?} tokens={tokens:?}", run_id.0),
            OutputStreamType::System,
        ),
        AgentRunEvent::Finished {
            run_id, exit_code, ..
        } => (
            format!("[run finished] {} exit_code={exit_code:?}", run_id.0),
            OutputStreamType::System,
        ),
        AgentRunEvent::ToolCall { tool_info, .. } => (
            format!("[tool call] {}", tool_info.tool_name),
            OutputStreamType::System,
        ),
        AgentRunEvent::ToolResult { result_info, .. } => {
            (format_tool_result(result_info), OutputStreamType::System)
        }
        AgentRunEvent::Artifact { artifact_info, .. } => (
            format!("[artifact] {}", artifact_info.artifact_id),
            OutputStreamType::System,
        ),
    }
}

fn format_tool_result(result_info: ToolResultInfo) -> String {
    format!(
        "[tool result] {} success={}",
        result_info.tool_name, result_info.success
    )
}

fn resolve_latest_workflow_run_dir(
    project_root: &str,
    workflow_id: &str,
) -> Result<Option<(String, std::path::PathBuf)>> {
    use crate::{ensure_safe_run_id, run_dir};
    use anyhow::Context;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::UNIX_EPOCH;

    fn runs_root_candidates(project_root: &str) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(scoped_parent) = run_dir(
            project_root,
            &RunId("workflow-monitor-root-probe".to_string()),
            None,
        )
        .parent()
        {
            candidates.push(scoped_parent.to_path_buf());
        }
        candidates.push(Path::new(project_root).join(".ao").join("runs"));
        candidates.push(
            Path::new(project_root)
                .join(".ao")
                .join("state")
                .join("runs"),
        );
        candidates.dedup();
        candidates
    }

    fn path_modified_millis(path: &Path) -> u128 {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    }

    let mut run_ids = BTreeSet::new();
    let prefix = format!("wf-{workflow_id}-");
    for runs_root in runs_root_candidates(project_root) {
        if !runs_root.exists() {
            continue;
        }
        for entry in fs::read_dir(&runs_root)
            .with_context(|| format!("failed to read run directory {}", runs_root.display()))?
        {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            if !name.starts_with(prefix.as_str()) || ensure_safe_run_id(name.as_str()).is_err() {
                continue;
            }
            run_ids.insert(name);
        }
    }

    let mut candidates = Vec::new();
    for run_id in run_ids {
        let Some(run_dir) =
            crate::services::operations::resolve_run_dir_for_lookup(project_root, run_id.as_str())?
        else {
            continue;
        };
        let events_path = run_dir.join("events.jsonl");
        let has_events = events_path.exists();
        let modified_millis = if has_events {
            path_modified_millis(events_path.as_path())
        } else {
            path_modified_millis(run_dir.as_path())
        };
        candidates.push((run_id, run_dir, has_events, modified_millis));
    }

    candidates.sort_by(|left, right| {
        right
            .2
            .cmp(&left.2)
            .then_with(|| right.3.cmp(&left.3))
            .then_with(|| left.0.cmp(&right.0))
    });

    Ok(candidates
        .into_iter()
        .next()
        .map(|(run_id, run_dir, _, _)| (run_id, run_dir)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{OutputStreamType as ProtocolStream, Timestamp};

    #[test]
    fn normalize_output_chunk_preserves_stream_type() {
        let line = serde_json::to_string(&AgentRunEvent::OutputChunk {
            run_id: RunId("run-1".to_string()),
            stream_type: ProtocolStream::Stderr,
            text: "boom".to_string(),
        })
        .expect("event should serialize");

        let (text, stream_type) = normalize_event_line(&line);
        assert_eq!(text, "boom");
        assert!(matches!(stream_type, OutputStreamType::Stderr));
    }

    #[test]
    fn normalize_tool_result_is_human_readable() {
        let line = serde_json::to_string(&AgentRunEvent::ToolResult {
            run_id: RunId("run-1".to_string()),
            result_info: ToolResultInfo {
                tool_name: "ao_task_get".to_string(),
                result: serde_json::json!({"ok": true}),
                duration_ms: 12,
                success: true,
            },
        })
        .expect("event should serialize");

        let (text, stream_type) = normalize_event_line(&line);
        assert_eq!(text, "[tool result] ao_task_get success=true");
        assert!(matches!(stream_type, OutputStreamType::System));
    }

    #[test]
    fn normalize_started_event_is_human_readable() {
        let line = serde_json::to_string(&AgentRunEvent::Started {
            run_id: RunId("run-2".to_string()),
            timestamp: Timestamp::now(),
        })
        .expect("event should serialize");

        let (text, stream_type) = normalize_event_line(&line);
        assert_eq!(text, "[run started] run-2");
        assert!(matches!(stream_type, OutputStreamType::System));
    }
}
