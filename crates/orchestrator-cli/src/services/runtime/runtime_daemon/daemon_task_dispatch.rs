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
    // `workflow_id`. We lease at most `limit` entries (matching the
    // upstream `ready_dispatch_limit`, which already nets out
    // pool_size and active_agents) and dispatch every one we lease.
    //
    // The v0.5 queue protocol intentionally has no Assigned → Pending
    // transition (`queue/release` covers Held → Pending only), so the
    // daemon must NOT lease entries it cannot dispatch — terminal
    // `queue/completion(cancelled)` would discard valid queued work.
    // We:
    //   1. Lease exactly `limit` entries. Headroom upstream guarantees
    //      we have spawn capacity for them.
    //   2. Plan the dispatch for every leased entry. The active-subject
    //      defensive filter is gone — if a subject is somehow already
    //      running despite headroom, `spawn_workflow_runner` will fail
    //      and we close that entry with queue/completion(failed). This
    //      is a v0.5 limitation documented in the issue thread (#240);
    //      a Pending re-injection transition is a v0.6 protocol item.
    //   3. After `execute_dispatch_plan_via_runner` returns, close
    //      spawn-failed entries via queue/completion(failed).
    //
    // Falls back to the in-tree `load_dispatch_queue_state` when no
    // plugin is installed. The in-tree path stays per Wave 3 "Out of
    // scope".
    let mut planned_starts: Vec<PlannedDispatchStart> = Vec::new();
    let mut plugin_owned_subject_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Track `entry_id` per planned start in dispatch order so we can
    // finalize claims by entry on spawn failure via
    // `queue/completion(failed)`. Keying by subject_key would lose
    // disambiguation when the queue holds multiple entries for the
    // same subject (e.g. queued under different workflow_refs).
    let mut leased_entry_ids: Vec<String> = Vec::new();
    let mut undecodable_entry_ids: Vec<String> = Vec::new();
    // Stranded-on-purpose entries: leased but cannot be dispatched
    // this tick (active subject). v0.5 protocol has no Assigned →
    // Pending transition; cancelling is the least-bad option to keep
    // the queue from accumulating ghost entries.
    let mut stranded_entry_ids: Vec<String> = Vec::new();
    let mut used_plugin_path = false;
    let project_root_path = std::path::Path::new(root);

    // Lease atomically. Pass `workflow_ids = None` so the plugin
    // synthesizes UUIDs; the daemon's `start_subject_workflow` produces
    // its own workflow id post-spawn, and only that id is the
    // authoritative one for the run.
    let lease_req = QueueLeaseRequest { max: limit, workflow_ids: None };
    match plugin_clients::call_queue_lease(project_root_path, &lease_req).await {
        Ok(Some(response)) => {
            used_plugin_path = true;
            for entry in response.leased {
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
                        warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue/lease returned undecodable subject_dispatch; closing entry as failed");
                        undecodable_entry_ids.push(entry.entry_id.clone());
                        continue;
                    }
                };
                let dispatch: protocol::SubjectDispatch = match serde_json::from_value(dispatch_value) {
                    Ok(d) => d,
                    Err(error) => {
                        warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue/lease subject_dispatch shape drift vs in-tree protocol::SubjectDispatch; closing entry as failed");
                        undecodable_entry_ids.push(entry.entry_id.clone());
                        continue;
                    }
                };
                if active_subject_ids.contains(&dispatch.subject_key())
                    || plugin_owned_subject_keys.contains(&dispatch.subject_key())
                {
                    // v0.5 known limitation: queue/lease atomically
                    // claims pending entries before the daemon can
                    // check active subjects or dedupe within the
                    // leased set. The protocol intentionally has no
                    // Assigned → Pending transition. Leaving the
                    // entry assigned would strand it forever when its
                    // workflow_ref differs from the in-flight run
                    // (completion's workflow_ref filter skips it).
                    // The least-bad option is queue/completion(
                    // cancelled) — the work is lost, but the entry is
                    // pruned. Operators see the warning and re-enqueue
                    // explicitly. Tracking Assigned → Pending as a
                    // v0.6 protocol enhancement (subject-aware lease
                    // filter or release).
                    warn!(
                        actor = protocol::ACTOR_DAEMON,
                        subject_key = %dispatch.subject_key(),
                        entry_id = %entry.entry_id,
                        "queue/lease returned entry for already-running or already-planned subject; cancelling (v0.5 known limitation — re-enqueue if needed)"
                    );
                    plugin_owned_subject_keys.insert(dispatch.subject_key());
                    stranded_entry_ids.push(entry.entry_id.clone());
                    continue;
                }

                let subject_key = dispatch.subject_key();
                plugin_owned_subject_keys.insert(subject_key);
                leased_entry_ids.push(entry.entry_id.clone());
                planned_starts
                    .push(PlannedDispatchStart { dispatch, selection_source: DispatchSelectionSource::DispatchQueue });
            }
        }
        Ok(None) => {
            // No queue plugin installed — fall through to in-tree state.
        }
        Err(error) => {
            // queue/lease is mutating: the plugin may have already
            // transitioned entries to Assigned before the host saw the
            // timeout / decode error / shutdown. Do NOT fall back to
            // the in-tree state — those claimed plugin entries would
            // be invisible to the local file and stranded forever.
            // Surface the failure and emit an empty dispatch summary
            // for this tick; the next tick will retry the plugin
            // path.
            warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue plugin queue/lease failed; deferring dispatch to next tick to avoid stranding claimed entries");
            used_plugin_path = true;
        }
    }

    // Close entries with undecodable dispatch envelopes as failed —
    // the dispatch payload is corrupt; there is nothing to re-run.
    for entry_id in &undecodable_entry_ids {
        let req = QueueCompletionRequest {
            entry_id: entry_id.clone(),
            status: queue_proto::completion_status::FAILED.to_string(),
            workflow_ref: None,
            workflow_id: None,
        };
        if let Err(error) = plugin_clients::call_queue_completion(project_root_path, &req).await {
            warn!(
                actor = protocol::ACTOR_DAEMON,
                entry_id = %entry_id,
                error = %error,
                "queue plugin queue/completion (undecodable entry) failed"
            );
        }
    }

    // Cancel entries the lease claimed for already-active subjects.
    // See the matching warn! above — this is the documented v0.5
    // limitation. Cancel (rather than leaving Assigned) ensures the
    // queue doesn't accumulate stranded entries when workflow_refs
    // differ.
    for entry_id in &stranded_entry_ids {
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
                "queue plugin queue/completion (active-subject collision) failed"
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
    //
    // Matching is by leased_entry_ids[i] ↔ planned_starts[i] index
    // (we pushed them in lockstep), then by subject_key against the
    // post-spawn started_workflows summary. This handles the case of
    // multiple dispatchable entries per subject correctly — even if a
    // later entry succeeds where an earlier failed (currently we
    // dedupe by subject_key during planning, so at most one entry per
    // subject_key reaches planned_starts; the index pair stays
    // unambiguous).
    if !leased_entry_ids.is_empty() {
        let started_keys: std::collections::HashSet<String> =
            summary.started_workflows.iter().map(|s| s.dispatch.subject_key()).collect();
        for (idx, planned) in planned_starts.iter().enumerate() {
            let Some(entry_id) = leased_entry_ids.get(idx) else {
                continue;
            };
            let subject_key = planned.dispatch.subject_key();
            if started_keys.contains(&subject_key) {
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
