use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use orchestrator_core::services::ServiceHub;

use crate::DispatchWorkflowStartSummary;

#[async_trait::async_trait(?Send)]
pub trait ProjectTickHooks {
    fn process_due_schedules(&mut self, root: &str, now: DateTime<Utc>);

    fn active_process_count(&self) -> usize {
        0
    }

    async fn reconcile_completed_processes(
        &mut self,
        _hub: Arc<dyn ServiceHub>,
        _root: &str,
    ) -> Result<(usize, usize)> {
        Ok((0, 0))
    }

    async fn dispatch_ready_tasks(
        &mut self,
        _hub: Arc<dyn ServiceHub>,
        _root: &str,
        _limit: usize,
    ) -> Result<DispatchWorkflowStartSummary> {
        Ok(DispatchWorkflowStartSummary::default())
    }
}
