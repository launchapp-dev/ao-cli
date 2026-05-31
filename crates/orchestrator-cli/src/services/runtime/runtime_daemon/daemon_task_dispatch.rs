use super::*;
use orchestrator_daemon_runtime::{
    execute_dispatch_plan_via_runner, load_dispatch_queue_state, DispatchNoticeSink, DispatchQueueEntryStatus,
    DispatchSelectionSource, PlannedDispatchStart,
};
pub use orchestrator_daemon_runtime::{DispatchNotice, DispatchWorkflowStartSummary};
use tracing::warn;

use crate::services::plugin_clients;
use animus_queue_protocol::{self as queue_proto, QueueCompletionRequest, QueueLeaseRequest};

pub async fn dispatch_queued_entries_via_runner(
    root: &str,
    process_manager: &mut ProcessManager,
    limit: usize,
) -> anyhow::Result<DispatchWorkflowStartSummary> {
    let active_subject_ids = process_manager.active_subject_ids();

    // Wave 3 follow-up (issue #240): atomic dispatch via `queue/lease`.
    // When a v0.5 queue plugin is installed it owns the queue. The
    // dispatch path previously read pending entries via `queue/list`
    // and claimed them post-spawn with `queue/mark_assigned`, leaving
    // a small window between read and claim that another daemon (which
    // v0.5 explicitly does not support) could exploit to double-claim.
    //
    // `queue/lease { max, workflow_ids: None }` reads + transitions
    // pending → assigned atomically and returns the full
    // `QueueEntry` with `SubjectDispatch` and the plugin-synthesized
    // `workflow_id`. We:
    //   1. Lease up to `limit*4` entries to keep tick headroom even
    //      when the front of the queue is filled with subjects that
    //      are already running locally. The upstream
    //      `ready_dispatch_limit` already accounts for pool capacity,
    //      so this overshoot only matters for stale-view edge cases.
    //   2. Filter out any subjects already active in the process
    //      manager. The protocol's `queue/release` only handles
    //      Held → Pending (not Assigned), so we instead mark deferred
    //      entries as `cancelled` via `queue/completion`. The next
    //      enqueue for the subject will create a fresh pending entry.
    //   3. Plan the dispatch with the remaining leased entries
    //      (clamped to `limit`).
    //   4. After `execute_dispatch_plan_via_runner` returns,
    //      `queue/completion(status=failed)` entries whose spawn
    //      failed so they don't strand on the queue. Spawn-failure
    //      is rare and the subject will need to be re-enqueued
    //      explicitly anyway.
    //
    // Falls back to the in-tree `load_dispatch_queue_state` when no
    // plugin is installed. The in-tree path stays per Wave 3 "Out of
    // scope".
    let mut planned_starts: Vec<PlannedDispatchStart> = Vec::new();
    let mut plugin_owned_subject_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Track `subject_key → entry_id` so we can finalize claims on
    // spawn failure via `queue/completion`. Without this, a failed
    // spawn would leave the entry in `assigned` forever.
    let mut plugin_entry_ids_by_subject_key: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // Entries the lease claimed but the daemon can't dispatch this
    // tick (subject already active or shape drift). The lease has
    // already transitioned them to Assigned; we drain via
    // queue/completion(cancelled) so the plugin reclaims the slot.
    let mut leased_but_deferred: Vec<String> = Vec::new();
    let mut used_plugin_path = false;
    let project_root_path = std::path::Path::new(root);

    // Lease atomically. Pass `workflow_ids = None` so the plugin
    // synthesizes UUIDs; the daemon's `start_subject_workflow` produces
    // its own workflow id post-spawn, and only that id is the
    // authoritative one for the run.
    let lease_max = limit.saturating_mul(4).max(limit);
    let lease_req = QueueLeaseRequest { max: lease_max, workflow_ids: None };
    match plugin_clients::call_queue_lease(project_root_path, &lease_req).await {
        Ok(Some(response)) => {
            used_plugin_path = true;
            for entry in response.leased {
                if planned_starts.len() >= limit {
                    // Any entries beyond `limit` were leased atomically
                    // but exceed the tick's spawn capacity. Defer them
                    // so the queue doesn't strand them assigned.
                    leased_but_deferred.push(entry.entry_id.clone());
                    continue;
                }
                // Wire-equivalent: the plugin returns the v0.5
                // `animus_subject_protocol::SubjectDispatch` whose JSON
                // shape matches the in-tree `protocol::SubjectDispatch`
                // byte-for-byte (preserved by Wave 1 re-homing). Re-encode
                // the value through the in-tree type to remain compatible
                // with the rest of the dispatch loop without forcing a
                // full subject-protocol migration in v0.5.
                let dispatch_value = match serde_json::to_value(&entry.subject_dispatch) {
                    Ok(v) => v,
                    Err(error) => {
                        warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue/lease returned undecodable subject_dispatch");
                        leased_but_deferred.push(entry.entry_id.clone());
                        continue;
                    }
                };
                let dispatch: protocol::SubjectDispatch = match serde_json::from_value(dispatch_value) {
                    Ok(d) => d,
                    Err(error) => {
                        warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue/lease subject_dispatch shape drift vs in-tree protocol::SubjectDispatch");
                        leased_but_deferred.push(entry.entry_id.clone());
                        continue;
                    }
                };
                if active_subject_ids.contains(&dispatch.subject_key()) {
                    // Subject is already running. Plugin transitioned
                    // this entry to Assigned; queue/release cannot move
                    // Assigned → Pending (protocol), so we drain via
                    // queue/completion(cancelled). The caller is
                    // responsible for re-enqueueing if the subject
                    // should still get this workflow after its current
                    // run completes.
                    leased_but_deferred.push(entry.entry_id.clone());
                    continue;
                }

                let subject_key = dispatch.subject_key();
                plugin_owned_subject_keys.insert(subject_key.clone());
                plugin_entry_ids_by_subject_key.insert(subject_key, entry.entry_id.clone());
                planned_starts
                    .push(PlannedDispatchStart { dispatch, selection_source: DispatchSelectionSource::DispatchQueue });
            }
        }
        Ok(None) => {
            // No queue plugin installed — fall through to in-tree state.
        }
        Err(error) => {
            warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue plugin queue/lease failed; falling back to in-tree state");
        }
    }

    // Drain entries leased but not dispatched (decode failure,
    // already-active subject, or beyond tick capacity). The protocol's
    // queue/release covers Held → Pending only; queue/completion is
    // the documented exit for Assigned entries. Use status=cancelled
    // so the entry is pruned without flagging spurious failure.
    for entry_id in &leased_but_deferred {
        let req = QueueCompletionRequest {
            entry_id: entry_id.clone(),
            status: queue_proto::completion_status::CANCELLED.to_string(),
            workflow_ref: None,
            workflow_id: None,
        };
        if let Err(error) = plugin_clients::call_queue_completion(project_root_path, &req).await {
            warn!(
                actor = protocol::ACTOR_DAEMON,
                entry_id = %entry_id,
                error = %error,
                "queue plugin queue/completion (deferred entry) failed"
            );
        }
    }

    if !used_plugin_path {
        let queue_state = match load_dispatch_queue_state(root) {
            Ok(state) => state,
            Err(error) => {
                warn!(
                    actor = protocol::ACTOR_DAEMON,
                    error = %error,
                    "failed to load dispatch queue state"
                );
                return Ok(DispatchWorkflowStartSummary::default());
            }
        };

        let Some(queue_state) = queue_state else {
            return Ok(DispatchWorkflowStartSummary::default());
        };

        for entry in &queue_state.entries {
            if planned_starts.len() >= limit {
                break;
            }
            if entry.status != DispatchQueueEntryStatus::Pending {
                continue;
            }
            let Some(dispatch) = &entry.dispatch else {
                continue;
            };
            if active_subject_ids.contains(&dispatch.subject_key()) {
                continue;
            }

            planned_starts.push(PlannedDispatchStart {
                dispatch: dispatch.clone(),
                selection_source: DispatchSelectionSource::DispatchQueue,
            });
        }
    }

    let mut notice_sink = CliDispatchNoticeSink { plugin_owned_subject_keys: plugin_owned_subject_keys.clone() };
    let summary = execute_dispatch_plan_via_runner(root, process_manager, &planned_starts, limit, &mut notice_sink);

    // Finalize entries whose spawn failed. Lease already transitioned
    // every dispatched entry to Assigned atomically; queue/release
    // can't undo Assigned → Pending (protocol), so we close the
    // failed-spawn entries with queue/completion(failed). The subject
    // can be re-enqueued by the caller (or via the usual scheduled
    // path) if it should still run.
    if !plugin_entry_ids_by_subject_key.is_empty() {
        let started_keys: std::collections::HashSet<String> =
            summary.started_workflows.iter().map(|s| s.dispatch.subject_key()).collect();
        for (subject_key, entry_id) in &plugin_entry_ids_by_subject_key {
            if started_keys.contains(subject_key) {
                continue;
            }
            let req = QueueCompletionRequest {
                entry_id: entry_id.clone(),
                status: queue_proto::completion_status::FAILED.to_string(),
                workflow_ref: None,
                workflow_id: None,
            };
            if let Err(error) = plugin_clients::call_queue_completion(project_root_path, &req).await {
                warn!(
                    actor = protocol::ACTOR_DAEMON,
                    subject_key = %subject_key,
                    entry_id = %entry_id,
                    error = %error,
                    "queue plugin queue/completion (spawn-failed entry) failed"
                );
            }
        }
    }
    Ok(summary)
}

