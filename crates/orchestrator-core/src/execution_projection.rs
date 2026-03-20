mod project_requirement_workflow_status;
mod project_task_terminal_workflow_status;
mod projector_registry;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::{Datelike, Utc};
use protocol::SubjectExecutionFact;

use crate::{
    load_schedule_state, save_schedule_state, services::ServiceHub, OrchestratorTask, TaskStatus, WorkflowStatus,
};

pub use project_requirement_workflow_status::project_requirement_workflow_status;
pub use project_task_terminal_workflow_status::project_task_terminal_workflow_status;
pub use projector_registry::{
    builtin_execution_projector_registry, execution_fact_subject_kind, ExecutionProjector, ExecutionProjectorRegistry,
};

pub const WORKFLOW_RUNNER_BLOCKED_PREFIX: &str = "workflow runner failed: ";
pub const WORKFLOW_RUNNER_CANCELLED_PREFIX: &str = "workflow runner cancelled: ";
pub const WORKFLOW_RUNNER_EXITED_PREFIX: &str = "workflow runner exited without workflow status";
pub const MAX_RUNNER_FAILURE_RESETS: u32 = 3;

/// Returns true when a task is blocked specifically because a workflow runner
/// exited with a non-zero status (a transient infrastructure failure), not
/// because of a dependency gate or human-required input.
pub fn is_workflow_runner_blocked(task: &OrchestratorTask) -> bool {
    if !task.status.is_blocked() || !task.paused {
        return false;
    }
    task.blocked_reason.as_deref().is_some_and(|reason| {
        reason.starts_with(WORKFLOW_RUNNER_BLOCKED_PREFIX)
            || reason.starts_with(WORKFLOW_RUNNER_CANCELLED_PREFIX)
            || reason.starts_with(WORKFLOW_RUNNER_EXITED_PREFIX)
    })
}

/// Resets a runner-blocked task back to `Ready` so the daemon can retry it.
///
/// Uses `consecutive_dispatch_failures` to track how many times this task has
/// been reset.  Once the count reaches `MAX_RUNNER_FAILURE_RESETS` the task is
/// left blocked and an error message is logged, signalling that human
/// intervention is needed.
pub async fn reconcile_runner_blocked_task(hub: Arc<dyn ServiceHub>, task: &OrchestratorTask) -> anyhow::Result<bool> {
    let count = task.consecutive_dispatch_failures.unwrap_or(0).saturating_add(1);

    if count > MAX_RUNNER_FAILURE_RESETS {
        eprintln!(
            "{}: task {} has been reset {} times after runner failures — escalating to human review (blocked_reason={:?})",
            protocol::ACTOR_DAEMON,
            task.id,
            count,
            task.blocked_reason,
        );
        return Ok(false);
    }

    let mut updated = task.clone();
    updated.status = TaskStatus::Ready;
    updated.paused = false;
    updated.blocked_reason = None;
    updated.blocked_at = None;
    updated.blocked_phase = None;
    updated.blocked_by = None;
    updated.consecutive_dispatch_failures = Some(count);
    updated.last_dispatch_failure_at = Some(Utc::now().to_rfc3339());
    updated.metadata.updated_at = Utc::now();
    updated.metadata.updated_by = protocol::ACTOR_DAEMON.to_string();
    updated.metadata.version = updated.metadata.version.saturating_add(1);
    hub.tasks().replace(updated).await?;
    eprintln!(
        "{}: unblocked task {} after runner failure (reset #{}/{}, previous reason: {:?})",
        protocol::ACTOR_DAEMON,
        task.id,
        count,
        MAX_RUNNER_FAILURE_RESETS,
        task.blocked_reason,
    );
    Ok(true)
}

pub async fn project_task_status(hub: Arc<dyn ServiceHub>, task_id: &str, status: TaskStatus) -> Result<()> {
    hub.tasks().set_status(task_id, status, false).await?;
    Ok(())
}

