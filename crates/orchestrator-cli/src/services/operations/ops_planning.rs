use std::sync::Arc;

use anyhow::Result;
use orchestrator_core::services::ServiceHub;

use crate::{PlanningCommand, PlanningVisionCommand, PlanningRequirementsCommand};

pub(crate) async fn handle_planning(
    command: PlanningCommand,
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    match command {
        PlanningCommand::Vision { command } => match command {
            PlanningVisionCommand::Draft(args) => {
                let vision_command = crate::VisionCommand::Draft(args);
                super::ops_vision::handle_vision(vision_command, hub.clone(), project_root, json).await
            }
            PlanningVisionCommand::Refine(args) => {
                let vision_command = crate::VisionCommand::Refine(args);
                super::ops_vision::handle_vision(vision_command, hub.clone(), project_root, json).await
            }
        },
        PlanningCommand::Requirements { command } => match command {
            PlanningRequirementsCommand::Draft(args) => {
                let requirements_command = crate::RequirementsCommand::Draft(args);
                super::ops_requirements::handle_requirements(requirements_command, hub.clone(), project_root, json).await
            }
            PlanningRequirementsCommand::Refine(args) => {
                let requirements_command = crate::RequirementsCommand::Refine(args);
                super::ops_requirements::handle_requirements(requirements_command, hub.clone(), project_root, json).await
            }
            PlanningRequirementsCommand::Execute(args) => {
                let requirements_command = crate::RequirementsCommand::Execute(args);
                super::ops_requirements::handle_requirements(requirements_command, hub.clone(), project_root, json).await
            }
        },
    }
}
