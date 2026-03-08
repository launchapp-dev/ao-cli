use serde::{Deserialize, Serialize};

use crate::{DispatchSelectionSource, SubjectDispatch};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchWorkflowStart {
    pub dispatch: SubjectDispatch,
    pub workflow_id: String,
    pub selection_source: DispatchSelectionSource,
}

impl DispatchWorkflowStart {
    pub fn task_id(&self) -> Option<&str> {
        self.dispatch.task_id()
    }
}
