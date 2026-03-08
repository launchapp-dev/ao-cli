use anyhow::Result;
use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseExecutionRequest {
    pub task_id: String,
    pub phase_id: String,
    pub pipeline_id: String,
    pub project_root: String,
    pub config_dir: String,
    pub model_override: Option<String>,
    pub tool_override: Option<String>,
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseVerdict {
    Advance,
    Rework { target_phase: String },
    Skip,
    Failed { reason: String },
}

#[derive(Debug, Clone)]
pub struct PhaseExecutionResult {
    pub exit_code: i32,
    pub verdict: PhaseVerdict,
    pub output_log: String,
    pub error: Option<String>,
    pub commit_message: Option<String>,
}

#[async_trait]
pub trait PhaseExecutor {
    async fn execute_phase(&self, request: PhaseExecutionRequest) -> Result<PhaseExecutionResult>;
}
