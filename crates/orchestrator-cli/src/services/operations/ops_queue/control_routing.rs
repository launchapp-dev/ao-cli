//! CLI-side `QueueRouting` adapter — bridges the daemon's control
//! surface back to the same in-tree queue helpers
//! (`queue_snapshot` / `enqueue_subject_dispatch` / `hold_subject` /
//! `release_subject` / `drop_subject` / `reorder_subjects` /
//! `queue_stats`) the CLI uses on the local code path.
//!
//! See the sibling
//! [`crate::services::operations::ops_workflow::control_routing`] and
//! [`crate::services::operations::ops_plugin::control_routing`] modules
//! for the workflow/* and plugin/* equivalents.
//!
//! ## Shape conversion
//!
//! The wire types under [`animus_control_protocol::types`] are leaner
//! than the in-tree `QueueEntrySnapshot`: wire entries expose
//! `id` / `subject_id` / `status` / `priority` / `enqueued_at` /
//! `hold_reason`. The CLI's local snapshot still carries the rich
//! `SubjectDispatch` payload for local renders. The wire-side `id` is
//! the in-tree subject id today — there is no separate entry id
//! concept yet, and `queue/drop` / `hold` / `release` accept it
//! verbatim.
//!
//! ## Error mapping
//!
//! Anyhow errors carry no machine-readable code. We surface them as
//! [`ControlError::Internal`] with the original message preserved (with
//! `{:#}` so the error chain shows up).

use std::path::PathBuf;
use std::sync::Arc;

use animus_control_protocol::{
    types::{
        QueueDropRequest as WireDropRequest, QueueEnqueueRequest as WireEnqueueRequest, QueueEntry as WireQueueEntry,
        QueueEntryStatus as WireEntryStatus, QueueHoldRequest as WireHoldRequest, QueueListRequest as WireListRequest,
        QueueListResponse as WireListResponse, QueueReleaseRequest as WireReleaseRequest,
        QueueReorderRequest as WireReorderRequest, QueueStats as WireQueueStats, Unit,
    },
    ControlError,
};
use animus_subject_protocol_wire::SubjectId;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use orchestrator_daemon_runtime::{
    control::QueueRouting, drop_subject, enqueue_subject_dispatch, hold_subject, queue_snapshot, queue_stats,
    release_subject, DispatchQueueEntryStatus, QueueEntrySnapshot, QueueStats as CoreQueueStats,
};
use protocol::SubjectDispatch;

/// Build a [`QueueRouting`] handle bound to `project_root`.
///
/// `project_root` is captured once at daemon startup and reused for every
/// routed call. The returned handle is `Clone` + `Send + Sync` via the
/// `Arc<dyn>` wrapper.
pub fn build_queue_routing(project_root: PathBuf) -> Arc<dyn QueueRouting> {
    Arc::new(QueueRoutingImpl { project_root })
}

struct QueueRoutingImpl {
    project_root: PathBuf,
}

impl QueueRoutingImpl {
    fn project_root_str(&self) -> String {
        self.project_root.to_string_lossy().to_string()
    }
}

fn internal(err: anyhow::Error) -> ControlError {
    ControlError::Internal(format!("{err:#}"))
}

fn wire_status_filter_to_core(status: WireEntryStatus) -> Option<DispatchQueueEntryStatus> {
    // The wire enum is richer than the in-tree status enum; "ready"
    // maps to Pending, "in-flight" to Assigned, "held" to Held. "done"
    // / "dropped" have no in-tree representation (terminal entries
    // are removed by the daemon) — those filters return an empty page.
    match status {
        WireEntryStatus::Ready => Some(DispatchQueueEntryStatus::Pending),
        WireEntryStatus::Held => Some(DispatchQueueEntryStatus::Held),
        WireEntryStatus::InFlight => Some(DispatchQueueEntryStatus::Assigned),
        WireEntryStatus::Done | WireEntryStatus::Dropped => None,
    }
}

fn core_status_to_wire(status: DispatchQueueEntryStatus) -> WireEntryStatus {
    match status {
        DispatchQueueEntryStatus::Pending => WireEntryStatus::Ready,
        DispatchQueueEntryStatus::Held => WireEntryStatus::Held,
        DispatchQueueEntryStatus::Assigned => WireEntryStatus::InFlight,
        DispatchQueueEntryStatus::Unknown => WireEntryStatus::Ready,
    }
}

/// Decode an RFC3339 timestamp string into a UTC `DateTime`, falling
/// back to the unix epoch when the field is missing or malformed.
/// The wire schema requires a concrete `DateTime<Utc>` so we cannot
/// pass through `None`; epoch is the least-misleading sentinel.
fn parse_or_epoch(raw: Option<&str>) -> DateTime<Utc> {
    raw.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
}

fn snapshot_entry_to_wire(entry: &QueueEntrySnapshot) -> WireQueueEntry {
    // Prefer the dispatch's requested_at (set on enqueue), then any
    // assigned/held timestamp present, otherwise epoch.
    let enqueued_at = entry
        .dispatch
        .as_ref()
        .map(|d| d.requested_at)
        .unwrap_or_else(|| parse_or_epoch(entry.assigned_at.as_deref().or(entry.held_at.as_deref())));
    WireQueueEntry {
        // The wire-side `id` is the in-tree subject id today; this is
        // what queue/drop / hold / release accept. When the wire grows
        // a separate entry id this is the field we'd swap.
        id: entry.subject_id.clone(),
        subject_id: SubjectId::new(entry.subject_id.clone()),
        status: core_status_to_wire(entry.status),
        priority: 2, // medium default — in-tree entries don't carry a priority today
        enqueued_at,
        hold_reason: None,
    }
}

