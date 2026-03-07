use serde::{Deserialize, Serialize};

use crate::TaskSelectionSource;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyTaskWorkflowStart {
    pub task_id: String,
    pub workflow_id: String,
    pub selection_source: TaskSelectionSource,
}