pub async fn project_task_blocked_with_reason(
    hub: Arc<dyn ServiceHub>,
    task: &OrchestratorTask,
    reason: String,
    blocked_by: Option<String>,
) -> Result<()> {
    let mut updated = task.clone();
    updated.status = TaskStatus::Blocked;
    updated.paused = true;
    updated.blocked_reason = Some(reason);
    updated.blocked_at = Some(Utc::now());
    updated.blocked_phase = None;
    updated.blocked_by = blocked_by;
    updated.metadata.updated_at = Utc::now();
    updated.metadata.updated_by = protocol::ACTOR_DAEMON.to_string();
    updated.metadata.version = updated.metadata.version.saturating_add(1);
    hub.tasks().replace(updated).await?;
    Ok(())
}

pub async fn project_task_dispatch_failure(
    hub: Arc<dyn ServiceHub>,
    task_id: &str,
    max_dispatch_retries: u32,
) -> Result<()> {
    let task = match hub.tasks().get(task_id).await {
        Ok(task) => task,
        Err(_) => {
            return project_task_status(hub, task_id, TaskStatus::Blocked).await;
        }
    };

    let count = task.consecutive_dispatch_failures.unwrap_or(0).saturating_add(1);

    if count >= max_dispatch_retries {
        let reason = format!("auto-blocked after {} consecutive dispatch failures", count);
        return project_task_blocked_with_reason(hub, &task, reason, None).await;
    }

    let mut updated = task;
    updated.consecutive_dispatch_failures = Some(count);
    updated.last_dispatch_failure_at = Some(Utc::now().to_rfc3339());
    hub.tasks().replace(updated).await?;
    project_task_status(hub, task_id, TaskStatus::Blocked).await
}

pub async fn project_task_workflow_start(
    hub: Arc<dyn ServiceHub>,
    task_id: &str,
    role: String,
    model: Option<String>,
    updated_by: String,
) -> Result<()> {
    hub.tasks().set_status(task_id, TaskStatus::InProgress, false).await?;
    hub.tasks().assign_agent(task_id, role, model, updated_by).await?;
    Ok(())
}

pub async fn project_task_execution_fact(hub: Arc<dyn ServiceHub>, _root: &str, fact: &SubjectExecutionFact) {
    let Some(task_id) = fact.task_id.as_deref() else {
        return;
    };

    if let Some(status) = fact.workflow_status {
        match status {
            WorkflowStatus::Pending | WorkflowStatus::Running | WorkflowStatus::Paused => return,
            WorkflowStatus::Completed => {
                let _ = project_task_status(hub, task_id, TaskStatus::Done).await;
                return;
            }
            WorkflowStatus::Cancelled => {
                let _ = project_task_status(hub, task_id, TaskStatus::Cancelled).await;
                return;
            }
            WorkflowStatus::Failed | WorkflowStatus::Escalated => {}
        }
    }

    if fact.success {
        let _ = project_task_status(hub, task_id, TaskStatus::Done).await;
        return;
    }

    if let Some(reason) = fact.failure_reason.clone() {
        if let Ok(task) = hub.tasks().get(task_id).await {
            let _ = project_task_blocked_with_reason(hub, &task, reason, None).await;
            return;
        }
    }

    let _ = project_task_status(hub, task_id, TaskStatus::Blocked).await;
}

pub async fn project_execution_fact(hub: Arc<dyn ServiceHub>, root: &str, fact: &SubjectExecutionFact) -> bool {
    match builtin_execution_projector_registry().project(hub.clone(), root, fact).await {
        Ok(projected) => projected,
        Err(err) => {
            let kind = execution_fact_subject_kind(fact).unwrap_or("unknown");
            eprintln!(
                "{}: failed to project execution fact for subject '{}' (kind='{}'): {}",
                protocol::ACTOR_DAEMON,
                fact.subject_id,
                kind,
                err
            );
            true
        }
    }
}

