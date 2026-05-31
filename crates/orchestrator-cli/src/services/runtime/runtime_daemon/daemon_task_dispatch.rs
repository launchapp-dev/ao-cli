use super::*;
use orchestrator_daemon_runtime::{
    execute_dispatch_plan_via_runner, load_dispatch_queue_state, DispatchNoticeSink, DispatchQueueEntryStatus,
    DispatchSelectionSource, PlannedDispatchStart,
};
pub use orchestrator_daemon_runtime::{DispatchNotice, DispatchWorkflowStartSummary};
use tracing::warn;

use crate::services::plugin_clients;
use animus_queue_protocol::{self as queue_proto, QueueListRequest, QueueMarkAssignedRequest};

pub async fn dispatch_queued_entries_via_runner(
    root: &str,
    process_manager: &mut ProcessManager,
    limit: usize,
) -> anyhow::Result<DispatchWorkflowStartSummary> {
    let active_subject_ids = process_manager.active_subject_ids();

    // Wave 3: attempt to source pending entries via the v0.5 `queue` plugin
    // (`queue/list` + `queue/mark_assigned` is the simplest correct hot-path
    // wiring; future iterations should switch to the atomic `queue/lease`
    // per the Brief F handoff state). Fall back to the in-tree
    // `load_dispatch_queue_state` when no plugin is installed. (The
    // in-tree path stays per Wave 3 "Out of scope".)
    //
    // The plugin owns the queue when installed. We:
    //   1. Read pending entries via `queue/list`.
    //   2. For each entry we choose to dispatch, immediately call
    //      `queue/mark_assigned` so the plugin transitions
    //      pending → assigned. This prevents subsequent ticks (or
    //      daemon restarts) from re-selecting the same entry, which
    //      would cause double-dispatch. (Codex P1 fix.)
    //   3. Suppress the legacy in-tree `mark_dispatch_queue_entry_assigned`
    //      call on the plugin path — the in-tree queue file is empty
    //      when the plugin owns the queue, and that call would log a
    //      spurious assignment-failed warning. We do this by replacing
    //      the selection_source with a marker the executor honours
    //      (see `DispatchSelectionSource::DispatchQueue` handling in
    //      `dispatch_execution.rs`); since v0.5 doesn't have a
    //      dedicated `PluginQueue` selection-source enum yet, we use
    //      a separate notice sink that swallows the in-tree
    //      assignment notice when the plugin handled the entry.
    //
    // TODO(codex-p2): switch to `queue/lease { max, workflow_ids }` per the
    // dispatch-headroom contract in the Wave 2A Brief F handoff. Lease is
    // atomic (read+claim in one RPC) so it eliminates the small window
    // between `queue/list` and `queue/mark_assigned` where a second
    // daemon (which v0.5 explicitly does NOT support) could double-claim
    // the same entry.
    let mut planned_starts: Vec<PlannedDispatchStart> = Vec::new();
    let mut plugin_owned_subject_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut used_plugin_path = false;
    let project_root_path = std::path::Path::new(root);
    let list_req =
        QueueListRequest { status: vec![queue_proto::status::PENDING.to_string()], limit: Some(limit), offset: None };
    match plugin_clients::call_queue_list(project_root_path, &list_req).await {
        Ok(Some(response)) => {
            used_plugin_path = true;
            for entry in response.entries {
                if planned_starts.len() >= limit {
                    break;
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
                        warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue/list returned undecodable subject_dispatch");
                        continue;
                    }
                };
                let dispatch: protocol::SubjectDispatch = match serde_json::from_value(dispatch_value) {
                    Ok(d) => d,
                    Err(error) => {
                        warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue/list subject_dispatch shape drift vs in-tree protocol::SubjectDispatch");
                        continue;
                    }
                };
                if active_subject_ids.contains(&dispatch.subject_key()) {
                    continue;
                }

                // Claim this entry on the plugin BEFORE pushing to
                // planned_starts so a tick interruption doesn't leave
                // the queue with both an assigned entry and a running
                // workflow that doesn't reference it.
                let mark_req = QueueMarkAssignedRequest { entry_id: entry.entry_id.clone(), workflow_id: None };
                match plugin_clients::call_queue_mark_assigned(project_root_path, &mark_req).await {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        // Should not happen — we just received the
                        // entry from a plugin call. Treat as transient
                        // and skip this entry; it'll re-appear on the
                        // next tick.
                        warn!(
                            actor = protocol::ACTOR_DAEMON,
                            entry_id = %entry.entry_id,
                            "queue plugin vanished between list and mark_assigned; skipping entry"
                        );
                        continue;
                    }
                    Err(error) => {
                        warn!(
                            actor = protocol::ACTOR_DAEMON,
                            entry_id = %entry.entry_id,
                            error = %error,
                            "queue plugin queue/mark_assigned failed; skipping entry to avoid double-dispatch"
                        );
                        continue;
                    }
                }
                plugin_owned_subject_keys.insert(dispatch.subject_key());
                planned_starts
                    .push(PlannedDispatchStart { dispatch, selection_source: DispatchSelectionSource::DispatchQueue });
            }
        }
        Ok(None) => {
            // No queue plugin installed — fall through to in-tree state.
        }
        Err(error) => {
            warn!(actor = protocol::ACTOR_DAEMON, error = %error, "queue plugin queue/list failed; falling back to in-tree state");
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

    let mut notice_sink = CliDispatchNoticeSink { plugin_owned_subject_keys };
    Ok(execute_dispatch_plan_via_runner(root, process_manager, &planned_starts, limit, &mut notice_sink))
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
