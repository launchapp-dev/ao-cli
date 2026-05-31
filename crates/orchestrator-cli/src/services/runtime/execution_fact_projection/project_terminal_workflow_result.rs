use std::sync::Arc;

use orchestrator_core::{project_task_terminal_workflow_status, services::ServiceHub, WorkflowStatus};
use orchestrator_daemon_runtime::remove_terminal_dispatch_queue_entry_non_fatal;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn project_terminal_workflow_result(
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    subject_id: &str,
    task_id: Option<&str>,
    workflow_ref: Option<&str>,
    workflow_id: Option<&str>,
    workflow_status: WorkflowStatus,
    failure_reason: Option<&str>,
) {
    if !matches!(
        workflow_status,
        WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Escalated | WorkflowStatus::Cancelled
    ) {
        return;
    }

    remove_terminal_dispatch_queue_entry_non_fatal(project_root, subject_id, workflow_ref, workflow_id);

    // TODO(codex-p2): wire `queue/completion` against the v0.5 queue
    // plugin alongside the in-tree call above. Currently when a plugin
    // is installed, `daemon_task_dispatch::dispatch_queued_entries_via_runner`
    // transitions the entry pending → assigned via the plugin but the
    // workflow's terminal status never feeds back into a
    // `queue/completion { status: "completed"/"failed"/"cancelled" }`
    // call. The plugin queue is therefore monotonically growing under
    // demo-quality v0.5. Fixes for this require either threading a
    // tokio handle through this synchronous projection (so we can
    // `block_in_place`/`spawn` the async plugin RPC) or making the
    // projection itself async. Deferred to v0.5.x alongside the
    // `queue/lease`+rollback wiring tracked in
    // `daemon_task_dispatch.rs` TODOs.

    let Some(task_id) = task_id.filter(|task_id| !task_id.trim().is_empty()) else {
        return;
    };

    project_task_terminal_workflow_status(hub, task_id, workflow_status, failure_reason.map(ToOwned::to_owned)).await;
}
