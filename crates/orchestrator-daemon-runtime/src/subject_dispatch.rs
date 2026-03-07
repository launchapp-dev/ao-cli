use chrono::{DateTime, Utc};
use orchestrator_core::WorkflowSubject;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectDispatch {
    pub subject: WorkflowSubject,
    pub pipeline_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    pub trigger_source: String,
    pub requested_at: DateTime<Utc>,
}

impl SubjectDispatch {
    pub fn for_task(task_id: impl Into<String>, pipeline_id: impl Into<String>) -> Self {
        Self {
            subject: WorkflowSubject::Task { id: task_id.into() },
            pipeline_id: pipeline_id.into(),
            input: None,
            priority: None,
            trigger_source: "ready-queue".to_string(),
            requested_at: Utc::now(),
        }
    }

    pub fn for_requirement(
        requirement_id: impl Into<String>,
        pipeline_id: impl Into<String>,
        trigger_source: impl Into<String>,
    ) -> Self {
        Self {
            subject: WorkflowSubject::Requirement {
                id: requirement_id.into(),
            },
            pipeline_id: pipeline_id.into(),
            input: None,
            priority: None,
            trigger_source: trigger_source.into(),
            requested_at: Utc::now(),
        }
    }

    pub fn for_custom(
        title: impl Into<String>,
        description: impl Into<String>,
        pipeline_id: impl Into<String>,
        input: Option<Value>,
        trigger_source: impl Into<String>,
    ) -> Self {
        Self {
            subject: WorkflowSubject::Custom {
                title: title.into(),
                description: description.into(),
            },
            pipeline_id: pipeline_id.into(),
            input,
            priority: None,
            trigger_source: trigger_source.into(),
            requested_at: Utc::now(),
        }
    }

    pub fn subject_id(&self) -> &str {
        self.subject.id()
    }

    pub fn task_id(&self) -> Option<&str> {
        match &self.subject {
            WorkflowSubject::Task { id } => Some(id),
            _ => None,
        }
    }

    pub fn schedule_id(&self) -> Option<&str> {
        match &self.subject {
            WorkflowSubject::Custom { title, .. } => title.strip_prefix("schedule:"),
            _ => None,
        }
    }

    pub fn build_runner_command(&self, project_root: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("ao-workflow-runner");
        cmd.arg("execute");

        match &self.subject {
            WorkflowSubject::Task { id } => {
                cmd.arg("--task-id").arg(id);
            }
            WorkflowSubject::Requirement { id } => {
                cmd.arg("--requirement-id").arg(id);
            }
            WorkflowSubject::Custom { title, description } => {
                cmd.arg("--title").arg(title);
                cmd.arg("--description").arg(description);
                if let Some(input) = &self.input {
                    cmd.env("AO_SCHEDULE_INPUT", input.to_string());
                }
            }
        }

        cmd.arg("--pipeline")
            .arg(&self.pipeline_id)
            .arg("--project-root")
            .arg(project_root);
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::SubjectDispatch;
    use orchestrator_core::WorkflowSubject;
    use serde_json::json;

    #[test]
    fn subject_id_matches_subject_identity() {
        let task = SubjectDispatch::for_task("TASK-1", "default");
        let requirement = SubjectDispatch::for_requirement("REQ-1", "default", "manual");
        let custom = SubjectDispatch::for_custom(
            "schedule:nightly",
            "nightly dispatch",
            "default",
            Some(json!({"key":"value"})),
            "schedule",
        );

        assert_eq!(task.subject_id(), "TASK-1");
        assert_eq!(requirement.subject_id(), "REQ-1");
        assert_eq!(custom.subject_id(), "schedule:nightly");
    }

    #[test]
    fn task_and_schedule_ids_are_derived_from_subject() {
        let task = SubjectDispatch::for_task("TASK-9", "default");
        let custom = SubjectDispatch::for_custom(
            "schedule:daily-review",
            "dispatch",
            "default",
            None,
            "schedule",
        );

        assert_eq!(task.task_id(), Some("TASK-9"));
        assert_eq!(task.schedule_id(), None);
        assert_eq!(custom.task_id(), None);
        assert_eq!(custom.schedule_id(), Some("daily-review"));
    }

    #[test]
    fn runner_command_uses_subject_and_pipeline_from_dispatch() {
        let dispatch = SubjectDispatch::for_custom(
            "schedule:nightly",
            "nightly dispatch",
            "ops",
            Some(json!({"nightly":true})),
            "schedule",
        );
        let command = dispatch.build_runner_command("/tmp/project");
        let program = command.get_program().to_string_lossy().into_owned();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(program, "ao-workflow-runner");
        assert_eq!(
            args,
            vec![
                "execute",
                "--title",
                "schedule:nightly",
                "--description",
                "nightly dispatch",
                "--pipeline",
                "ops",
                "--project-root",
                "/tmp/project",
            ]
        );
        assert_eq!(
            &dispatch.subject,
            &WorkflowSubject::Custom {
                title: "schedule:nightly".to_string(),
                description: "nightly dispatch".to_string(),
            }
        );
    }
}
