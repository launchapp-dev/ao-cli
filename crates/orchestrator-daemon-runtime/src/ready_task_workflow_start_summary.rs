use serde::{Deserialize, Serialize};

use crate::ReadyTaskWorkflowStart;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadyTaskWorkflowStartSummary {
    pub started: usize,
    pub started_workflows: Vec<ReadyTaskWorkflowStart>,
}
