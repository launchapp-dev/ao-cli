use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::{EmWorkQueueEntry, EmWorkQueueEntryStatus, EmWorkQueueState};

const EM_WORK_QUEUE_STATE_FILE: &str = "em-work-queue.json";

pub fn em_work_queue_state_path(project_root: &str) -> Result<PathBuf> {
    let runtime_root = protocol::scoped_state_root(std::path::Path::new(project_root))
        .ok_or_else(|| anyhow!("failed to resolve scoped state root for {project_root}"))?;
    Ok(runtime_root
        .join("scheduler")
        .join(EM_WORK_QUEUE_STATE_FILE))
}

pub fn load_em_work_queue_state(project_root: &str) -> Result<Option<EmWorkQueueState>> {
    let path = em_work_queue_state_path(project_root)?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).with_context(|| {
        format!(
            "failed to read EM work queue state file at {}",
            path.display()
        )
    })?;
    if content.trim().is_empty() {
        return Ok(Some(EmWorkQueueState::default()));
    }

    serde_json::from_str::<EmWorkQueueState>(&content)
        .map(Some)
        .or_else(|_| {
            serde_json::from_str::<Vec<EmWorkQueueEntry>>(&content)
                .map(|entries| Some(EmWorkQueueState { entries }))
        })
        .with_context(|| {
            format!(
                "failed to parse EM work queue state file at {}",
                path.display()
            )
        })
}

pub fn save_em_work_queue_state(project_root: &str, state: &EmWorkQueueState) -> Result<()> {
    let path = em_work_queue_state_path(project_root)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if state.entries.is_empty() {
        if path.exists() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }

    let payload = serde_json::to_string_pretty(state)?;
    let tmp_path = path.with_file_name(format!(
        "{}.{}.tmp",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(EM_WORK_QUEUE_STATE_FILE),
        Uuid::new_v4()
    ));
    fs::write(&tmp_path, payload)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn mark_em_work_queue_entry_assigned(
    project_root: &str,
    task_id: &str,
    workflow_id: &str,
) -> Result<bool> {
    let Some(mut state) = load_em_work_queue_state(project_root)? else {
        return Ok(false);
    };

    let mut updated = false;
    for entry in &mut state.entries {
        if entry.task_id != task_id {
            continue;
        }
        if entry.status != EmWorkQueueEntryStatus::Pending {
            continue;
        }
        entry.status = EmWorkQueueEntryStatus::Assigned;
        entry.workflow_id = Some(workflow_id.to_string());
        entry.assigned_at = Some(Utc::now().to_rfc3339());
        updated = true;
        break;
    }

    if updated {
        save_em_work_queue_state(project_root, &state)?;
    }

    Ok(updated)
}

fn remove_terminal_em_work_queue_entry(
    project_root: &str,
    task_id: &str,
    workflow_id: Option<&str>,
) -> Result<usize> {
    let Some(mut state) = load_em_work_queue_state(project_root)? else {
        return Ok(0);
    };

    let before = state.entries.len();
    state.entries.retain(|entry| {
        if entry.task_id != task_id {
            return true;
        }
        if entry.status != EmWorkQueueEntryStatus::Assigned {
            return true;
        }
        if let Some(workflow_id) = workflow_id {
            if entry
                .workflow_id
                .as_deref()
                .is_some_and(|entry_workflow_id| entry_workflow_id != workflow_id)
            {
                return true;
            }
        }
        false
    });
    let removed = before.saturating_sub(state.entries.len());
    if removed > 0 {
        save_em_work_queue_state(project_root, &state)?;
    }
    Ok(removed)
}

pub fn remove_terminal_em_work_queue_entry_non_fatal(
    project_root: &str,
    task_id: &str,
    workflow_id: Option<&str>,
) {
    if let Err(error) = remove_terminal_em_work_queue_entry(project_root, task_id, workflow_id) {
        eprintln!(
            "{}: failed to remove terminal EM queue entry for task {}: {}",
            protocol::ACTOR_DAEMON,
            task_id,
            error
        );
    }
}
