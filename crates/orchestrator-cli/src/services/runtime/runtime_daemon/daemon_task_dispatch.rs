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
    // Codex R5 [P1]: track `subject_key → entry_id` so we can roll back
    // the plugin claim via `queue/release` when `spawn_workflow_runner`
    // fails downstream. Without this, a failed spawn would leave the
    // entry in `assigned` forever; the in-tree path (which only flipped
    // `assigned` AFTER a successful spawn) is unaffected.
    let mut plugin_entry_ids_by_subject_key: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut used_plugin_path = false;
    let project_root_path = std::path::Path::new(root);
    // Codex R2 P2: request more entries than `limit` so the local
    // "already-active" / decode-error filters leave headroom on the
    // tick. Without this, a pool of 5 with 5 active subjects atop the
    // queue would consume the entire fetch budget and starve any
    // dispatchable work further down. Multiplier of 4× is a pragmatic
    // ceiling for v0.5 (the in-tree path filtered the full queue, so
    // strictly speaking any cap risks starvation — the cap exists to
    // bound plugin response size on enormous queues; future versions
    // can switch to `queue/lease` for atomic claim-with-filter).
    let list_limit = limit.saturating_mul(4).max(limit);
    let list_req = QueueListRequest {
        status: vec![queue_proto::status::PENDING.to_string()],
        limit: Some(list_limit),
        offset: None,
    };
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

                // Codex R10 [P1]: do NOT claim the entry on the plugin
                // here. The previous in-tree path called
                // `mark_dispatch_queue_entry_assigned` only AFTER a
                // successful `spawn_workflow_runner`; matching that
                // semantic prevents spawn-failure rollbacks from
                // erroneously failing entries that never ran. After
                // `execute_dispatch_plan_via_runner` returns, we
                // transition only the subjects in
                // `summary.started_workflows` to `assigned`. Entries
                // whose spawn failed stay pending and will be
                // re-tried next tick.
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

    let mut notice_sink = CliDispatchNoticeSink { plugin_owned_subject_keys: plugin_owned_subject_keys.clone() };
    let summary = execute_dispatch_plan_via_runner(root, process_manager, &planned_starts, limit, &mut notice_sink);

    // Codex R10 [P1]: claim only the subjects that successfully
    // spawned. The previous in-tree path called
    // `mark_dispatch_queue_entry_assigned` post-spawn; matching that
    // semantic means non-started entries remain `pending` in the
    // plugin queue and are eligible for re-dispatch on the next tick.
    if !plugin_entry_ids_by_subject_key.is_empty() {
        for started in &summary.started_workflows {
            let subject_key = started.dispatch.subject_key();
            let Some(entry_id) = plugin_entry_ids_by_subject_key.get(&subject_key) else {
                continue;
            };
            let mark_req =
                QueueMarkAssignedRequest { entry_id: entry_id.clone(), workflow_id: started.workflow_id.clone() };
            if let Err(error) = plugin_clients::call_queue_mark_assigned(project_root_path, &mark_req).await {
                warn!(
                    actor = protocol::ACTOR_DAEMON,
                    subject_key = %subject_key,
                    entry_id = %entry_id,
                    error = %error,
                    "queue plugin queue/mark_assigned (post-spawn) failed"
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
