use anyhow::Result;

use crate::DaemonRunEvent;

#[async_trait::async_trait(?Send)]
pub trait DaemonRunHooks {
    fn handle_event(&mut self, event: DaemonRunEvent) -> Result<()>;

    async fn recover_orphaned_running_workflows_on_startup(
        &mut self,
        _project_root: &str,
    ) -> Result<usize> {
        Ok(0)
    }

    async fn flush_notifications(&mut self, _project_root: &str) -> Result<()> {
        Ok(())
    }
}
