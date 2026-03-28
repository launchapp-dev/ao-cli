use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use orchestrator_core::{ServiceHub, OrchestratorTask, OrchestratorWorkflow, RequirementItem, TaskStatus, WorkflowStatus};
use serde::Serialize;

use crate::print_value;

const NOW_SCHEMA: &str = "ao.now.v1";

#[derive(Debug, Clone, Serialize)]
struct NowSurface {
    schema: &'static str,
    generated_at: DateTime<Utc>,
    next_task: Option<NextTaskItem>,
    active_workflows: Vec<ActiveWorkflowItem>,
    blocked_items: Vec<BlockedItem>,
    stale_items: Vec<StaleItem>,
}

#[derive(Debug, Clone, Serialize)]
struct NextTaskItem {
    id: String,
    title: String,
    priority: String,
    status: String,
    linked_requirements: Vec<LinkedRequirement>,
}

#[derive(Debug, Clone, Serialize)]
struct LinkedRequirement {
    id: String,
    title: String,
    priority: String,
}

#[derive(Debug, Clone, Serialize)]
struct ActiveWorkflowItem {
    id: String,
    task_id: String,
    task_title: String,
    status: String,
    current_phase: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BlockedItem {
    id: String,
    item_type: String,
    title: String,
    blocked_reason: Option<String>,
    blocked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
struct StaleItem {
    id: String,
    item_type: String,
    title: String,
    last_updated: DateTime<Utc>,
    days_stale: u32,
}

pub(crate) async fn handle_now(hub: Arc<dyn ServiceHub>, json: bool) -> Result<()> {
    let tasks_service = hub.tasks();
    let workflows_service = hub.workflows();
    let requirements_service = hub.planning();

    let (next_task, all_tasks, all_workflows, all_requirements) = tokio::join!(
        tasks_service.next_task(),
        tasks_service.list(),
        workflows_service.list(),
        requirements_service.list_requirements()
    );

    let next_task = next_task.ok().flatten();
    let all_tasks = all_tasks.unwrap_or_default();
    let all_workflows = all_workflows.unwrap_or_default();
    let all_requirements = all_requirements.unwrap_or_default();

    let now_surface = build_now_surface(next_task, &all_tasks, &all_workflows, &all_requirements);

    if json {
        return print_value(now_surface, true);
    }

    println!("{}", render_now_surface(&now_surface));
    Ok(())
}

fn build_now_surface(
    next_task: Option<OrchestratorTask>,
    all_tasks: &[OrchestratorTask],
    all_workflows: &[OrchestratorWorkflow],
    all_requirements: &[RequirementItem],
) -> NowSurface {
    let next_task_item = next_task.as_ref().map(|task| {
        let linked_reqs = task
            .linked_requirements
            .iter()
            .filter_map(|req_id| all_requirements.iter().find(|r| r.id == *req_id).map(|r| LinkedRequirement {
                id: r.id.clone(),
                title: r.title.clone(),
                priority: format!("{:?}", r.priority),
            }))
            .collect();

        NextTaskItem {
            id: task.id.clone(),
            title: task.title.clone(),
            priority: format!("{:?}", task.priority),
            status: format!("{:?}", task.status),
            linked_requirements: linked_reqs,
        }
    });

    let active_workflows = all_workflows
        .iter()
        .filter(|w| w.status == WorkflowStatus::Running)
        .map(|w| {
            let task_title = all_tasks
                .iter()
                .find(|t| t.id == w.task_id)
                .map(|t| t.title.clone())
                .unwrap_or_else(|| "Unknown task".to_string());

            ActiveWorkflowItem {
                id: w.id.clone(),
                task_id: w.task_id.clone(),
                task_title,
                status: format!("{:?}", w.status),
                current_phase: w.current_phase.clone(),
            }
        })
        .collect();

    let blocked_items = all_tasks
        .iter()
        .filter(|t| t.status.is_blocked())
        .map(|t| BlockedItem {
            id: t.id.clone(),
            item_type: "task".to_string(),
            title: t.title.clone(),
            blocked_reason: t.blocked_reason.clone(),
            blocked_at: t.blocked_at,
        })
        .collect();

    let now = Utc::now();
    let stale_items = all_tasks
        .iter()
        .filter_map(|t| {
            let days_since_update = (now.signed_duration_since(t.metadata.updated_at).num_seconds() / 86400) as u32;
            if days_since_update > 7 && t.status == TaskStatus::InProgress {
                Some(StaleItem {
                    id: t.id.clone(),
                    item_type: "task".to_string(),
                    title: t.title.clone(),
                    last_updated: t.metadata.updated_at,
                    days_stale: days_since_update,
                })
            } else {
                None
            }
        })
        .collect();

    NowSurface { schema: NOW_SCHEMA, generated_at: Utc::now(), next_task: next_task_item, active_workflows, blocked_items, stale_items }
}

fn render_now_surface(surface: &NowSurface) -> String {
    let mut output = String::new();

    output.push_str("AO Now/Inbox Surface\n");
    output.push_str(&format!("Generated At: {}\n\n", surface.generated_at.to_rfc3339()));

    output.push_str("Next Task\n");
    if let Some(ref next) = surface.next_task {
        output.push_str(&format!("  id: {}\n", next.id));
        output.push_str(&format!("  title: {}\n", next.title));
        output.push_str(&format!("  priority: {}\n", next.priority));
        output.push_str(&format!("  status: {}\n", next.status));
        if !next.linked_requirements.is_empty() {
            output.push_str("  linked_requirements:\n");
            for req in &next.linked_requirements {
                output.push_str(&format!("    - id={} title={} priority={}\n", req.id, req.title, req.priority));
            }
        }
    } else {
        output.push_str("  none\n");
    }
    output.push_str("\n");

    output.push_str("Active Workflows\n");
    if surface.active_workflows.is_empty() {
        output.push_str("  none\n");
    } else {
        for w in &surface.active_workflows {
            output.push_str(&format!("  - id={} task_id={} task_title={} phase={}\n",
                w.id, w.task_id, w.task_title, w.current_phase.as_deref().unwrap_or("unknown")));
        }
    }
    output.push_str("\n");

    output.push_str("Blocked Items\n");
    if surface.blocked_items.is_empty() {
        output.push_str("  none\n");
    } else {
        for item in &surface.blocked_items {
            output.push_str(&format!("  - id={} type={} title={}\n", item.id, item.item_type, item.title));
            if let Some(reason) = &item.blocked_reason {
                output.push_str(&format!("    reason: {}\n", reason));
            }
        }
    }
    output.push_str("\n");

    output.push_str("Stale Items (>7 days in progress)\n");
    if surface.stale_items.is_empty() {
        output.push_str("  none\n");
    } else {
        for item in &surface.stale_items {
            output.push_str(&format!("  - id={} type={} title={} days_stale={}\n",
                item.id, item.item_type, item.title, item.days_stale));
        }
    }

    output
}

#[cfg(test)]
mod tests;
