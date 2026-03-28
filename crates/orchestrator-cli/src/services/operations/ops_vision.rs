use std::sync::Arc;

use anyhow::Result;
use orchestrator_core::services::ServiceHub;

use crate::services::operations::ops_workflow::execute::WorkflowExecuteArgs;
use crate::{VisionCommand, VisionDraftArgs, VisionRefineArgs};

pub(crate) async fn handle_vision(
    command: VisionCommand,
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    match command {
        VisionCommand::Draft(args) => {
            let execute_args = build_vision_draft_args(args)?;
            super::ops_workflow::execute::handle_workflow_execute(execute_args, hub.clone(), project_root, json).await
        }
        VisionCommand::Refine(args) => {
            let execute_args = build_vision_refine_args(args)?;
            super::ops_workflow::execute::handle_workflow_execute(execute_args, hub.clone(), project_root, json).await
        }
    }
}

fn build_vision_draft_args(args: VisionDraftArgs) -> Result<WorkflowExecuteArgs> {
    Ok(WorkflowExecuteArgs {
        workflow_id: None,
        task_id: None,
        requirement_id: None,
        title: None,
        description: None,
        workflow_ref: Some("builtin/vision-draft".to_string()),
        phase: None,
        model: None,
        tool: None,
        phase_timeout_secs: None,
        input_json: args.input_json,
        vars: Vec::new(),
    })
}

fn build_vision_refine_args(args: VisionRefineArgs) -> Result<WorkflowExecuteArgs> {
    Ok(WorkflowExecuteArgs {
        workflow_id: None,
        task_id: None,
        requirement_id: None,
        title: None,
        description: None,
        workflow_ref: Some("builtin/vision-refine".to_string()),
        phase: None,
        model: None,
        tool: None,
        phase_timeout_secs: None,
        input_json: args.input_json,
        vars: Vec::new(),
    })
}
