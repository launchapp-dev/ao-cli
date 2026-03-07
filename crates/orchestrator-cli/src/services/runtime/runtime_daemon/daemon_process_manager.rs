use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RunnerEvent {
    pub event: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub pipeline: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub enum WorkflowSubjectArgs {
    Task { task_id: String },
    Requirement { requirement_id: String },
    Custom { title: String, description: Option<String>, input_json: Option<String> },
}

impl WorkflowSubjectArgs {
    pub fn subject_id(&self) -> &str {
        match self {
            Self::Task { task_id } => task_id,
            Self::Requirement { requirement_id } => requirement_id,
            Self::Custom { title, .. } => title,
        }
    }

    pub fn schedule_id(&self) -> Option<&str> {
        match self {
            Self::Custom { title, .. } => title.strip_prefix("schedule:"),
            _ => None,
        }
    }

    pub fn build_runner_command(&self, pipeline_id: &str, project_root: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("ao-workflow-runner");
        cmd.arg("execute");

        match self {
            Self::Task { task_id } => {
                cmd.arg("--task-id").arg(task_id);
            }
            Self::Requirement { requirement_id } => {
                cmd.arg("--requirement-id").arg(requirement_id);
            }
            Self::Custom { title, description, input_json } => {
                cmd.arg("--title").arg(title);
                if let Some(desc) = description {
                    cmd.arg("--description").arg(desc);
                }
                if let Some(json) = input_json {
                    cmd.env("AO_SCHEDULE_INPUT", json);
                }
            }
        }

        cmd.arg("--pipeline").arg(pipeline_id)
            .arg("--project-root").arg(project_root);
        cmd
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowProcess {
    pub subject_id: String,
    pub task_id: Option<String>,
    pub schedule_id: Option<String>,
    pub child: Arc<Mutex<Child>>,
    pub stderr_lines: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug)]
pub struct CompletedProcess {
    pub subject_id: String,
    pub task_id: Option<String>,
    pub schedule_id: Option<String>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub failure_reason: Option<String>,
    pub events: Vec<RunnerEvent>,
}

#[derive(Default)]
pub struct ProcessManager {
    processes: Vec<WorkflowProcess>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Vec::new(),
        }
    }

    pub fn spawn_workflow_runner(
        &mut self,
        subject: &WorkflowSubjectArgs,
        pipeline_id: &str,
        project_root: &str,
    ) -> Result<WorkflowProcess> {
        let std_cmd = subject.build_runner_command(pipeline_id, project_root);
        let mut command = Command::from(std_cmd);
        command.stdout(Stdio::null()).stderr(Stdio::piped());

        let mut child = command.spawn().context("failed to spawn ao-workflow-runner")?;

        let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        if let Some(stderr) = child.stderr.take() {
            let lines = stderr_lines.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stderr);
                let mut line_stream = reader.lines();
                while let Ok(Some(line)) = line_stream.next_line().await {
                    if let Ok(mut buf) = lines.lock() {
                        buf.push(line);
                    }
                }
            });
        }

        let task_id = match subject {
            WorkflowSubjectArgs::Task { task_id } => Some(task_id.clone()),
            _ => None,
        };
        let schedule_id = subject.schedule_id().map(String::from);

        let process = WorkflowProcess {
            subject_id: subject.subject_id().to_string(),
            task_id,
            schedule_id,
            child: Arc::new(Mutex::new(child)),
            stderr_lines,
        };

        self.processes.push(process.clone());

        Ok(process)
    }

    pub fn check_running(&mut self) -> Vec<CompletedProcess> {
        let mut completed = Vec::new();
        let mut active = Vec::with_capacity(self.processes.len());

        for process in self.processes.drain(..) {
            let status = {
                let mut maybe_child = match process.child.lock() {
                    Ok(guard) => guard,
                    Err(error) => {
                        completed.push(CompletedProcess {
                            subject_id: process.subject_id,
                            task_id: process.task_id,
                            schedule_id: process.schedule_id,
                            exit_code: None,
                            success: false,
                            failure_reason: Some(format!(
                                "failed to lock workflow process handle: {}",
                                error
                            )),
                            events: Vec::new(),
                        });
                        continue;
                    }
                };

                maybe_child.try_wait()
            };

            match status {
                Ok(Some(status)) => {
                    let exit_code = status.code();
                    let (success, failure_reason) = if status.success() {
                        (true, None)
                    } else {
                        (
                            false,
                            Some(format!(
                                "workflow runner exited unsuccessfully with status {:?}",
                                exit_code
                            )),
                        )
                    };

                    let events = parse_runner_events(&process.stderr_lines);

                    completed.push(CompletedProcess {
                        subject_id: process.subject_id,
                        task_id: process.task_id,
                        schedule_id: process.schedule_id,
                        exit_code,
                        success,
                        failure_reason,
                        events,
                    });
                }
                Ok(None) => {
                    active.push(process);
                }
                Err(error) => {
                    completed.push(CompletedProcess {
                        subject_id: process.subject_id,
                        task_id: process.task_id,
                        schedule_id: process.schedule_id,
                        exit_code: None,
                        success: false,
                        failure_reason: Some(format!("failed to probe workflow process status: {}", error)),
                        events: Vec::new(),
                    });
                }
            }
        }

        self.processes = active;
        completed
    }

    pub fn active_count(&self) -> usize {
        self.processes.len()
    }
}

