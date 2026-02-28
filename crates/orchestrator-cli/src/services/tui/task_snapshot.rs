use orchestrator_core::{Assignee, OrchestratorTask, TaskStatus};

#[derive(Debug, Clone)]
pub(crate) struct TaskSnapshot {
    pub(crate) id: String,
    pub(crate) status: TaskStatus,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) assignee_label: String,
}

impl TaskSnapshot {
    pub(crate) fn from_task(task: OrchestratorTask) -> Self {
        let assignee_label = match &task.assignee {
            Assignee::Agent { role, model } => {
                if let Some(m) = model {
                    format!("agent:{role}/{m}")
                } else {
                    format!("agent:{role}")
                }
            }
            Assignee::Human { user_id } => user_id.clone(),
            Assignee::Unassigned => String::new(),
        };
        Self {
            id: task.id,
            status: task.status,
            title: task.title,
            description: task.description,
            assignee_label,
        }
    }

    pub(crate) fn status_label(&self) -> &'static str {
        status_label(self.status)
    }

    pub(crate) fn label(&self) -> String {
        format!("{} [{}] {}", self.id, self.status_label(), self.title)
    }
}

pub(crate) fn status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Backlog => "backlog",
        TaskStatus::Ready => "ready",
        TaskStatus::InProgress => "in-progress",
        TaskStatus::Blocked => "blocked",
        TaskStatus::OnHold => "on-hold",
        TaskStatus::Done => "done",
        TaskStatus::Cancelled => "cancelled",
    }
}

pub(crate) const STATUS_CYCLE: &[TaskStatus] = &[
    TaskStatus::Backlog,
    TaskStatus::Ready,
    TaskStatus::InProgress,
    TaskStatus::OnHold,
    TaskStatus::Done,
    TaskStatus::Cancelled,
];
