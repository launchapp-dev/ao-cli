use anyhow::{Context, Result};
use orchestrator_daemon_runtime::ScheduleDispatch as RuntimeScheduleDispatch;

use crate::services::runtime::runtime_daemon::daemon_process_manager::{
    ProcessManager, WorkflowSubjectArgs,
};

pub(super) struct ScheduleDispatch;

impl ScheduleDispatch {
    pub(super) fn allows_proactive_dispatch(
        active_hours: Option<&str>,
        now: chrono::NaiveTime,
    ) -> bool {
        RuntimeScheduleDispatch::allows_proactive_dispatch(active_hours, now)
    }

    pub(super) fn process_due_schedules(
        process_manager: &mut ProcessManager,
        project_root: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        RuntimeScheduleDispatch::process_due_schedules(
            project_root,
            now,
            |schedule_id, pipeline_id, input_json| {
                spawn_schedule_pipeline(
                    process_manager,
                    project_root,
                    schedule_id,
                    pipeline_id,
                    input_json,
                )
            },
            |schedule_id, command| spawn_schedule_command(project_root, schedule_id, command),
        );
    }

    pub(super) fn update_completion_state(project_root: &str, schedule_id: &str, status: &str) {
        RuntimeScheduleDispatch::update_completion_state(project_root, schedule_id, status);
    }
}

fn spawn_schedule_pipeline(
    process_manager: &mut ProcessManager,
    project_root: &str,
    schedule_id: &str,
    pipeline_id: &str,
    input_json: Option<&str>,
) -> Result<()> {
    let subject = WorkflowSubjectArgs::Custom {
        title: format!("schedule:{schedule_id}"),
        description: Some(format!("Triggered by schedule '{schedule_id}'")),
        input_json: input_json.map(String::from),
    };
    process_manager.spawn_workflow_runner(&subject, pipeline_id, project_root)?;

    eprintln!(
        "{}: schedule '{}' fired pipeline '{}'",
        protocol::ACTOR_DAEMON,
        schedule_id,
        pipeline_id
    );
    Ok(())
}

fn spawn_schedule_command(project_root: &str, schedule_id: &str, command: &str) -> Result<()> {
    use tokio::process::Command as TokioCommand;

    let mut child = TokioCommand::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(project_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn schedule command for '{schedule_id}'"))?;

    eprintln!(
        "{}: schedule '{}' fired command: {}",
        protocol::ACTOR_DAEMON,
        schedule_id,
        command
    );

    let root = project_root.to_string();
    let sched_id = schedule_id.to_string();
    tokio::spawn(async move {
        let status = match child.wait().await {
            Ok(exit) if exit.success() => "completed",
            Ok(_) => "failed",
            Err(_) => "failed",
        };
        ScheduleDispatch::update_completion_state(&root, &sched_id, status);
    });

    Ok(())
}
