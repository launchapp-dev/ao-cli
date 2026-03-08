use serde::{Deserialize, Serialize};

use crate::DispatchWorkflowStart;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DispatchWorkflowStartSummary {
    pub started: usize,
    pub started_workflows: Vec<DispatchWorkflowStart>,
}
