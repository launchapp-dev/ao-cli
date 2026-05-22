use std::path::Path;

use chrono::{DateTime, Utc};
use orchestrator_daemon_runtime::{
    hold_subject, queue_snapshot, release_subject, reorder_subjects, DispatchQueueEntryStatus, QueueEntrySnapshot,
    QueueSnapshot,
};
use protocol::orchestrator::OrchestratorTask;

use super::{
    parsing::parse_json_body,
    requests::{QueueHoldRequest, QueueReleaseRequest, QueueReorderRequest},
    WebApiError, WebApiService,
};

fn throughput_last_hour(snapshot: &QueueSnapshot, now: DateTime<Utc>) -> usize {
    snapshot
        .entries
        .iter()
        .filter_map(|entry| entry.assigned_at.as_deref())
        .filter_map(|assigned_at| DateTime::parse_from_rfc3339(assigned_at).ok())
        .filter(|assigned_at| now.signed_duration_since(assigned_at.with_timezone(&Utc)).num_hours() < 1)
        .count()
}

fn avg_wait_time_secs(snapshot: &QueueSnapshot, now: DateTime<Utc>) -> i64 {
    let mut total_wait_secs = 0i64;
    let mut wait_count = 0usize;

    for entry in &snapshot.entries {
        if entry.status != DispatchQueueEntryStatus::Pending {
            continue;
        }
        let Some(dispatch) = &entry.dispatch else {
            continue;
        };
        total_wait_secs += now.signed_duration_since(dispatch.requested_at).num_seconds().max(0);
        wait_count += 1;
    }

    if wait_count == 0 {
        return 0;
    }

    total_wait_secs / wait_count as i64
}

fn queue_entry_json(
    entry: &QueueEntrySnapshot,
    task_lookup: &std::collections::HashMap<&str, &OrchestratorTask>,
    position: usize,
    now: DateTime<Utc>,
) -> serde_json::Value {
    let task = entry.task_id.as_deref().and_then(|task_id| task_lookup.get(task_id));

    let wait_time =
        entry.dispatch.as_ref().map(|d| now.signed_duration_since(d.requested_at).num_seconds().max(0) as f64);

    serde_json::json!({
        "subject_id": entry.subject_id,
        "task_id": entry.task_id,
        "dispatch": entry.dispatch,
        "status": entry.status,
        "workflow_id": entry.workflow_id,
        "assigned_at": entry.assigned_at,
        "held_at": entry.held_at,
        "position": position,
        "wait_time": wait_time,
        "task": task.map(|t| serde_json::json!({
            "id": t.id,
            "title": t.title,
            "description": t.description,
            "status": t.status,
            "priority": t.priority,
        }))
    })
}

impl WebApiService {
    pub async fn queue_list(&self) -> Result<serde_json::Value, WebApiError> {
        let project_root = &self.context.project_root;
        if let Some(wire) = try_queue_list_via_control(project_root).await {
            // wire shape differs from local; v0.4.x cleanup
            return Ok(serde_json::to_value(wire).unwrap_or(serde_json::Value::Null));
        }
        let snapshot = queue_snapshot(project_root)
            .map_err(|e| WebApiError::new("internal_error", format!("failed to load queue: {}", e), 1))?;

        let tasks = self.context.hub.tasks().list().await.unwrap_or_default();
        let task_lookup =
            tasks.iter().map(|task| (task.id.as_str(), task)).collect::<std::collections::HashMap<_, _>>();

        let now = Utc::now();
        let entries = snapshot
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| queue_entry_json(entry, &task_lookup, i + 1, now))
            .collect::<Vec<_>>();