struct CliDispatchNoticeSink {
    /// Subject keys whose queue ownership lives on the v0.5 queue plugin.
    /// `execute_dispatch_plan_via_runner` always tries to mark the
    /// dispatched entry assigned in the in-tree queue file; when the
    /// plugin owns the queue that call is expected to be a no-op (entry
    /// is not in the in-tree file) and should not surface as a warning.
    plugin_owned_subject_keys: std::collections::HashSet<String>,
}

impl DispatchNoticeSink for CliDispatchNoticeSink {
    fn notice(&mut self, notice: DispatchNotice) {
        match notice {
            DispatchNotice::QueueAssignmentFailed { dispatch, error } => {
                if self.plugin_owned_subject_keys.contains(&dispatch.subject_key()) {
                    // Already marked assigned on the queue plugin before
                    // we ever pushed this entry to planned_starts;
                    // in-tree mark_dispatch_queue_entry_assigned has
                    // nothing to do.
                    return;
                }
                warn!(
                    actor = protocol::ACTOR_DAEMON,
                    subject_id = %dispatch.subject_key(),
                    error = %error,
                    "failed to mark dispatch queue entry assigned"
                );
            }
            DispatchNotice::Failed { dispatch, error } => {
                warn!(
                    actor = protocol::ACTOR_DAEMON,
                    subject_id = %dispatch.subject_key(),
                    error = %error,
                    "failed to start workflow runner"
                );
            }
            _ => {}
        }
    }
}
