use std::path::Path;

use crate::{not_found_error, print_ok, print_value, ScheduleCommand};
use anyhow::{Context, Result};
use chrono::Utc;
use orchestrator_core::{load_schedule_state, load_workflow_config};
use orchestrator_daemon_runtime::{
    build_runner_command_from_dispatch, ScheduleDispatch as RuntimeScheduleDispatch,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ScheduleListEntry {
    id: String,
    cron: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pipeline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    enabled: bool,
    last_run: Option<String>,
}

#[derive(Debug, Serialize)]
struct ScheduleHistoryEntry {
    id: String,
    last_run: Option<String>,
    last_status: String,
    run_count: u64,
}

pub(crate) async fn handle_schedule(
    command: ScheduleCommand,
    _hub: std::sync::Arc<dyn orchestrator_core::services::ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    match command {
        ScheduleCommand::List => {
            let schedule_config = load_workflow_config(Path::new(project_root))
                .with_context(|| format!("failed to load workflow config from {}", project_root))?;
            let schedule_state = load_schedule_state(Path::new(project_root))
                .with_context(|| format!("failed to load schedule state from {}", project_root))?;
            let rows = schedule_config
                .schedules
                .iter()
                .map(|schedule| {
                    let run_state = schedule_state.schedules.get(&schedule.id);
                    ScheduleListEntry {
                        id: schedule.id.clone(),
                        cron: schedule.cron.clone(),
                        pipeline: schedule.pipeline.clone(),
                        command: schedule.command.clone(),
                        enabled: schedule.enabled,
                        last_run: run_state
                            .and_then(|state| state.last_run)
                            .map(|value| value.to_rfc3339()),
                    }
                })
                .collect::<Vec<_>>();

            if json {
                print_value(rows, json)
            } else {
                print_schedule_table(&rows);
                Ok(())
            }
        }
        ScheduleCommand::Fire { id } => {
            let schedule_config = load_workflow_config(Path::new(project_root))
                .with_context(|| format!("failed to load workflow config from {}", project_root))?;
            if !schedule_config
                .schedules
                .iter()
                .any(|schedule| schedule.id == id)
            {
                return Err(not_found_error(format!("schedule not found: {id}")));
            }

            fire_schedule(project_root, &id)?;
            print_ok(&format!("fired schedule: {id}"), json);
            Ok(())
        }
        ScheduleCommand::History { id } => {
            let schedule_config = load_workflow_config(Path::new(project_root))
                .with_context(|| format!("failed to load workflow config from {}", project_root))?;
            let schedule_state = load_schedule_state(Path::new(project_root))
                .with_context(|| format!("failed to load schedule state from {}", project_root))?;
            if !schedule_config
                .schedules
                .iter()
                .any(|schedule| schedule.id == id)
            {
                return Err(not_found_error(format!("schedule not found: {id}")));
            }

            let run_state = schedule_state
                .schedules
                .get(&id)
                .cloned()
                .unwrap_or_default();
            let history = ScheduleHistoryEntry {
                id,
                last_run: run_state.last_run.map(|value| value.to_rfc3339()),
                last_status: run_state.last_status,
                run_count: run_state.run_count,
            };
            print_value(history, json)
        }
    }
}

fn fire_schedule(project_root: &str, schedule_id: &str) -> Result<()> {
    use std::process::Stdio;

    RuntimeScheduleDispatch::fire_schedule(
        project_root,
        schedule_id,
        Utc::now(),
        "manual-schedule-fire",
        |schedule_id, dispatch| {
            let mut cmd = build_runner_command_from_dispatch(dispatch, project_root);
            cmd.stdout(Stdio::null()).stderr(Stdio::inherit());
            let child = cmd.spawn().with_context(|| {
                format!(
                    "failed to spawn ao-workflow-runner for schedule '{}'",
                    schedule_id
                )
            })?;
            spawn_completion_writeback(project_root, schedule_id, child);
            Ok(())
        },
        |schedule_id, command| {
            use std::process::Command as StdCommand;
            let child = StdCommand::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(project_root)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .with_context(|| {
                    format!("failed to spawn command for schedule '{}'", schedule_id)
                })?;
            spawn_completion_writeback(project_root, schedule_id, child);
            Ok(())
        },
    )?;

    Ok(())
}

fn spawn_completion_writeback(
    project_root: &str,
    schedule_id: &str,
    mut child: std::process::Child,
) {
    let root = project_root.to_string();
    let sched_id = schedule_id.to_string();
    std::thread::spawn(move || {
        let status = match child.wait() {
            Ok(exit) if exit.success() => "completed",
            Ok(_) => "failed",
            Err(_) => "failed",
        };
        let path = std::path::Path::new(&root);
        let mut state = orchestrator_core::load_schedule_state(path).unwrap_or_default();
        if let Some(entry) = state.schedules.get_mut(&sched_id) {
            entry.last_status = status.to_string();
            let _ = orchestrator_core::save_schedule_state(path, &state);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use orchestrator_core::{builtin_workflow_config, load_schedule_state, write_workflow_config};
    use tempfile::tempdir;

    use super::fire_schedule;

    fn wait_for_schedule_status(project_root: &std::path::Path, schedule_id: &str) -> String {
        for _ in 0..50 {
            let state = load_schedule_state(project_root).expect("schedule state should load");
            if let Some(entry) = state.schedules.get(schedule_id) {
                if entry.last_status != "dispatched" {
                    return entry.last_status.clone();
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        load_schedule_state(project_root)
            .expect("schedule state should load")
            .schedules
            .get(schedule_id)
            .map(|entry| entry.last_status.clone())
            .unwrap_or_default()
    }

    #[test]
    fn manual_fire_updates_command_schedule_to_completed() {
        let temp = tempdir().expect("tempdir should be created");
        let project_root = temp.path();
        let mut config = builtin_workflow_config();
        config.schedules.push(orchestrator_core::WorkflowSchedule {
            id: "nightly".to_string(),
            cron: "30 12 * * *".to_string(),
            pipeline: None,
            command: Some("exit 0".to_string()),
            input: None,
            enabled: true,
        });
        write_workflow_config(project_root, &config).expect("workflow config should be written");

        fire_schedule(project_root.to_string_lossy().as_ref(), "nightly")
            .expect("manual schedule fire should dispatch");

        let status = wait_for_schedule_status(project_root, "nightly");
        assert_eq!(status, "completed");
    }

    #[test]
    fn manual_fire_updates_command_schedule_to_failed() {
        let temp = tempdir().expect("tempdir should be created");
        let project_root = temp.path();
        let mut config = builtin_workflow_config();
        config.schedules.push(orchestrator_core::WorkflowSchedule {
            id: "nightly".to_string(),
            cron: "30 12 * * *".to_string(),
            pipeline: None,
            command: Some("exit 1".to_string()),
            input: None,
            enabled: true,
        });
        write_workflow_config(project_root, &config).expect("workflow config should be written");

        fire_schedule(project_root.to_string_lossy().as_ref(), "nightly")
            .expect("manual schedule fire should dispatch");

        let status = wait_for_schedule_status(project_root, "nightly");
        assert_eq!(status, "failed");
    }
}

fn print_schedule_table(rows: &[ScheduleListEntry]) {
    let headers = ["id", "cron", "pipeline", "command", "enabled", "last_run"];
    let mut widths = headers
        .iter()
        .map(|header| header.len())
        .collect::<Vec<_>>();

    for row in rows {
        widths[0] = widths[0].max(row.id.len());
        widths[1] = widths[1].max(row.cron.len());
        widths[2] = widths[2].max(row.pipeline.as_deref().unwrap_or("-").len());
        widths[3] = widths[3].max(row.command.as_deref().unwrap_or("-").len());
        widths[4] = widths[4].max(row.enabled.to_string().len());
        widths[5] = widths[5].max(row.last_run.as_deref().unwrap_or("-").len());
    }

    let header = format!(
        "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  {:<w5$}",
        headers[0],
        headers[1],
        headers[2],
        headers[3],
        headers[4],
        headers[5],
        w0 = widths[0],
        w1 = widths[1],
        w2 = widths[2],
        w3 = widths[3],
        w4 = widths[4],
        w5 = widths[5]
    );
    println!("{}", header);
    println!("{}", "-".repeat(header.len()));

    for row in rows {
        println!(
            "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {:<w4$}  {:<w5$}",
            row.id,
            row.cron,
            row.pipeline.as_deref().unwrap_or("-"),
            row.command.as_deref().unwrap_or("-"),
            row.enabled,
            row.last_run.as_deref().unwrap_or("-"),
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3],
            w4 = widths[4],
            w5 = widths[5]
        );
    }
}