        Ok(serde_json::json!({
            "entries": entries,
            "stats": {
                "total": snapshot.stats.total,
                "pending": snapshot.stats.pending,
                "assigned": snapshot.stats.assigned,
                "held": snapshot.stats.held
            }
        }))
    }

    pub async fn queue_stats(&self) -> Result<serde_json::Value, WebApiError> {
        let project_root = &self.context.project_root;
        if let Some(wire) = try_queue_stats_via_control(project_root).await {
            // wire shape differs from local; v0.4.x cleanup
            return Ok(serde_json::to_value(wire).unwrap_or(serde_json::Value::Null));
        }
        let snapshot = queue_snapshot(project_root)
            .map_err(|e| WebApiError::new("internal_error", format!("failed to load queue: {}", e), 1))?;
        let now = Utc::now();

        Ok(serde_json::json!({
            "depth": snapshot.stats.total,
            "pending": snapshot.stats.pending,
            "assigned": snapshot.stats.assigned,
            "held": snapshot.stats.held,
            "throughput_last_hour": throughput_last_hour(&snapshot, now),
            "avg_wait_time_secs": avg_wait_time_secs(&snapshot, now),
        }))
    }

    pub async fn queue_reorder(&self, body: serde_json::Value) -> Result<serde_json::Value, WebApiError> {
        let request: QueueReorderRequest = parse_json_body(body)?;
        let project_root = &self.context.project_root;

        // Wire `queue/reorder` is single-id + anchor + position while the
        // local path takes a Vec of subject ids. The shapes are not
        // compatible, so route stays local. Lifting requires adding a
        // multi-id variant to `QueueReorderRequest` in animus-protocol
        // (v0.1.4); deferred to v0.4.8 alongside the other web-api
        // migrations.
        let updated = reorder_subjects(project_root, request.subject_ids)
            .map_err(|e| WebApiError::new("internal_error", format!("failed to reorder queue: {}", e), 1))?;

        if updated {
            self.publish_event("queue-reorder", serde_json::json!({ "message": "queue reordered" }));
        }

        Ok(serde_json::json!({ "reordered": updated }))
    }

    pub async fn queue_hold(&self, task_id: &str, body: serde_json::Value) -> Result<serde_json::Value, WebApiError> {
        let _request: QueueHoldRequest = parse_json_body(body).unwrap_or(QueueHoldRequest {});
        let project_root = &self.context.project_root;

        if let Some(()) = try_queue_hold_via_control(project_root, task_id).await {
            self.publish_event("queue-hold", serde_json::json!({ "task_id": task_id, "held": true }));
            return Ok(serde_json::json!({ "held": true, "task_id": task_id }));
        }
        let updated = hold_subject(project_root, task_id)
            .map_err(|e| WebApiError::new("internal_error", format!("failed to hold task: {}", e), 1))?;

        if updated {
            self.publish_event("queue-hold", serde_json::json!({ "task_id": task_id, "held": true }));
        }

        Ok(serde_json::json!({ "held": updated, "task_id": task_id }))
    }

    pub async fn queue_release(
        &self,
        task_id: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, WebApiError> {
        let request: QueueReleaseRequest = parse_json_body(body).unwrap_or(QueueReleaseRequest { reason: None });
        let project_root = &self.context.project_root;

        if let Some(()) = try_queue_release_via_control(project_root, task_id).await {
            let mut payload = serde_json::json!({ "task_id": task_id, "released": true });
            if let Some(reason) = request.reason.as_deref() {
                payload["reason"] = serde_json::Value::String(reason.to_string());
            }
            self.publish_event("queue-release", payload);

            let mut response = serde_json::json!({ "released": true, "task_id": task_id });
            if let Some(reason) = request.reason.as_deref() {
                response["reason"] = serde_json::Value::String(reason.to_string());
            }
            return Ok(response);
        }
        let updated = release_subject(project_root, task_id)
            .map_err(|e| WebApiError::new("internal_error", format!("failed to release task: {}", e), 1))?;

        if updated {
            let mut payload = serde_json::json!({ "task_id": task_id, "released": true });
            if let Some(reason) = request.reason.as_deref() {
                payload["reason"] = serde_json::Value::String(reason.to_string());
            }
            self.publish_event("queue-release", payload);
        }

        let mut response = serde_json::json!({ "released": updated, "task_id": task_id });
        if let Some(reason) = request.reason.as_deref() {
            response["reason"] = serde_json::Value::String(reason.to_string());
        }

        Ok(response)
    }
}