pub fn project_schedule_dispatch_attempt(root: &str, schedule_id: &str, run_at: chrono::DateTime<Utc>, status: &str) {
    update_schedule_state(root, schedule_id, Some(run_at), status, true);
}

pub(crate) fn project_schedule_completion_status(root: &str, schedule_id: &str, status: &str) {
    update_schedule_state(root, schedule_id, None, status, false);
}

pub fn project_schedule_execution_fact(root: &str, fact: &SubjectExecutionFact) {
    let Some(schedule_id) = fact.schedule_id.as_deref() else {
        return;
    };

    project_schedule_completion_status(root, schedule_id, fact.completion_status());

    // Check if this failure is due to a Claude rate-limit, and if so, pause the schedule
    if let Some(ref failure_reason) = fact.failure_reason {
        if let Some(reset_time) = parse_rate_limit_reset_time(failure_reason) {
            pause_schedule_for_rate_limit(root, schedule_id, reset_time);
        }
    }
}

/// Rate-limit error message format: "You've hit your limit · resets Mar 24 at 10am"
/// or "workflow runner failed: You've hit your limit · resets Mar 24 at 10am"
const RATE_LIMIT_SUBSTRING: &str = "You've hit your limit";

/// Parses the rate-limit reset timestamp from an error message.
///
/// Returns `Some(DateTime<Utc>)` if the message contains a rate-limit indicator,
/// otherwise returns `None`.
pub fn parse_rate_limit_reset_time(error_message: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if !error_message.contains(RATE_LIMIT_SUBSTRING) {
        return None;
    }

    // Extract the reset time portion after "· resets "
    let reset_marker = "· resets ";
    let Some(reset_pos) = error_message.find(reset_marker) else {
        return None;
    };

    let reset_str = &error_message[reset_pos + reset_marker.len()..];
    // The format is typically "Mar 24 at 10am" or "Mar 24 at 10:00"
    parse_rate_limit_date_string(reset_str)
}

/// Parses a rate-limit date string like "Mar 24 at 10am" or "Mar 24 at 10:00" into a DateTime<Utc>.
fn parse_rate_limit_date_string(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    // Trim any trailing text
    let s = s.trim();
    let parts: Vec<&str> = s.splitn(2, " at ").collect();
    if parts.len() != 2 {
        return None;
    }

    let date_part = parts[0].trim(); // e.g., "Mar 24"
    let time_part = parts[1].trim(); // e.g., "10am" or "10:00"

    // Parse month and day
    let date_components: Vec<&str> = date_part.split_whitespace().collect();
    if date_components.len() != 2 {
        return None;
    }

    let month_str = date_components[0];
    let day_str = date_components[1];

    let month = match month_str {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };

    let day: u32 = day_str.parse().ok()?;
    if day == 0 || day > 31 {
        return None;
    }

    // Parse time (handle "10am", "10pm", "10:00am", "10:00pm", etc.)
    // IMPORTANT: Check for AM/PM indicator BEFORE trimming, as trimming removes non-digits
    let is_pm = time_part.to_lowercase().contains("pm");
    let time_part_trimmed = time_part.trim_end_matches(|c: char| !c.is_ascii_digit() && c != ':');
    let time_digits: String = time_part_trimmed.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();

    let (hour, minute) = if time_digits.contains(':') {
        let time_parts: Vec<&str> = time_digits.split(':').collect();
        if time_parts.len() != 2 {
            return None;
        }
        let h: u32 = time_parts[0].parse().ok()?;
        let m: u32 = time_parts[1].parse().ok()?;
        (h, m)
    } else {
        let h: u32 = time_digits.parse().ok()?;
        (h, 0)
    };

    // Convert to 24-hour format
    // Special case: 12am = 0:00, 12pm = 12:00
    let hour_24 = if hour == 12 {
        if is_pm { 12 } else { 0 }
    } else if is_pm {
        hour + 12
    } else {
        hour
    };

    // Use the current year and assume the reset is either in the current year or next year
    let now = chrono::Utc::now();
    let year = now.year();

    // Try current year first, then next year if the date hasn't passed yet this year
    let make_date = |year| {
        chrono::NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|d| d.and_hms_opt(hour_24, minute, 0))
            .map(|ndt| ndt.and_utc())
    };

    if let Some(dt) = make_date(year) {
        // If the parsed date is in the past, it must be next year
        if dt > now {
            return Some(dt);
        }
    }

    // Try next year
    make_date(year + 1)
}

