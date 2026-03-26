use super::*;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub(super) struct TaskListInput {
    #[serde(default)]
    pub(super) project_root: Option<String>,
    #[serde(default)]
    pub(super) task_type: Option<String>,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) priority: Option<String>,
    #[serde(default)]
    pub(super) risk: Option<String>,
    #[serde(default)]
    pub(super) assignee_type: Option<String>,
    #[serde(default)]
    pub(super) tag: Vec<String>,
    #[serde(default)]
    pub(super) linked_requirement: Option<String>,
    #[serde(default)]
    pub(super) linked_architecture_entity: Option<String>,
    #[serde(default)]
    pub(super) search: Option<String>,
    #[serde(default)]
    pub(super) sort: Option<String>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) offset: Option<usize>,
    #[serde(default)]
    pub(super) max_tokens: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub(super) struct TaskPrioritizedInput {
    #[serde(default)]
    pub(super) project_root: Option<String>,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) priority: Option<String>,
    #[serde(default)]
    pub(super) assignee_type: Option<String>,
    #[serde(default)]
    pub(super) search: Option<String>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
    #[serde(default)]
    pub(super) offset: Option<usize>,
    #[serde(default)]
    pub(super) max_tokens: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskCreateInput {
    pub(super) title: String,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) task_type: Option<String>,
    #[serde(default)]
    pub(super) priority: Option<String>,
    #[serde(default)]
    pub(super) linked_requirement: Vec<String>,
    #[serde(default)]
    pub(super) linked_architecture_entity: Vec<String>,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskStatusInput {
    pub(super) id: String,
    pub(super) status: String,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskGetInput {
    pub(super) id: String,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskDeleteInput {
    pub(super) id: String,
    #[serde(default)]
    pub(super) confirm: Option<String>,
    #[serde(default)]
    pub(super) dry_run: bool,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskControlInput {
    #[serde(alias = "task_id")]
    pub(super) id: String,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskUpdateInput {
    pub(super) id: String,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) priority: Option<String>,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) assignee: Option<String>,
    #[serde(default)]
    pub(super) linked_architecture_entity: Vec<String>,
    #[serde(default)]
    pub(super) replace_linked_architecture_entities: bool,
    #[serde(default)]
    pub(super) input_json: Option<String>,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskAssignInput {
    pub(super) id: String,
    pub(super) assignee: String,
    #[serde(default)]
    pub(super) assignee_type: Option<String>,
    #[serde(default)]
    pub(super) agent_role: Option<String>,
    #[serde(default)]
    pub(super) model: Option<String>,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct BulkTaskStatusItem {
    pub(super) id: String,
    pub(super) status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskBulkStatusInput {
    pub(super) updates: Vec<BulkTaskStatusItem>,
    #[serde(default)]
    pub(super) on_error: OnError,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct BulkTaskUpdateItem {
    pub(super) id: String,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) priority: Option<String>,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) assignee: Option<String>,
    #[serde(default)]
    pub(super) input_json: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskBulkUpdateInput {
    pub(super) updates: Vec<BulkTaskUpdateItem>,
    #[serde(default)]
    pub(super) on_error: OnError,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskCancelInput {
    #[serde(alias = "task_id")]
    pub(super) id: String,
    #[serde(default)]
    pub(super) confirm: Option<String>,
    #[serde(default)]
    pub(super) dry_run: bool,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskSetPriorityInput {
    #[serde(alias = "task_id")]
    pub(super) id: String,
    pub(super) priority: String,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskSetDeadlineInput {
    #[serde(alias = "task_id")]
    pub(super) id: String,
    #[serde(default)]
    pub(super) deadline: Option<String>,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskChecklistAddInput {
    pub(super) id: String,
    pub(super) description: String,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub(super) struct TaskChecklistUpdateInput {
    pub(super) id: String,
    pub(super) item_id: String,
    pub(super) completed: bool,
    #[serde(default)]
    pub(super) project_root: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn task_control_inputs_accept_task_id_aliases() {
        let control: TaskControlInput =
            serde_json::from_value(json!({ "task_id": "TASK-1" })).expect("task control alias should deserialize");
        assert_eq!(control.id, "TASK-1");

        let cancel: TaskCancelInput =
            serde_json::from_value(json!({ "task_id": "TASK-2" })).expect("task cancel alias should deserialize");
        assert_eq!(cancel.id, "TASK-2");

        let priority: TaskSetPriorityInput = serde_json::from_value(json!({
            "task_id": "TASK-3",
            "priority": "critical"
        }))
        .expect("task priority alias should deserialize");
        assert_eq!(priority.id, "TASK-3");

        let deadline: TaskSetDeadlineInput =
            serde_json::from_value(json!({ "task_id": "TASK-4" })).expect("task deadline alias should deserialize");
        assert_eq!(deadline.id, "TASK-4");
    }
}
