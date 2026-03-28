use crate::types::{OrchestratorTask, OrchestratorWorkflow};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::services::ServiceHub;

/// Default staleness threshold for attention items (24 hours)
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 60 * 60;

/// Availability envelope for optional payload fields.
/// Wraps data fields with availability tracking and error reporting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityEnvelope<T> {
    /// Whether the data was successfully retrieved
    pub available: bool,
    /// The data, if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// Error message if data retrieval failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> AvailabilityEnvelope<T> {
    /// Create a successful envelope with data
    pub fn available(data: T) -> Self {
        Self { available: true, data: Some(data), error: None }
    }

    /// Create an unsuccessful envelope with an error
    pub fn error(error: impl Into<String>) -> Self {
        Self { available: false, data: None, error: Some(error.into()) }
    }
}

/// A blocked task with the reason it's blocked
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedItem {
    /// The task that is blocked
    pub task: OrchestratorTask,
    /// Reason for the block
    pub reason: Option<String>,
}

/// An active workflow with its associated task and current phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveWorkflowItem {
    /// The workflow
    pub workflow: OrchestratorWorkflow,
    /// The task associated with this workflow
    pub task: OrchestratorTask,
    /// Current phase being executed
    pub current_phase: Option<String>,
}

/// The inbox payload containing next action, blocked items, active workflows, and attention items.
/// Schema: `ao.inbox.v1`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxPayload {
    /// Next action to take (highest priority ready task)
    #[serde(rename = "next_action")]
    pub next_action: AvailabilityEnvelope<Option<OrchestratorTask>>,
    /// Tasks that are blocked with reasons
    pub blocked_items: AvailabilityEnvelope<Vec<BlockedItem>>,
    /// Active workflows with their associated tasks
    pub active_workflows: AvailabilityEnvelope<Vec<ActiveWorkflowItem>>,
    /// Tasks in progress with no active agent beyond staleness threshold
    pub attention_items: AvailabilityEnvelope<Vec<OrchestratorTask>>,
}