fn parse_runner_events(stderr_lines: &Arc<Mutex<Vec<String>>>) -> Vec<RunnerEvent> {
    let lines = match stderr_lines.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return Vec::new(),
    };
    lines
        .iter()
        .filter_map(|line| serde_json::from_str::<RunnerEvent>(line).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::test_utils::EnvVarGuard;
    use std::env;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn new_process_manager_starts_empty() {
        let manager = ProcessManager::new();
        assert_eq!(manager.active_count(), 0);
    }

    #[tokio::test]
    async fn spawn_workflow_runner_tracks_active_processes() {
        let _lock = crate::shared::test_env_lock().lock().expect("env lock should be available");

        let temp_dir = TempDir::new().expect("temp directory should be created");
        let runner_path = {
            #[cfg(unix)]
            let path = temp_dir.path().join("ao-workflow-runner");
            #[cfg(not(unix))]
            let path = temp_dir.path().join("ao-workflow-runner.exe");
            path
        };

        #[cfg(unix)]
        let runner_payload = "#!/bin/sh\nexit 0\n";
        #[cfg(not(unix))]
        let runner_payload = "@echo off\r\nexit /B 0\r\n";

        fs::write(&runner_path, runner_payload).expect("mock runner should be written");
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(&runner_path)
                .expect("mock runner metadata should be available")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner_path, permissions)
                .expect("mock runner should be executable");
        }

        let original_path = env::var_os("PATH").unwrap_or_default();
        let mut paths = env::split_paths(&original_path).collect::<Vec<_>>();
        paths.insert(0, temp_dir.path().to_path_buf());
        let candidate_path = env::join_paths(paths).expect("path list should join");
        let candidate_path = candidate_path.to_string_lossy();
        let _path_guard = EnvVarGuard::set("PATH", Some(candidate_path.as_ref()));

        let mut manager = ProcessManager::new();
        let subject = WorkflowSubjectArgs::Task { task_id: "task-123".to_string() };
        let process =
            manager
                .spawn_workflow_runner(&subject, "standard", temp_dir.path().to_string_lossy().as_ref())
                .expect("mock runner should be discovered from PATH and spawned");
        assert_eq!(process.subject_id, "task-123");
        assert_eq!(process.task_id.as_deref(), Some("task-123"));
        assert_eq!(manager.active_count(), 1);
        let _ = manager.check_running();

        drop(process);
    }

    #[test]
    fn subject_id_returns_correct_value_for_each_variant() {
        let task = WorkflowSubjectArgs::Task { task_id: "TASK-1".into() };
        assert_eq!(task.subject_id(), "TASK-1");
        assert!(task.schedule_id().is_none());

        let req = WorkflowSubjectArgs::Requirement { requirement_id: "REQ-1".into() };
        assert_eq!(req.subject_id(), "REQ-1");
        assert!(req.schedule_id().is_none());

        let custom = WorkflowSubjectArgs::Custom {
            title: "schedule:daily-review".into(),
            description: Some("desc".into()),
            input_json: Some(r#"{"key":"val"}"#.into()),
        };
        assert_eq!(custom.subject_id(), "schedule:daily-review");
        assert_eq!(custom.schedule_id(), Some("daily-review"));
    }

    #[tokio::test]
    async fn spawn_custom_subject_tracks_schedule_id() {
        let _lock = crate::shared::test_env_lock().lock().expect("env lock should be available");

        let temp_dir = TempDir::new().expect("temp directory should be created");
        let runner_path = {
            #[cfg(unix)]
            let path = temp_dir.path().join("ao-workflow-runner");
            #[cfg(not(unix))]
            let path = temp_dir.path().join("ao-workflow-runner.exe");
            path
        };

        #[cfg(unix)]
        let runner_payload = "#!/bin/sh\nexit 0\n";
        #[cfg(not(unix))]
        let runner_payload = "@echo off\r\nexit /B 0\r\n";

        fs::write(&runner_path, runner_payload).expect("mock runner should be written");
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(&runner_path)
                .expect("mock runner metadata should be available")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner_path, permissions)
                .expect("mock runner should be executable");
        }

        let original_path = env::var_os("PATH").unwrap_or_default();
        let mut paths = env::split_paths(&original_path).collect::<Vec<_>>();
        paths.insert(0, temp_dir.path().to_path_buf());
        let candidate_path = env::join_paths(paths).expect("path list should join");
        let candidate_path = candidate_path.to_string_lossy();
        let _path_guard = EnvVarGuard::set("PATH", Some(candidate_path.as_ref()));

        let mut manager = ProcessManager::new();
        let subject = WorkflowSubjectArgs::Custom {
            title: "schedule:nightly".into(),
            description: Some("Nightly run".into()),
            input_json: Some(r#"{"timeout":300}"#.into()),
        };
        let process = manager
            .spawn_workflow_runner(&subject, "standard", temp_dir.path().to_string_lossy().as_ref())
            .expect("mock runner should be spawned for custom subject");
        assert_eq!(process.subject_id, "schedule:nightly");
        assert!(process.task_id.is_none());
        assert_eq!(process.schedule_id.as_deref(), Some("nightly"));
        assert_eq!(manager.active_count(), 1);
        let _ = manager.check_running();
    }
}