// ---- try-control helpers (C8: web-api routes via daemon control) ----
//
// Each helper opens the daemon's Unix socket, attempts the RPC, and
// returns Some(wire_response) on success. Returns None when the daemon
// is not running, the socket is missing/stale, the RPC method is
// unavailable, or the RPC errors — callers then fall back to the local
// code path.

async fn try_queue_list_via_control(project_root: &str) -> Option<animus_control_protocol::types::QueueListResponse> {
    use animus_control_protocol::types::QueueListRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.queue_list(WireRequest::default()).await {
        Ok(response) => Some(response),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/list wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "queue/list wire error; falling back to local");
            None
        }
    }
}

async fn try_queue_stats_via_control(project_root: &str) -> Option<animus_control_protocol::types::QueueStats> {
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.queue_stats().await {
        Ok(response) => Some(response),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/stats wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "queue/stats wire error; falling back to local");
            None
        }
    }
}

async fn try_queue_hold_via_control(project_root: &str, id: &str) -> Option<()> {
    use animus_control_protocol::types::QueueHoldRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.queue_hold(WireRequest { id: id.to_string(), reason: None }).await {
        Ok(_) => Some(()),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/hold wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "queue/hold wire error; falling back to local");
            None
        }
    }
}

async fn try_queue_release_via_control(project_root: &str, id: &str) -> Option<()> {
    use animus_control_protocol::types::QueueReleaseRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.queue_release(WireRequest { id: id.to_string() }).await {
        Ok(_) => Some(()),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/release wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "queue/release wire error; falling back to local");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use protocol::SubjectDispatch;

    use super::*;

    #[test]
    fn throughput_last_hour_counts_recent_assignments() {
        let now = Utc.with_ymd_and_hms(2026, 3, 8, 0, 30, 0).unwrap();
        let snapshot = QueueSnapshot {
            entries: vec![
                QueueEntrySnapshot {
                    subject_id: "TASK-1".into(),
                    task_id: Some("TASK-1".into()),
                    dispatch: None,
                    status: DispatchQueueEntryStatus::Assigned,
                    workflow_id: None,
                    assigned_at: Some(now.to_rfc3339()),
                    held_at: None,
                },
                QueueEntrySnapshot {
                    subject_id: "TASK-2".into(),
                    task_id: Some("TASK-2".into()),
                    dispatch: None,
                    status: DispatchQueueEntryStatus::Assigned,
                    workflow_id: None,
                    assigned_at: Some(Utc.with_ymd_and_hms(2026, 3, 7, 20, 30, 0).unwrap().to_rfc3339()),
                    held_at: None,
                },
            ],
            stats: orchestrator_daemon_runtime::QueueStats { total: 2, pending: 0, assigned: 2, held: 0 },
        };

        assert_eq!(throughput_last_hour(&snapshot, now), 1);
    }

    #[test]
    fn avg_wait_time_uses_dispatch_requested_at_for_pending_entries() {
        let now = Utc.with_ymd_and_hms(2026, 3, 8, 0, 30, 0).unwrap();
        let snapshot = QueueSnapshot {
            entries: vec![QueueEntrySnapshot {
                subject_id: "TASK-1".into(),
                task_id: Some("TASK-1".into()),
                dispatch: Some(SubjectDispatch::for_task_with_metadata(
                    "TASK-1",
                    "ops",
                    "queue-test",
                    Utc.with_ymd_and_hms(2026, 3, 8, 0, 20, 0).unwrap(),
                )),
                status: DispatchQueueEntryStatus::Pending,
                workflow_id: None,
                assigned_at: None,
                held_at: None,
            }],
            stats: orchestrator_daemon_runtime::QueueStats { total: 1, pending: 1, assigned: 0, held: 0 },
        };

        assert_eq!(avg_wait_time_secs(&snapshot, now), 600);
    }
}