/// Pauses a schedule until the specified reset time due to a rate-limit.
pub fn pause_schedule_for_rate_limit(root: &str, schedule_id: &str, reset_time: chrono::DateTime<chrono::Utc>) {
    let project_root = Path::new(root);
    let mut state = load_schedule_state(project_root).unwrap_or_default();
    let entry = state.schedules.entry(schedule_id.to_string()).or_default();

    // Only update if not already paused, or if the new reset time is further in the future
    let should_update = match entry.paused_until {
        Some(existing) => reset_time > existing,
        None => true,
    };

    if should_update {
        entry.paused_until = Some(reset_time);
        let _ = save_schedule_state(project_root, &state);
        eprintln!(
            "{}: Schedule '{}' paused until {} due to rate limit",
            protocol::ACTOR_DAEMON,
            schedule_id,
            reset_time.to_rfc3339()
        );
    }
}

fn update_schedule_state(
    root: &str,
    schedule_id: &str,
    run_at: Option<chrono::DateTime<Utc>>,
    status: &str,
    increment_run_count: bool,
) {
    let project_root = Path::new(root);
    let mut state = load_schedule_state(project_root).unwrap_or_default();
    let entry = state.schedules.entry(schedule_id.to_string()).or_default();
    if let Some(run_at) = run_at {
        entry.last_run = Some(run_at);
    }
    if increment_run_count {
        entry.run_count = entry.run_count.saturating_add(1);
    }
    entry.last_status = status.to_string();
    let _ = save_schedule_state(project_root, &state);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Datelike, Timelike, Utc};
    use protocol::{SubjectExecutionFact, SUBJECT_KIND_TASK};

    use super::{
        execution_fact_subject_kind, is_workflow_runner_blocked, parse_rate_limit_reset_time, pause_schedule_for_rate_limit,
        project_execution_fact, project_schedule_execution_fact, reconcile_runner_blocked_task, MAX_RUNNER_FAILURE_RESETS,
    };
    use crate::{
        load_schedule_state, services::ServiceHub, InMemoryServiceHub, OrchestratorTask, Priority, ResourceRequirements, Scope,
        TaskMetadata, TaskStatus, TaskType, WorkflowMetadata,
    };
    use tempfile::tempdir;

    async fn upsert_task(hub: &Arc<InMemoryServiceHub>, id: &str, status: TaskStatus) -> OrchestratorTask {
        let now = Utc::now();
        let task = OrchestratorTask {
            id: id.to_string(),
            title: format!("Task {id}"),
            description: "Execution projection".to_string(),
            task_type: TaskType::Feature,
            status,
            blocked_reason: None,
            blocked_at: None,
            blocked_phase: None,
            blocked_by: None,
            priority: Priority::Medium,
            risk: crate::RiskLevel::Medium,
            scope: Scope::Medium,
            complexity: crate::Complexity::default(),
            impact_area: Vec::new(),
            assignee: crate::Assignee::Unassigned,
            estimated_effort: None,
            linked_requirements: Vec::new(),
            linked_architecture_entities: Vec::new(),
            dependencies: Vec::new(),
            checklist: Vec::new(),
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
        };

        hub.tasks().replace(task.clone()).await.expect("upsert task");
        task
    }

    #[tokio::test]
    async fn project_execution_fact_uses_task_projector_for_subject_kind() {
        let hub = Arc::new(InMemoryServiceHub::new());
        upsert_task(&hub, "TASK-1", TaskStatus::Ready).await;

        let fact = SubjectExecutionFact {
            subject_id: "TASK-1".to_string(),
            subject_kind: Some(SUBJECT_KIND_TASK.to_string()),
            task_id: Some("TASK-1".to_string()),
            workflow_id: None,
            workflow_ref: None,
            workflow_status: None,
            schedule_id: None,
            exit_code: Some(0),
            success: true,
            failure_reason: None,
            runner_events: Vec::new(),
        };

        let projected = project_execution_fact(hub.clone(), ".", &fact).await;

        assert!(projected);
        let updated = hub.tasks().get("TASK-1").await.expect("task should exist");
        assert_eq!(updated.status, TaskStatus::Done);
    }

    #[tokio::test]
    async fn project_execution_fact_preserves_legacy_task_fact_compatibility() {
        let hub = Arc::new(InMemoryServiceHub::new());
        upsert_task(&hub, "TASK-2", TaskStatus::Ready).await;

        let fact = SubjectExecutionFact {
            subject_id: "TASK-2".to_string(),
            subject_kind: None,
            task_id: Some("TASK-2".to_string()),
            workflow_id: None,
            workflow_ref: None,
            workflow_status: None,
            schedule_id: None,
            exit_code: Some(0),
            success: true,
            failure_reason: None,
            runner_events: Vec::new(),
        };

        let projected = project_execution_fact(hub.clone(), ".", &fact).await;

        assert!(projected);
        assert_eq!(execution_fact_subject_kind(&fact), Some(SUBJECT_KIND_TASK));
        let updated = hub.tasks().get("TASK-2").await.expect("task should exist");
        assert_eq!(updated.status, TaskStatus::Done);
    }

    #[tokio::test]
    async fn project_execution_fact_reports_unknown_subject_kind_as_unprojected() {
        let hub = Arc::new(InMemoryServiceHub::new());
        let fact = SubjectExecutionFact {
            subject_id: "REV-1".to_string(),
            subject_kind: Some("pack.review".to_string()),
            task_id: None,
            workflow_id: None,
            workflow_ref: None,
            workflow_status: None,
            schedule_id: None,
            exit_code: Some(0),
            success: true,
            failure_reason: None,
            runner_events: Vec::new(),
        };

        let projected = project_execution_fact(hub, ".", &fact).await;

        assert!(!projected);
    }

    // --- runner-blocked reconciliation tests ---

    async fn upsert_runner_blocked_task(
        hub: &Arc<InMemoryServiceHub>,
        id: &str,
        blocked_reason: &str,
        dispatch_failures: Option<u32>,
    ) -> OrchestratorTask {
        let now = Utc::now();
        let task = OrchestratorTask {
            id: id.to_string(),
            title: format!("Task {id}"),
            description: "Runner blocked task".to_string(),
            task_type: TaskType::Feature,
            status: TaskStatus::Blocked,
            blocked_reason: Some(blocked_reason.to_string()),
            blocked_at: Some(now),
            blocked_phase: None,
            blocked_by: None,
            priority: Priority::Medium,
            risk: crate::RiskLevel::Medium,
            scope: Scope::Medium,
            complexity: crate::Complexity::default(),
            impact_area: Vec::new(),
            assignee: crate::Assignee::Unassigned,
            estimated_effort: None,
            linked_requirements: Vec::new(),
            linked_architecture_entities: Vec::new(),
            dependencies: Vec::new(),
            checklist: Vec::new(),
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
            paused: true,
            cancelled: false,
            resolution: None,
            resource_requirements: ResourceRequirements::default(),
            consecutive_dispatch_failures: dispatch_failures,
            last_dispatch_failure_at: None,
            dispatch_history: Vec::new(),
        };

        hub.tasks().replace(task.clone()).await.expect("upsert task");
        task
    }

    #[test]
    fn is_workflow_runner_blocked_detects_runner_failure() {
        let task = OrchestratorTask {
            status: TaskStatus::Blocked,
            paused: true,
            blocked_reason: Some(
                "workflow runner failed: workflow runner exited unsuccessfully with status Some(1)".to_string(),
            ),
            ..base_test_task("TASK-1")
        };
        assert!(is_workflow_runner_blocked(&task));
    }

    #[test]
    fn is_workflow_runner_blocked_detects_exited_without_status() {
        let task = OrchestratorTask {
            status: TaskStatus::Blocked,
            paused: true,
            blocked_reason: Some(
                "workflow runner exited without workflow status: workflow runner exited with status Some(1)"
                    .to_string(),
            ),
            ..base_test_task("TASK-1")
        };
        assert!(is_workflow_runner_blocked(&task));
    }

    #[test]
    fn is_workflow_runner_blocked_detects_cancelled() {
        let task = OrchestratorTask {
            status: TaskStatus::Blocked,
            paused: true,
            blocked_reason: Some("workflow runner cancelled: operator requested".to_string()),
            ..base_test_task("TASK-1")
        };
        assert!(is_workflow_runner_blocked(&task));
    }

    #[test]
    fn is_workflow_runner_blocked_rejects_non_runner_reasons() {
        let task = OrchestratorTask {
            status: TaskStatus::Blocked,
            paused: true,
            blocked_reason: Some("dependency gate: waiting on TASK-001".to_string()),
            ..base_test_task("TASK-1")
        };
        assert!(!is_workflow_runner_blocked(&task));
    }

    #[test]
    fn is_workflow_runner_blocked_rejects_not_paused() {
        let task = OrchestratorTask {
            status: TaskStatus::Blocked,
            paused: false,
            blocked_reason: Some("workflow runner failed: something".to_string()),
            ..base_test_task("TASK-1")
        };
        assert!(!is_workflow_runner_blocked(&task));
    }

    #[test]
    fn is_workflow_runner_blocked_rejects_not_blocked() {
        let task = OrchestratorTask {
            status: TaskStatus::Ready,
            paused: false,
            blocked_reason: None,
            ..base_test_task("TASK-1")
        };
        assert!(!is_workflow_runner_blocked(&task));
    }

    #[tokio::test]
    async fn reconcile_resets_runner_blocked_task_to_ready() {
        let hub = Arc::new(InMemoryServiceHub::new());
        upsert_runner_blocked_task(
            &hub,
            "TASK-R1",
            "workflow runner failed: workflow runner exited unsuccessfully with status Some(1)",
            None,
        )
        .await;

        let task = hub.tasks().get("TASK-R1").await.unwrap();
        let result = reconcile_runner_blocked_task(hub.clone(), &task).await.unwrap();

        assert!(result);
        let updated = hub.tasks().get("TASK-R1").await.unwrap();
        assert_eq!(updated.status, TaskStatus::Ready);
        assert!(!updated.paused);
        assert!(updated.blocked_reason.is_none());
    }

    #[tokio::test]
    async fn reconcile_increments_and_persists_failure_counter() {
        let hub = Arc::new(InMemoryServiceHub::new());
        upsert_runner_blocked_task(
            &hub,
            "TASK-R3",
            "workflow runner exited without workflow status: workflow runner exited with status Some(1)",
            Some(1),
        )
        .await;

        let task = hub.tasks().get("TASK-R3").await.unwrap();
        let result = reconcile_runner_blocked_task(hub.clone(), &task).await.unwrap();

        assert!(result);
        let updated = hub.tasks().get("TASK-R3").await.unwrap();
        assert_eq!(updated.status, TaskStatus::Ready);
        assert!(!updated.paused);
        assert!(updated.blocked_reason.is_none());
        assert_eq!(updated.consecutive_dispatch_failures, Some(2));
        assert!(updated.last_dispatch_failure_at.is_some());
    }

    #[tokio::test]
    async fn reconcile_stops_resetting_after_max_retries() {
        let hub = Arc::new(InMemoryServiceHub::new());
        upsert_runner_blocked_task(
            &hub,
            "TASK-R2",
            "workflow runner failed: workflow runner exited unsuccessfully with status Some(1)",
            Some(MAX_RUNNER_FAILURE_RESETS),
        )
        .await;

        let task = hub.tasks().get("TASK-R2").await.unwrap();
        let result = reconcile_runner_blocked_task(hub.clone(), &task).await.unwrap();

        assert!(!result);
        let still_blocked = hub.tasks().get("TASK-R2").await.unwrap();
        assert_eq!(still_blocked.status, TaskStatus::Blocked);
    }

    fn base_test_task(id: &str) -> OrchestratorTask {
        let now = Utc::now();
        OrchestratorTask {
            id: id.to_string(),
            title: format!("Task {id}"),
            description: String::new(),
            task_type: TaskType::Feature,
            status: TaskStatus::Backlog,
            blocked_reason: None,
            blocked_at: None,
            blocked_phase: None,
            blocked_by: None,
            priority: Priority::Medium,
            risk: crate::RiskLevel::Medium,
            scope: Scope::Medium,
            complexity: crate::Complexity::default(),
            impact_area: Vec::new(),
            assignee: crate::Assignee::Unassigned,
            estimated_effort: None,
            linked_requirements: Vec::new(),
            linked_architecture_entities: Vec::new(),
            dependencies: Vec::new(),
            checklist: Vec::new(),
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

    // --- rate-limit pause tests ---

    #[test]
    fn parse_rate_limit_reset_time_detects_rate_limit() {
        let msg = "You've hit your limit · resets Mar 24 at 10am";
        let result = parse_rate_limit_reset_time(msg);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 24);
        assert_eq!(dt.hour(), 10);
        assert_eq!(dt.minute(), 0);
    }

    #[test]
    fn parse_rate_limit_reset_time_handles_prefix() {
        let msg = "workflow runner failed: You've hit your limit · resets Dec 15 at 3pm";
        let result = parse_rate_limit_reset_time(msg);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.month(), 12);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 15);
        assert_eq!(dt.minute(), 0);
    }

    #[test]
    fn parse_rate_limit_reset_time_handles_time_with_minutes() {
        let msg = "You've hit your limit · resets Mar 24 at 10:30";
        let result = parse_rate_limit_reset_time(msg);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 24);
        assert_eq!(dt.hour(), 10);
        assert_eq!(dt.minute(), 30);
    }

    #[test]
    fn parse_rate_limit_reset_time_handles_pm() {
        let msg = "You've hit your limit · resets Mar 24 at 2pm";
        let result = parse_rate_limit_reset_time(msg);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 24);
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 0);
    }

    #[test]
    fn parse_rate_limit_reset_time_handles_12am() {
        let msg = "You've hit your limit · resets Mar 24 at 12am";
        let result = parse_rate_limit_reset_time(msg);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.hour(), 0);
    }

    #[test]
    fn parse_rate_limit_reset_time_handles_12pm() {
        let msg = "You've hit your limit · resets Mar 24 at 12pm";
        let result = parse_rate_limit_reset_time(msg);
        assert!(result.is_some());
        let dt = result.unwrap();
        assert_eq!(dt.hour(), 12);
    }

    #[test]
    fn parse_rate_limit_reset_time_returns_none_for_non_rate_limit() {
        let msg = "workflow runner failed: some other error";
        assert!(parse_rate_limit_reset_time(msg).is_none());
    }

    #[test]
    fn parse_rate_limit_reset_time_returns_none_for_empty() {
        assert!(parse_rate_limit_reset_time("").is_none());
    }

    #[test]
    fn parse_rate_limit_reset_time_returns_none_for_invalid_format() {
        // Missing "at"
        let msg = "You've hit your limit · resets Mar 24";
        assert!(parse_rate_limit_reset_time(msg).is_none());
    }

    #[test]
    fn pause_schedule_for_rate_limit_updates_state() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().to_str().unwrap();

        // Create initial state
        let now = Utc::now();
        let reset_time = now + chrono::Duration::hours(2);

        pause_schedule_for_rate_limit(root, "pr-reviewer", reset_time);

        let state = load_schedule_state(temp.path()).expect("state loads");
        let entry = state.schedules.get("pr-reviewer").expect("entry exists");
        assert!(entry.paused_until.is_some());
        assert_eq!(entry.paused_until.unwrap(), reset_time);
    }

    #[test]
    fn pause_schedule_for_rate_limit_keeps_later_reset_time() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().to_str().unwrap();

        let now = Utc::now();
        let earlier = now + chrono::Duration::hours(1);
        let later = now + chrono::Duration::hours(3);

        // First pause with earlier time
        pause_schedule_for_rate_limit(root, "pr-reviewer", earlier);

        // Try to pause with earlier time again - should keep original
        pause_schedule_for_rate_limit(root, "pr-reviewer", earlier);

        let state = load_schedule_state(temp.path()).expect("state loads");
        assert_eq!(state.schedules.get("pr-reviewer").unwrap().paused_until.unwrap(), earlier);

        // Now pause with later time - should update
        pause_schedule_for_rate_limit(root, "pr-reviewer", later);

        let state = load_schedule_state(temp.path()).expect("state loads");
        assert_eq!(state.schedules.get("pr-reviewer").unwrap().paused_until.unwrap(), later);
    }

    #[test]
    fn project_schedule_execution_fact_pauses_on_rate_limit() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().to_str().unwrap();

        let fact = SubjectExecutionFact {
            subject_id: "schedule:pr-reviewer".to_string(),
            subject_kind: Some("custom".to_string()),
            task_id: None,
            workflow_id: Some("WF-1".to_string()),
            workflow_ref: Some("pr-reviewer".to_string()),
            workflow_status: Some(protocol::orchestrator::WorkflowStatus::Failed),
            schedule_id: Some("pr-reviewer".to_string()),
            exit_code: Some(1),
            success: false,
            failure_reason: Some("workflow runner failed: You've hit your limit · resets Mar 24 at 10am".to_string()),
            runner_events: Vec::new(),
        };

        project_schedule_execution_fact(root, &fact);

        let state = load_schedule_state(temp.path()).expect("state loads");
        let entry = state.schedules.get("pr-reviewer").expect("entry exists");
        assert!(entry.paused_until.is_some());
        let paused = entry.paused_until.unwrap();
        assert_eq!(paused.month(), 3);
        assert_eq!(paused.day(), 24);
        assert_eq!(paused.hour(), 10);
    }

    #[test]
    fn project_schedule_execution_fact_ignores_non_rate_limit_failures() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().to_str().unwrap();

        let fact = SubjectExecutionFact {
            subject_id: "schedule:pr-reviewer".to_string(),
            subject_kind: Some("custom".to_string()),
            task_id: None,
            workflow_id: Some("WF-1".to_string()),
            workflow_ref: Some("pr-reviewer".to_string()),
            workflow_status: Some(protocol::orchestrator::WorkflowStatus::Failed),
            schedule_id: Some("pr-reviewer".to_string()),
            exit_code: Some(1),
            success: false,
            failure_reason: Some("workflow runner failed: some other error".to_string()),
            runner_events: Vec::new(),
        };

        project_schedule_execution_fact(root, &fact);

        let state = load_schedule_state(temp.path()).expect("state loads");
        let entry = state.schedules.get("pr-reviewer").expect("entry exists");
        assert!(entry.paused_until.is_none());
    }
}
