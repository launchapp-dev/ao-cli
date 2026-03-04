use std::path::Path;

use crate::{not_found_error, print_ok, print_value, ScheduleCommand};
use anyhow::{Context, Result};
use chrono::Utc;
use orchestrator_core::{load_schedule_state, load_workflow_config, save_schedule_state};
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
    let schedule_config = load_workflow_config(Path::new(project_root))
        .with_context(|| format!("failed to load workflow config from {}", project_root))?;
    let mut schedule_state = load_schedule_state(Path::new(project_root))
        .with_context(|| format!("failed to load schedule state from {}", project_root))?;

    match command {
        ScheduleCommand::List => {
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
            if !schedule_config.schedules.iter().any(|schedule| schedule.id == id) {
                return Err(not_found_error(format!("schedule not found: {id}")));
            }

            let run_state = schedule_state
                .schedules
                .entry(id.clone())
                .or_insert_with(Default::default);
            run_state.last_run = Some(Utc::now());
            run_state.last_status = "fired".to_string();
            run_state.run_count = run_state.run_count.saturating_add(1);

            save_schedule_state(Path::new(project_root), &schedule_state)
                .with_context(|| format!("failed to save schedule state to {project_root}"))?;
            print_ok(&format!("fired schedule: {id}"), json);
            Ok(())
        }
        ScheduleCommand::History { id } => {
            if !schedule_config.schedules.iter().any(|schedule| schedule.id == id) {
                return Err(not_found_error(format!("schedule not found: {id}")));
            }

            let run_state = schedule_state.schedules.get(&id).cloned().unwrap_or_default();
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

fn print_schedule_table(rows: &[ScheduleListEntry]) {
    let headers = [
        "id",
        "cron",
        "pipeline",
        "command",
        "enabled",
        "last_run",
    ];
    let mut widths = headers.iter().map(|header| header.len()).collect::<Vec<_>>();

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
