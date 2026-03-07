use crate::RunnerEvent;

#[derive(Debug, Clone)]
pub struct SubjectExecutionFact {
    pub subject_id: String,
    pub task_id: Option<String>,
    pub schedule_id: Option<String>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub failure_reason: Option<String>,
    pub runner_events: Vec<RunnerEvent>,
}

impl SubjectExecutionFact {
    pub fn completion_status(&self) -> &str {
        if self.success {
            "completed"
        } else {
            "failed"
        }
    }
}
