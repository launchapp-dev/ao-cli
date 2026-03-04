use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::process::Stdio;
use tokio::process::{Child, Command};

#[derive(Debug, Clone)]
pub struct WorkflowProcess {
    pub task_id: String,
    pub child: Arc<Mutex<Child>>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct CompletedProcess {
    pub task_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub failure_reason: Option<String>,
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
        task_id: &str,
        pipeline_id: &str,
        project_root: &str,
    ) -> Result<WorkflowProcess> {
        let mut command = Command::new("ao-workflow-runner");
        command
            .arg("execute")
            .arg("--task-id")
            .arg(task_id)
            .arg("--pipeline")
            .arg(pipeline_id)
            .arg("--project-root")
            .arg(project_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = command.spawn().context("failed to spawn ao-workflow-runner")?;
        let process = WorkflowProcess {
            task_id: task_id.to_string(),
            child: Arc::new(Mutex::new(child)),
            started_at: Utc::now(),
        };

        self.processes.push(process.clone());

        Ok(process)
    }

    pub fn check_running(&mut self) -> Vec<CompletedProcess> {
        let now = Utc::now();
        let mut completed = Vec::new();
        let mut active = Vec::with_capacity(self.processes.len());

        for process in self.processes.drain(..) {
            let status = {
                let mut maybe_child = match process.child.lock() {
                    Ok(guard) => guard,
                    Err(error) => {
                        completed.push(CompletedProcess {
                            task_id: process.task_id,
                            started_at: process.started_at,
                            completed_at: now,
                            exit_code: None,
                            success: false,
                            failure_reason: Some(format!(
                                "failed to lock workflow process handle: {}",
                                error
                            )),
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

                    completed.push(CompletedProcess {
                        task_id: process.task_id,
                        started_at: process.started_at,
                        completed_at: now,
                        exit_code,
                        success,
                        failure_reason,
                    });
                }
                Ok(None) => {
                    active.push(process);
                }
                Err(error) => {
                    completed.push(CompletedProcess {
                        task_id: process.task_id,
                        started_at: process.started_at,
                        completed_at: now,
                        exit_code: None,
                        success: false,
                        failure_reason: Some(format!("failed to probe workflow process status: {}", error)),
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
