use super::*;
use orchestrator_core::{
    Assignee, ChecklistItem, Complexity, ImpactArea, Priority, ResourceRequirements, RiskLevel, Scope,
    TaskDependency, TaskMetadata, TaskType, WorkflowMetadata,
};
use chrono::Utc;

fn make_task(id: &str, title: &str, status: TaskStatus) -> OrchestratorTask {
    let now = Utc::now();
    OrchestratorTask {
        id: id.to_string(),
        title: title.to_string(),
        description: String::new(),
        task_type: TaskType::Feature,
        status,
        blocked_reason: None,
        blocked_at: None,
        blocked_phase: None,
        blocked_by: None,
        priority: Priority::Medium,
        risk: RiskLevel::Medium,
        scope: Scope::Medium,
        complexity: Complexity::Medium,
        impact_area: Vec::<ImpactArea>::new(),
        assignee: Assignee::Unassigned,
        estimated_effort: None,
        linked_requirements: Vec::new(),
        linked_architecture_entities: Vec::new(),
        dependencies: Vec::<TaskDependency>::new(),
        checklist: Vec::<ChecklistItem>::new(),
        tags: Vec::new(),
        workflow_metadata: WorkflowMetadata::default(),
        worktree_path: None,
        branch_name: None,
        metadata: TaskMetadata {
            created_at: now,
            updated_at: now,
            created_by: "test".to_string(),
            updated_by: "test".to_string(),
            started_at: None,
            completed_at: None,
            version: 1,
        },
        deadline: None,
        paused: false,
        cancelled: false,
        resolution: None,
        resource_requirements: ResourceRequirements::default(),
        consecutive_dispatch_failures: None,
        last_dispatch_failure_at: None,
        dispatch_history: Vec::new(),
    }
}

#[test]
fn test_build_now_surface_with_next_task() {
    let next_task = make_task("TASK-001", "Test task", TaskStatus::Ready);
    let all_tasks = vec![next_task.clone()];
    let all_workflows = vec![];
    let all_requirements = vec![];

    let surface = build_now_surface(Some(next_task), &all_tasks, &all_workflows, &all_requirements);

    assert!(surface.next_task.is_some());
    assert_eq!(surface.next_task.as_ref().unwrap().id, "TASK-001");
    assert_eq!(surface.next_task.as_ref().unwrap().title, "Test task");
}

#[test]
fn test_build_now_surface_without_next_task() {
    let all_tasks = vec![];
    let all_workflows = vec![];
    let all_requirements = vec![];

    let surface = build_now_surface(None, &all_tasks, &all_workflows, &all_requirements);

    assert!(surface.next_task.is_none());
}

#[test]
fn test_blocked_items_filtering() {
    let blocked_task = {
        let mut task = make_task("TASK-001", "Blocked task", TaskStatus::Blocked);
        task.blocked_reason = Some("Waiting for review".to_string());
        task
    };
    let ready_task = make_task("TASK-002", "Ready task", TaskStatus::Ready);

    let all_tasks = vec![blocked_task, ready_task];
    let all_workflows = vec![];
    let all_requirements = vec![];

    let surface = build_now_surface(None, &all_tasks, &all_workflows, &all_requirements);

    assert_eq!(surface.blocked_items.len(), 1);
    assert_eq!(surface.blocked_items[0].id, "TASK-001");
}
