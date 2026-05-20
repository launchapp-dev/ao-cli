//! CLI-side `WorkflowRouting` adapter — bridges the daemon's control
//! surface back to the same `WorkflowServiceApi` + `dispatch_workflow_event`
//! helpers the CLI uses for its in-process code path.
//!
//! See the sibling [`crate::services::operations::ops_plugin::control_routing`]
//! and [`crate::services::runtime::runtime_daemon::control_routing`] modules
//! for the plugin/* and daemon/* equivalents.
//!
//! ## Shape conversion
//!
//! The wire types under [`animus_control_protocol::types`] are a small
//! subset of the in-tree [`orchestrator_core::OrchestratorWorkflow`].
//! [`workflow_summary_from_core`] does the projection: top-level summary
//! fields land in [`WireWorkflowRunSummary`]; the full
//! `OrchestratorWorkflow` is re-encoded as opaque JSON under
//! [`WireWorkflowRun::detail`] so wire consumers that need the rich
//! schema can still parse it.
//!
//! ## Error mapping
//!
//! Anyhow errors carry no machine-readable code. We surface them as
//! [`ControlError::Internal`] with the original message preserved (with
//! `{:#}` so the chain shows up).

use std::path::PathBuf;
use std::sync::Arc;

use animus_control_protocol::{
    types::{
        Unit, WorkflowCancelRequest as WireCancelRequest, WorkflowExecuteRequest as WireExecuteRequest,
        WorkflowGetRequest as WireGetRequest, WorkflowListRequest as WireListRequest,
        WorkflowListResponse as WireListResponse, WorkflowPauseRequest as WirePauseRequest,
        WorkflowResumeRequest as WireResumeRequest, WorkflowRun as WireWorkflowRun,
        WorkflowRunRequest as WireRunRequest, WorkflowRunStart as WireRunStart, WorkflowRunSummary as WireRunSummary,
        WorkflowStatus as WireWorkflowStatus,
    },
    ControlError,
};
use animus_subject_protocol_wire::SubjectId;
use async_trait::async_trait;
use orchestrator_core::{
    dispatch_workflow_event, FileServiceHub, ListPage, ListPageRequest, OrchestratorWorkflow, ServiceHub,
    WorkflowEvent, WorkflowFilter, WorkflowQuery, WorkflowRunInput, WorkflowStatus as CoreWorkflowStatus,
};
use orchestrator_daemon_runtime::control::WorkflowRouting;

/// Build a [`WorkflowRouting`] handle bound to `project_root`.
///
/// `project_root` is captured once at daemon startup and reused for every
/// routed call. The returned handle is `Clone` + `Send + Sync` via the
/// `Arc<dyn>` wrapper.
pub fn build_workflow_routing(project_root: PathBuf) -> Arc<dyn WorkflowRouting> {
    Arc::new(WorkflowRoutingImpl { project_root })
}

struct WorkflowRoutingImpl {
    project_root: PathBuf,
}

impl WorkflowRoutingImpl {
    fn project_root_str(&self) -> String {
        self.project_root.to_string_lossy().to_string()
    }

    fn hub(&self) -> Result<Arc<dyn ServiceHub>, ControlError> {
        let hub = FileServiceHub::new(&self.project_root_str())
            .map_err(|err| ControlError::Internal(format!("workflow routing: hub init: {err:#}")))?;
        Ok(Arc::new(hub))
    }
}

fn internal(err: anyhow::Error) -> ControlError {
    ControlError::Internal(format!("{err:#}"))
}

/// Map the core `WorkflowStatus` (snake_case + `Escalated`) into the wire
/// `WorkflowStatus` (kebab-case + no `Escalated`). Escalated runs are
/// reported on the wire as `Failed` — closest semantic match, and the
/// full status string survives in the opaque `detail` JSON.
fn core_status_to_wire(status: CoreWorkflowStatus) -> WireWorkflowStatus {
    match status {
        CoreWorkflowStatus::Pending => WireWorkflowStatus::Pending,
        CoreWorkflowStatus::Running => WireWorkflowStatus::Running,
        CoreWorkflowStatus::Paused => WireWorkflowStatus::Paused,
        CoreWorkflowStatus::Completed => WireWorkflowStatus::Completed,
        CoreWorkflowStatus::Failed | CoreWorkflowStatus::Escalated => WireWorkflowStatus::Failed,
        CoreWorkflowStatus::Cancelled => WireWorkflowStatus::Cancelled,
    }
}

fn wire_status_to_core(status: WireWorkflowStatus) -> CoreWorkflowStatus {
    match status {
        WireWorkflowStatus::Pending => CoreWorkflowStatus::Pending,
        WireWorkflowStatus::Running => CoreWorkflowStatus::Running,
        WireWorkflowStatus::Paused => CoreWorkflowStatus::Paused,
        WireWorkflowStatus::Completed => CoreWorkflowStatus::Completed,
        WireWorkflowStatus::Failed => CoreWorkflowStatus::Failed,
        WireWorkflowStatus::Cancelled => CoreWorkflowStatus::Cancelled,
    }
}

