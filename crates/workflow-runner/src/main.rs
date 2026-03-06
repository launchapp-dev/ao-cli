use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use tokio::process::Command as TokioCommand;

#[derive(Parser)]
#[command(name = "ao-workflow-runner", about = "Standalone workflow phase runner")]
struct WorkflowRunnerCli {
    #[command(subcommand)]
    command: WorkflowRunnerCommand,
}

#[derive(Subcommand)]
enum WorkflowRunnerCommand {
    Execute(WorkflowExecuteArgs),
}

#[derive(Args)]
struct WorkflowExecuteArgs {
    #[arg(long)]
    task_id: Option<String>,

    #[arg(long)]
    requirement_id: Option<String>,

    #[arg(long)]
    title: Option<String>,

    #[arg(long)]
    description: Option<String>,

    #[arg(long)]
    pipeline: Option<String>,

    #[arg(long)]
    project_root: String,

    #[arg(long)]
    config_path: Option<String>,

    #[arg(long)]
    model: Option<String>,

    #[arg(long)]
    tool: Option<String>,

    #[arg(long)]
    phase_timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct RunnerEvent {
    event: &'static str,
    task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pipeline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
}

fn resolve_ao_binary() -> String {
    if let Ok(path) = std::env::var("AO_BIN") {
        return path;
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let candidate = dir.join("ao");
            if candidate.is_file() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    "ao".to_string()
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = WorkflowRunnerCli::parse();

    match cli.command {
        WorkflowRunnerCommand::Execute(args) => match run_execute(args).await {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("ao-workflow-runner failed: {error}");
                ExitCode::from(1)
            }
        },
    }
}

async fn run_execute(args: WorkflowExecuteArgs) -> Result<u8> {
    let subject_id = args
        .task_id
        .clone()
        .or_else(|| args.requirement_id.clone())
        .or_else(|| args.title.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let startup = RunnerEvent {
        event: "runner_start",
        task_id: subject_id.clone(),
        pipeline: args.pipeline.clone(),
        exit_code: None,
    };
    eprintln!("{}", serde_json::to_string(&startup).unwrap_or_default());

    let ao_bin = resolve_ao_binary();
    let mut command = TokioCommand::new(&ao_bin);
    command
        .arg("--project-root")
        .arg(&args.project_root)
        .arg("workflow")
        .arg("execute")
        .arg("--quiet");

    if let Some(ref task_id) = args.task_id {
        command.arg("--task-id").arg(task_id);
    }
    if let Some(ref requirement_id) = args.requirement_id {
        command.arg("--requirement-id").arg(requirement_id);
    }
    if let Some(ref title) = args.title {
        command.arg("--title").arg(title);
    }
    if let Some(ref description) = args.description {
        command.arg("--description").arg(description);
    }
    if let Some(ref pipeline) = args.pipeline {
        command.arg("--pipeline-id").arg(pipeline);
    }
    if let Some(ref model) = args.model {
        command.arg("--model").arg(model);
    }
    if let Some(ref tool) = args.tool {
        command.arg("--tool").arg(tool);
    }
    if let Some(timeout) = args.phase_timeout_secs {
        command.arg("--phase-timeout-secs").arg(timeout.to_string());
    }

    command.stdout(std::process::Stdio::inherit());
    command.stderr(std::process::Stdio::inherit());

    let status = command
        .status()
        .await
        .with_context(|| format!("failed to spawn ao binary at '{ao_bin}'"))?;

    let exit_code = status.code().unwrap_or(1);

    let completion = RunnerEvent {
        event: "runner_complete",
        task_id: subject_id,
        pipeline: args.pipeline,
        exit_code: Some(exit_code),
    };
    eprintln!(
        "{}",
        serde_json::to_string(&completion).unwrap_or_default()
    );

    Ok(clamp_exit_code(exit_code))
}

fn clamp_exit_code(code: i32) -> u8 {
    match u8::try_from(code) {
        Ok(value) => value,
        Err(_) => {
            if code < 0 {
                1
            } else {
                u8::MAX
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_exit_code_zero() {
        assert_eq!(clamp_exit_code(0), 0);
    }

    #[test]
    fn clamp_exit_code_normal() {
        assert_eq!(clamp_exit_code(1), 1);
        assert_eq!(clamp_exit_code(42), 42);
        assert_eq!(clamp_exit_code(255), 255);
    }

    #[test]
    fn clamp_exit_code_negative() {
        assert_eq!(clamp_exit_code(-1), 1);
        assert_eq!(clamp_exit_code(-128), 1);
    }

    #[test]
    fn clamp_exit_code_overflow() {
        assert_eq!(clamp_exit_code(256), u8::MAX);
        assert_eq!(clamp_exit_code(i32::MAX), u8::MAX);
    }

    #[test]
    fn resolve_ao_binary_respects_env() {
        std::env::set_var("AO_BIN", "/custom/path/ao");
        let result = resolve_ao_binary();
        std::env::remove_var("AO_BIN");
        assert_eq!(result, "/custom/path/ao");
    }

    #[test]
    fn resolve_ao_binary_falls_back_to_path() {
        std::env::remove_var("AO_BIN");
        let result = resolve_ao_binary();
        assert!(!result.is_empty());
    }

    #[test]
    fn source_contains_no_stub_executor() {
        let source = include_str!("main.rs");
        let marker = format!("{}PhaseExecutor", "Stub");
        let count = source.matches(&marker).count();
        assert_eq!(
            count, 0,
            "workflow-runner must not contain {marker} (found {count} occurrences)"
        );
    }

    #[test]
    fn runner_event_serialization() {
        let event = RunnerEvent {
            event: "runner_start",
            task_id: "TASK-001".to_string(),
            pipeline: Some("default".to_string()),
            exit_code: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("runner_start"));
        assert!(json.contains("TASK-001"));
        assert!(!json.contains("exit_code"));

        let complete = RunnerEvent {
            event: "runner_complete",
            task_id: "TASK-001".to_string(),
            pipeline: None,
            exit_code: Some(0),
        };
        let json = serde_json::to_string(&complete).unwrap();
        assert!(json.contains("runner_complete"));
        assert!(json.contains("\"exit_code\":0"));
        assert!(!json.contains("pipeline"));
    }
}
