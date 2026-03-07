#[derive(Debug, Clone)]
pub enum WorkflowSubjectArgs {
    Task {
        task_id: String,
    },
    Requirement {
        requirement_id: String,
    },
    Custom {
        title: String,
        description: Option<String>,
        input_json: Option<String>,
    },
}

impl WorkflowSubjectArgs {
    pub fn subject_id(&self) -> &str {
        match self {
            Self::Task { task_id } => task_id,
            Self::Requirement { requirement_id } => requirement_id,
            Self::Custom { title, .. } => title,
        }
    }

    pub fn schedule_id(&self) -> Option<&str> {
        match self {
            Self::Custom { title, .. } => title.strip_prefix("schedule:"),
            _ => None,
        }
    }

    pub fn build_runner_command(
        &self,
        pipeline_id: &str,
        project_root: &str,
    ) -> std::process::Command {
        let mut cmd = std::process::Command::new("ao-workflow-runner");
        cmd.arg("execute");

        match self {
            Self::Task { task_id } => {
                cmd.arg("--task-id").arg(task_id);
            }
            Self::Requirement { requirement_id } => {
                cmd.arg("--requirement-id").arg(requirement_id);
            }
            Self::Custom {
                title,
                description,
                input_json,
            } => {
                cmd.arg("--title").arg(title);
                if let Some(desc) = description {
                    cmd.arg("--description").arg(desc);
                }
                if let Some(json) = input_json {
                    cmd.env("AO_SCHEDULE_INPUT", json);
                }
            }
        }

        cmd.arg("--pipeline")
            .arg(pipeline_id)
            .arg("--project-root")
            .arg(project_root);
        cmd
    }
}