fn workflow_summary_from_core(workflow: &OrchestratorWorkflow) -> WireRunSummary {
    let definition = workflow.workflow_ref.clone().unwrap_or_default();
    let subject_id =
        if workflow.task_id.is_empty() { None } else { Some(SubjectId::new(format!("task:{}", workflow.task_id))) };
    WireRunSummary {
        id: workflow.id.clone(),
        definition,
        status: core_status_to_wire(workflow.status),
        subject_id,
        started_at: workflow.started_at,
        finished_at: workflow.completed_at,
    }
}

fn workflow_to_wire_run(workflow: OrchestratorWorkflow) -> Result<WireWorkflowRun, ControlError> {
    let summary = workflow_summary_from_core(&workflow);
    let detail =
        serde_json::to_value(&workflow).map_err(|e| ControlError::Internal(format!("workflow detail encode: {e}")))?;
    Ok(WireWorkflowRun { summary, detail })
}

#[async_trait]
impl WorkflowRouting for WorkflowRoutingImpl {
    async fn workflow_list(&self, request: WireListRequest) -> Result<WireListResponse, ControlError> {
        let hub = self.hub()?;
        let limit = request.limit.map(|v| v as usize);
        let offset: usize = request.cursor.as_deref().and_then(|c| c.parse().ok()).unwrap_or(0);
        let query = WorkflowQuery {
            filter: WorkflowFilter {
                status: request.status.map(wire_status_to_core),
                workflow_ref: None,
                task_id: None,
                phase_id: None,
                search_text: None,
            },
            page: ListPageRequest { limit, offset },
            sort: Default::default(),
        };
        let page: ListPage<OrchestratorWorkflow> = hub.workflows().query(query).await.map_err(internal)?;
        let runs: Vec<WireRunSummary> = page.items.iter().map(workflow_summary_from_core).collect();
        let next_cursor = page.next_offset.map(|n| n.to_string());
        Ok(WireListResponse { runs, next_cursor })
    }

    async fn workflow_get(&self, request: WireGetRequest) -> Result<WireWorkflowRun, ControlError> {
        let hub = self.hub()?;
        let workflow = hub.workflows().get(&request.id).await.map_err(internal)?;
        workflow_to_wire_run(workflow)
    }

    async fn workflow_run(&self, request: WireRunRequest) -> Result<WireRunStart, ControlError> {
        let hub = self.hub()?;
        let input = WorkflowRunInput::for_task(request.task_id, request.definition);
        let workflow = hub.workflows().run(input).await.map_err(internal)?;
        Ok(WireRunStart {
            workflow_id: workflow.id,
            status: core_status_to_wire(workflow.status),
            started_at: workflow.started_at,
        })
    }

    async fn workflow_execute(&self, request: WireExecuteRequest) -> Result<WireRunStart, ControlError> {
        let hub = self.hub()?;
        let subject_id_str = request.subject_id.as_ref().map(|sid| sid.as_str().to_string());
        // Strip the optional "task:" prefix from subject ids so a wire
        // caller can pass either `task:TASK-1` or `TASK-1`.
        let task_id =
            subject_id_str.map(|s| s.strip_prefix("task:").map(str::to_string).unwrap_or(s)).unwrap_or_default();
        if task_id.is_empty() {
            return Err(ControlError::InvalidRequest(
                "workflow/execute requires a task subject_id for the CLI control routing".to_string(),
            ));
        }
        let input = WorkflowRunInput::for_task(task_id, Some(request.definition));
        let workflow = hub.workflows().run(input).await.map_err(internal)?;
        Ok(WireRunStart {
            workflow_id: workflow.id,
            status: core_status_to_wire(workflow.status),
            started_at: workflow.started_at,
        })
    }

    async fn workflow_pause(&self, request: WirePauseRequest) -> Result<Unit, ControlError> {
        let hub = self.hub()?;
        let _ =
            dispatch_workflow_event(hub, &self.project_root_str(), WorkflowEvent::Pause { workflow_id: request.id })
                .await
                .map_err(internal)?;
        Ok(Unit::default())
    }

    async fn workflow_resume(&self, request: WireResumeRequest) -> Result<Unit, ControlError> {
        let hub = self.hub()?;
        let _ = dispatch_workflow_event(
            hub,
            &self.project_root_str(),
            WorkflowEvent::Resume { workflow_id: request.id, feedback: None },
        )
        .await
        .map_err(internal)?;
        Ok(Unit::default())
    }

    async fn workflow_cancel(&self, request: WireCancelRequest) -> Result<Unit, ControlError> {
        let hub = self.hub()?;
        let _ =
            dispatch_workflow_event(hub, &self.project_root_str(), WorkflowEvent::Cancel { workflow_id: request.id })
                .await
                .map_err(internal)?;
        Ok(Unit::default())
    }
}