/// Collects inbox data from existing services without direct file reads.
///
/// # Arguments
/// * `hub` - The service hub providing access to tasks and workflows
/// * `project_root` - The project root path
/// * `staleness` - Duration threshold for marking tasks as stale
///
/// # Returns
/// An InboxPayload with availability envelopes around each field to ensure
/// partial failures don't abort the entire payload.
pub async fn collect_inbox(
    hub: Arc<dyn ServiceHub>,
    _project_root: &str,
    staleness: Duration,
) -> InboxPayload {
    let mut result = InboxPayload {
        next_action: AvailabilityEnvelope::available(None),
        blocked_items: AvailabilityEnvelope::available(Vec::new()),
        active_workflows: AvailabilityEnvelope::available(Vec::new()),
        attention_items: AvailabilityEnvelope::available(Vec::new()),
    };

    // Collect next action
    match hub.tasks().next_task().await {
        Ok(task) => {
            result.next_action = AvailabilityEnvelope::available(task);
        }
        Err(e) => {
            result.next_action = AvailabilityEnvelope::error(e.to_string());
        }
    }

    // Collect blocked items
    match hub.tasks().list_filtered(crate::types::TaskFilter {
        status: Some(crate::types::TaskStatus::Blocked),
        ..Default::default()
    }).await {
        Ok(blocked_tasks) => {
            let blocked_items: Vec<BlockedItem> = blocked_tasks
                .into_iter()
                .map(|task| BlockedItem {
                    reason: task.blocked_reason.clone(),
                    task,
                })
                .collect();
            result.blocked_items = AvailabilityEnvelope::available(blocked_items);
        }
        Err(e) => {
            result.blocked_items = AvailabilityEnvelope::error(e.to_string());
        }
    }

    // Collect active workflows
    match hub
        .workflows()
        .query(crate::types::WorkflowQuery {
            filter: crate::types::WorkflowFilter {
                status: Some(crate::types::WorkflowStatus::Running),
                ..Default::default()
            },
            page: crate::types::ListPageRequest::unbounded(),
            sort: Default::default(),
        })
        .await
    {
        Ok(page) => {
            let mut active_workflow_items = Vec::new();
            for workflow in page.items {
                match hub.tasks().get(&workflow.task_id).await {
                    Ok(task) => {
                        active_workflow_items.push(ActiveWorkflowItem {
                            current_phase: workflow.current_phase.clone(),
                            workflow,
                            task,
                        });
                    }
                    Err(_) => {
                        // Skip workflows where task can't be found
                    }
                }
            }
            result.active_workflows = AvailabilityEnvelope::available(active_workflow_items);
        }
        Err(e) => {
            result.active_workflows = AvailabilityEnvelope::error(e.to_string());
        }
    }

    // Collect attention items (in-progress tasks with no active agent beyond staleness threshold)
    match hub
        .tasks()
        .list_filtered(crate::types::TaskFilter {
            status: Some(crate::types::TaskStatus::InProgress),
            ..Default::default()
        })
        .await
    {
        Ok(in_progress_tasks) => {
            let now = Utc::now();
            let stale_threshold = now - staleness;

            let attention_items: Vec<OrchestratorTask> = in_progress_tasks
                .into_iter()
                .filter(|task| {
                    // Check if task has no active agent and is older than staleness threshold
                    // If started_at is None or older than threshold, it needs attention
                    if let Some(started_at) = task.metadata.started_at {
                        started_at < stale_threshold
                    } else {
                        // If we don't know when it started, include it
                        true
                    }
                })
                .collect();

            result.attention_items = AvailabilityEnvelope::available(attention_items);
        }
        Err(e) => {
            result.attention_items = AvailabilityEnvelope::error(e.to_string());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::InMemoryServiceHub;
    use crate::types::{Priority, TaskMetadata, TaskStatus, TaskType};
    use chrono::Utc;

    #[tokio::test]
    async fn test_collect_inbox_basic() {
        let hub = Arc::new(InMemoryServiceHub::new());

        let result =
            collect_inbox(hub, "/test", Duration::hours(24)).await;

        // All fields should be available
        assert!(result.next_action.available);
        assert!(result.blocked_items.available);
        assert!(result.active_workflows.available);
        assert!(result.attention_items.available);

        // No data initially
        assert!(result.next_action.data.as_ref().unwrap().is_none());
        assert!(result.blocked_items.data.as_ref().unwrap().is_empty());
        assert!(result.active_workflows.data.as_ref().unwrap().is_empty());
        assert!(result.attention_items.data.as_ref().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_availability_envelope_serialization() {
        let envelope: AvailabilityEnvelope<String> = AvailabilityEnvelope::available("test".to_string());
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"available\":true"));
        assert!(json.contains("\"data\":\"test\""));
        assert!(!json.contains("\"error\""));

        let error_envelope: AvailabilityEnvelope<String> = AvailabilityEnvelope::error("something went wrong");
        let json = serde_json::to_string(&error_envelope).unwrap();
        assert!(json.contains("\"available\":false"));
        assert!(json.contains("\"error\":\"something went wrong\""));
    }

    #[test]
    fn test_blocked_item_structure() {
        let task = OrchestratorTask {
            id: "task-1".to_string(),
            title: "Test Task".to_string(),
            description: "A test task".to_string(),
            task_type: TaskType::Feature,
            status: TaskStatus::Blocked,
            blocked_reason: Some("Waiting for input".to_string()),
            blocked_at: Some(Utc::now()),
            blocked_phase: None,
            blocked_by: None,
            priority: Priority::High,
            risk: Default::default(),
            scope: Default::default(),
            complexity: Default::default(),
            impact_area: vec![],
            assignee: Default::default(),
            estimated_effort: None,
            linked_requirements: vec![],
            linked_architecture_entities: vec![],
            dependencies: vec![],
            checklist: vec![],
            tags: vec![],
            workflow_metadata: Default::default(),
            worktree_path: None,
            branch_name: None,
            metadata: TaskMetadata {
                created_at: Utc::now(),
                updated_at: Utc::now(),
                created_by: "system".to_string(),
                updated_by: "system".to_string(),
                started_at: None,
                completed_at: None,
                version: 1,
            },
            deadline: None,
            paused: false,
            cancelled: false,
            resolution: None,
            resource_requirements: Default::default(),
            consecutive_dispatch_failures: None,
            last_dispatch_failure_at: None,
            dispatch_history: vec![],
        };

        let blocked_item = BlockedItem { task: task.clone(), reason: Some("Waiting for input".to_string()) };

        let json = serde_json::to_string(&blocked_item).unwrap();
        let deserialized: BlockedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task.id, "task-1");
        assert_eq!(deserialized.reason, Some("Waiting for input".to_string()));
    }
}
