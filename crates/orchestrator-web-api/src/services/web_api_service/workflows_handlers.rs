use orchestrator_core::{FileServiceHub, ServiceHub, WorkflowRunInput};
use protocol::orchestrator::WorkflowSubject;
use serde_json::{json, Value};

use super::{parsing::parse_json_body, requests::WorkflowRunRequest, WebApiError, WebApiService};

fn resolve_workflow_run_input(
    request: WorkflowRunRequest,
) -> Result<WorkflowRunInput, WebApiError> {
    match (request.task_id, request.requirement_id, request.title) {
        (Some(task_id), None, None) => {
            Ok(WorkflowRunInput::for_task(task_id, request.workflow_ref))
        }
        (None, Some(requirement_id), None) => Ok(WorkflowRunInput::for_requirement(
            requirement_id,
            request.workflow_ref,
        )),
        (None, None, Some(title)) => Ok(WorkflowRunInput::for_custom(
            title,
            request.description.unwrap_or_default(),
            request.workflow_ref,
        )),
        (None, None, None) => Err(WebApiError::new(
            "invalid_input",
            "one of task_id, requirement_id, or title must be provided".to_string(),
            2,
        )),
        _ => Err(WebApiError::new(
            "invalid_input",
            "task_id, requirement_id, and title are mutually exclusive".to_string(),
            2,
        )),
    }
}

impl WebApiService {
    pub async fn workflows_list(&self) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.workflows().list().await?))
    }

    pub async fn project_workflows(&self, id: &str) -> Result<Value, WebApiError> {
        let project = self.context.hub.projects().get(id).await?;
        let hub = FileServiceHub::new(&project.path)?;
        let workflows = hub.workflows().list().await?;

        Ok(json!({
            "project": project,
            "workflows": workflows,
        }))
    }

    pub async fn workflows_get(&self, id: &str) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.workflows().get(id).await?))
    }

    pub async fn workflows_decisions(&self, id: &str) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.workflows().decisions(id).await?))
    }

    pub async fn workflows_checkpoints(&self, id: &str) -> Result<Value, WebApiError> {
        Ok(json!(
            self.context.hub.workflows().list_checkpoints(id).await?
        ))
    }

    pub async fn workflows_get_checkpoint(
        &self,
        id: &str,
        checkpoint: usize,
    ) -> Result<Value, WebApiError> {
        Ok(json!(
            self.context
                .hub
                .workflows()
                .get_checkpoint(id, checkpoint)
                .await?
        ))
    }

    pub async fn workflows_run(&self, body: Value) -> Result<Value, WebApiError> {
        let request: WorkflowRunRequest = parse_json_body(body)?;
        let input = resolve_workflow_run_input(request)?;
        let workflow = self.context.hub.workflows().run(input).await?;
        let subject_id = match &workflow.subject {
            WorkflowSubject::Task { id } | WorkflowSubject::Requirement { id } => id.clone(),
            WorkflowSubject::Custom { title, .. } => title.clone(),
        };
        self.publish_event(
            "workflow-run",
            json!({
                "workflow_id": workflow.id,
                "subject_id": subject_id,
                "task_id": workflow.task_id,
            }),
        );
        Ok(json!(workflow))
    }

    pub async fn workflows_resume(&self, id: &str) -> Result<Value, WebApiError> {
        let workflow = self.context.hub.workflows().resume(id).await?;
        self.publish_event(
            "workflow-resume",
            json!({ "workflow_id": workflow.id, "status": workflow.status }),
        );
        Ok(json!(workflow))
    }

    pub async fn workflows_pause(&self, id: &str) -> Result<Value, WebApiError> {
        let workflow = self.context.hub.workflows().pause(id).await?;
        self.publish_event(
            "workflow-pause",
            json!({ "workflow_id": workflow.id, "status": workflow.status }),
        );
        Ok(json!(workflow))
    }

    pub async fn workflows_cancel(&self, id: &str) -> Result<Value, WebApiError> {
        let workflow = self.context.hub.workflows().cancel(id).await?;
        self.publish_event(
            "workflow-cancel",
            json!({ "workflow_id": workflow.id, "status": workflow.status }),
        );
        Ok(json!(workflow))
    }
}