fn core_stats_to_wire(stats: CoreQueueStats) -> WireQueueStats {
    WireQueueStats {
        ready: stats.pending as u64,
        held: stats.held as u64,
        in_flight: stats.assigned as u64,
        // Done/dropped recents aren't tracked by the in-tree queue
        // store (terminal entries are removed on completion). Reporting
        // 0 is honest; future v0.4.x work can wire a recent-history
        // sidecar if operators want the throughput line.
        done_recent: 0,
        dropped_recent: 0,
    }
}

#[async_trait]
impl QueueRouting for QueueRoutingImpl {
    async fn queue_list(&self, request: WireListRequest) -> Result<WireListResponse, ControlError> {
        let project_root = self.project_root_str();
        let snapshot = queue_snapshot(&project_root).map_err(internal)?;
        let target_status = request.status.and_then(wire_status_filter_to_core);
        let entries: Vec<WireQueueEntry> = snapshot
            .entries
            .iter()
            .filter(|entry| match target_status {
                Some(status) => entry.status == status,
                None => request.status.is_none(),
            })
            .map(snapshot_entry_to_wire)
            .collect();
        // The in-tree queue store doesn't paginate today; the full set
        // fits in one response. Honor `limit` defensively so wire
        // callers can cap large queues.
        let entries = match request.limit {
            Some(n) => entries.into_iter().take(n as usize).collect(),
            None => entries,
        };
        Ok(WireListResponse { entries, next_cursor: None })
    }

    async fn queue_enqueue(&self, request: WireEnqueueRequest) -> Result<WireQueueEntry, ControlError> {
        let project_root = self.project_root_str();
        // The wire-side enqueue carries only `task_id` + optional
        // priority; the CLI's local path resolves the full
        // SubjectDispatch via the service hub. For the wire we build a
        // minimal task dispatch with the project's default workflow ref
        // — that matches what the CLI's `queue enqueue --task-id` does
        // without --workflow-ref.
        let workflow_ref = orchestrator_core::load_workflow_config_or_default(std::path::Path::new(&project_root))
            .config
            .default_workflow_ref;
        let dispatch = SubjectDispatch::for_task_with_metadata(
            request.task_id.clone(),
            workflow_ref,
            "control-queue-enqueue",
            Utc::now(),
        );
        let result = enqueue_subject_dispatch(&project_root, dispatch).map_err(internal)?;
        // Re-read the snapshot to surface the wire-shaped entry that
        // landed (or already existed); the in-tree result is just a
        // boolean + subject_id.
        let snapshot = queue_snapshot(&project_root).map_err(internal)?;
        let entry = snapshot
            .entries
            .iter()
            .find(|e| e.subject_id == result.subject_id)
            .map(snapshot_entry_to_wire)
            .ok_or_else(|| {
                ControlError::Internal(format!("enqueued entry {} not found in snapshot", result.subject_id))
            })?;
        Ok(entry)
    }

    async fn queue_drop(&self, request: WireDropRequest) -> Result<Unit, ControlError> {
        let project_root = self.project_root_str();
        let removed = drop_subject(&project_root, &request.id).map_err(internal)?;
        if removed == 0 {
            return Err(ControlError::NotFound(format!("queue entry '{}' not found", request.id)));
        }
        Ok(Unit::default())
    }

    async fn queue_hold(&self, request: WireHoldRequest) -> Result<Unit, ControlError> {
        let project_root = self.project_root_str();
        let held = hold_subject(&project_root, &request.id).map_err(internal)?;
        if !held {
            return Err(ControlError::NotFound(format!("queue entry '{}' not found or not pending", request.id)));
        }
        Ok(Unit::default())
    }

    async fn queue_release(&self, request: WireReleaseRequest) -> Result<Unit, ControlError> {
        let project_root = self.project_root_str();
        let released = release_subject(&project_root, &request.id).map_err(internal)?;
        if !released {
            return Err(ControlError::NotFound(format!("queue entry '{}' not found or not held", request.id)));
        }
        Ok(Unit::default())
    }

    async fn queue_reorder(&self, _request: WireReorderRequest) -> Result<Unit, ControlError> {
        // Wire `queue/reorder` is single-id + anchor + position. The
        // in-tree `reorder_subjects` takes a vector of subject ids and
        // does a whole-queue move atomically. The two shapes are
        // semantically different enough that the safe mapping is to
        // refuse the call for now and let CLI callers stay on the local
        // path. v0.4.x cleanup will land a wire-side multi-id reorder.
        Err(ControlError::NotSupported(
            "queue/reorder wire surface is single-id anchor-position; multi-id reorder stays local for now".to_string(),
        ))
    }

    async fn queue_stats(&self) -> Result<WireQueueStats, ControlError> {
        let project_root = self.project_root_str();
        let stats = queue_stats(&project_root).map_err(internal)?;
        Ok(core_stats_to_wire(stats))
    }
}
