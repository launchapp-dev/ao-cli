use super::*;
use orchestrator_daemon_runtime::{
    execute_dispatch_plan_via_runner, load_dispatch_queue_state, DispatchNoticeSink, DispatchQueueEntryStatus,
    DispatchSelectionSource, PlannedDispatchStart,
};
pub use orchestrator_daemon_runtime::{DispatchNotice, DispatchWorkflowStartSummary};
use tracing::warn;

use crate::services::plugin_clients;
use animus_queue_protocol::{self as queue_proto, QueueListRequest};

pub async fn dispatch_queued_entries_via_runner(
    root: &str,
    process_manager: &mut ProcessManager,
    limit: usize,
) -> anyhow::Result<DispatchWorkflowStartSummary> {
    let active_subject_ids = process_manager.active_subject_ids();

    // Wave 3: attempt to source pending entries via the v0.5 `queue` plugin
    // (`queue/list` is the simplest hot-path call; future iterations should
    // switch to the atomic `queue/lease` per the Brief F handoff state).
    // Fall back to the in-tree `load_dispatch_queue_state` when no plugin is
    // installed. (The in-tree path stays per Wave 3 "Out of scope".)
    //
    // TODO(codex-p2): switch to `queue/lease { max, workflow_ids }` per the
    // dispatch-headroom contract in the Wave 2A Brief F handoff. The lease
    // path requires pre-minting workflow ids at the daemon and is wired in
    // v0.5.x once the in-tree assignment side-effects are reconciled.
    let mut planned_starts: Vec<PlannedDispatchStart> = Vec::new();
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

    let mut notice_sink = CliDispatchNoticeSink;
    Ok(execute_dispatch_plan_via_runner(root, process_manager, &planned_starts, limit, &mut notice_sink))
}

struct CliDispatchNoticeSink;

impl DispatchNoticeSink for CliDispatchNoticeSink {
    fn notice(&mut self, notice: DispatchNotice) {
        match notice {
            DispatchNotice::QueueAssignmentFailed { dispatch, error } => {
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
