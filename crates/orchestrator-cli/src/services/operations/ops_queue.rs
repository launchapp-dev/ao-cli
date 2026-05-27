mod control_routing;

pub(crate) use control_routing::build_queue_routing;

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use orchestrator_core::{load_workflow_config_or_default, services::ServiceHub, workflow_ref_for_task};
use orchestrator_daemon_runtime::{
    drop_subject, enqueue_subject_dispatch, hold_subject, queue_snapshot, queue_stats, release_subject,
    reorder_subjects,
};
use protocol::SubjectDispatch;

use super::ops_workflow::resolve_requirement_workflow_ref;
use crate::{print_ok, print_value, QueueCommand};

#[allow(clippy::too_many_arguments)]
async fn resolve_enqueue_dispatch(
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    task_id: Option<String>,
    requirement_id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    workflow_ref: Option<String>,
    input: Option<serde_json::Value>,
) -> Result<SubjectDispatch> {
    match (task_id, requirement_id, title) {
        (Some(task_id), None, None) => {
            let task = hub.tasks().get(&task_id).await?;
            let workflow_ref = workflow_ref.unwrap_or_else(|| workflow_ref_for_task(&task));
            Ok(SubjectDispatch::for_task_with_metadata(
                task.id.clone(),
                workflow_ref,
                "manual-queue-enqueue",
                chrono::Utc::now(),
            )
            .with_input(input))
        }
        (None, Some(requirement_id), None) => {
            hub.planning().get_requirement(&requirement_id).await?;
            Ok(SubjectDispatch::for_requirement(
                requirement_id,
                workflow_ref.unwrap_or(resolve_requirement_workflow_ref(project_root)?),
                "manual-queue-enqueue",
            )
            .with_input(input))
        }
        (None, None, Some(title)) => Ok(SubjectDispatch::for_custom(
            title,
            description.unwrap_or_default(),
            workflow_ref.unwrap_or_else(|| {
                load_workflow_config_or_default(std::path::Path::new(project_root)).config.default_workflow_ref
            }),
            input,
            "manual-queue-enqueue",
        )),
        (None, None, None) => Err(anyhow!(
            "no subject specified. Use --task-id TASK_ID for existing tasks, --requirement-id REQ_ID for requirements, or --title \"name\" for custom dispatches."
        )),
        _ => Err(anyhow!(
            "--task-id, --requirement-id, and --title are mutually exclusive — provide only one subject selector."
        )),
    }
}

pub(crate) async fn handle_queue(
    command: QueueCommand,
    hub: Arc<dyn ServiceHub>,
    project_root: &str,
    json: bool,
) -> Result<()> {
    match command {
        QueueCommand::List => {
            // C6.6: prefer the control wire when daemon is running + json
            // mode so the daemon's view of queue entries is authoritative.
            // Falls back to the local in-process snapshot when no socket
            // is available or the daemon returns NotSupported.
            if json {
                if let Some(response) = try_queue_list_via_control(project_root).await? {
                    return print_value(response, true);
                }
            }
            print_value(queue_snapshot(project_root)?, json)
        }
        QueueCommand::Stats => {
            if json {
                if let Some(response) = try_queue_stats_via_control(project_root).await? {
                    return print_value(response, true);
                }
            }
            print_value(queue_stats(project_root)?, json)
        }
        QueueCommand::Enqueue(args) => {
            // C6.6: when JSON mode + task id + daemon is running, route
            // through the wire. Requirement / title / custom dispatches
            // and any --input-json payload stay local — the wire
            // `queue/enqueue` surface only covers task subjects today.
            //
            // `--workflow-ref` also stays local: the wire `queue/enqueue`
            // request shape only carries `task_id` + `priority`, with no
            // slot for a workflow override. Routing through the wire
            // when the user passed `--workflow-ref` would silently swap
            // their requested workflow for the project default — the
            // exact P2 fix tracked here. Until the wire grows a
            // workflow_ref field we degrade to the local path that
            // honors the flag faithfully.
            if json
                && args.input_json.is_none()
                && args.requirement_id.is_none()
                && args.title.is_none()
                && args.workflow_ref.is_none()
            {
                if let Some(task_id) = args.task_id.as_ref() {
                    if let Some(entry) = try_queue_enqueue_via_control(project_root, task_id).await? {
                        return print_value(entry, true);
                    }
                }
            }
            let input = args.input_json.map(|value| serde_json::from_str(&value)).transpose()?;
            let dispatch = resolve_enqueue_dispatch(
                hub.clone(),
                project_root,
                args.task_id,
                args.requirement_id,
                args.title,
                args.description,
                args.workflow_ref,
                input,
            )
            .await?;
            let result = enqueue_subject_dispatch(project_root, dispatch)?;
            if !json {
                if result.enqueued {
                    print_ok("subject dispatch enqueued", false);
                    return Ok(());
                }
                print_ok("subject dispatch already queued", false);
                return Ok(());
            }
            print_value(result, true)
        }
        QueueCommand::Hold(args) => {
            if json {
                if let Some(()) = try_queue_hold_via_control(project_root, &args.subject_id).await? {
                    return print_value(serde_json::json!({ "held": true, "subject_id": args.subject_id }), true);
                }
            }
            let held = hold_subject(project_root, &args.subject_id)?;
            if !json {
                if held {
                    print_ok("queue subject held", false);
                    return Ok(());
                }
                return Err(anyhow!("queue subject not found or not pending"));
            }
            print_value(serde_json::json!({ "held": held, "subject_id": args.subject_id }), true)
        }
        QueueCommand::Release(args) => {
            if json {
                if let Some(()) = try_queue_release_via_control(project_root, &args.subject_id).await? {
                    return print_value(serde_json::json!({ "released": true, "subject_id": args.subject_id }), true);
                }
            }
            let released = release_subject(project_root, &args.subject_id)?;
            if !json {
                if released {
                    print_ok("queue subject released", false);
                    return Ok(());
                }
                return Err(anyhow!("queue subject not found or not held"));
            }
            print_value(serde_json::json!({ "released": released, "subject_id": args.subject_id }), true)
        }
        QueueCommand::Drop(args) => {
            if json {
                if let Some(()) = try_queue_drop_via_control(project_root, &args.subject_id).await? {
                    // Wire surface returns Unit; local removed count not
                    // exposed across the wire. Report 1 to indicate the
                    // drop succeeded.
                    return print_value(serde_json::json!({ "dropped": 1, "subject_id": args.subject_id }), true);
                }
            }
            let removed = drop_subject(project_root, &args.subject_id)?;
            if !json {
                if removed > 0 {
                    print_ok(&format!("dropped {removed} queue entry/entries for {}", args.subject_id), false);
                    return Ok(());
                }
                return Err(anyhow!("queue subject not found"));
            }
            print_value(serde_json::json!({ "dropped": removed, "subject_id": args.subject_id }), true)
        }
        QueueCommand::Reorder(args) => {
            // Wire `queue/reorder` is single-id per-call (id + position
            // anchor). The CLI's `--subject-id` repeated form is a
            // multi-id reorder. We could send N wire calls in sequence
            // but the in-tree implementation does the multi-id move
            // atomically. Keep the CLI verb local for now and let the
            // future wire-side multi-id reorder land as a v0.4.x cleanup.
            let reordered = reorder_subjects(project_root, args.subject_ids.clone())?;
            if !json {
                if reordered {
                    print_ok("queue reordered", false);
                    return Ok(());
                }
                print_ok("queue order unchanged", false);
                return Ok(());
            }
            print_value(serde_json::json!({ "reordered": reordered, "subject_ids": args.subject_ids }), true)
        }
    }
}

// =====================================================================
// C6.6 — control-wire routing helpers for queue/*
// =====================================================================
//
// Each helper opens the control socket (returns Ok(None) when the daemon
// isn't running so the caller falls back to the local in-process path),
// issues the corresponding JSON-RPC call, and returns the wire-shaped
// response. When the daemon advertises the surface but the specific
// method is unavailable (older daemon, mid-rollout) we treat that the
// same as "socket missing" and degrade to local.

async fn try_queue_list_via_control(
    project_root: &str,
) -> Result<Option<animus_control_protocol::types::QueueListResponse>> {
    use animus_control_protocol::types::QueueListRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let Some(client) = ControlClient::try_connect(project_root_path).await? else {
        return Ok(None);
    };
    match client.queue_list(WireRequest::default()).await {
        Ok(response) => Ok(Some(response)),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/list wire returned unavailable; falling back to local");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn try_queue_stats_via_control(project_root: &str) -> Result<Option<animus_control_protocol::types::QueueStats>> {
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let Some(client) = ControlClient::try_connect(project_root_path).await? else {
        return Ok(None);
    };
    match client.queue_stats().await {
        Ok(response) => Ok(Some(response)),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/stats wire returned unavailable; falling back to local");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn try_queue_enqueue_via_control(
    project_root: &str,
    task_id: &str,
) -> Result<Option<animus_control_protocol::types::QueueEntry>> {
    use animus_control_protocol::types::QueueEnqueueRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let Some(client) = ControlClient::try_connect(project_root_path).await? else {
        return Ok(None);
    };
    let request = WireRequest { task_id: task_id.to_string(), priority: None };
    match client.queue_enqueue(request).await {
        Ok(response) => Ok(Some(response)),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/enqueue wire returned unavailable; falling back to local");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn try_queue_drop_via_control(project_root: &str, id: &str) -> Result<Option<()>> {
    use animus_control_protocol::types::QueueDropRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let Some(client) = ControlClient::try_connect(project_root_path).await? else {
        return Ok(None);
    };
    match client.queue_drop(WireRequest { id: id.to_string() }).await {
        Ok(_) => Ok(Some(())),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/drop wire returned unavailable; falling back to local");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn try_queue_hold_via_control(project_root: &str, id: &str) -> Result<Option<()>> {
    use animus_control_protocol::types::QueueHoldRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let Some(client) = ControlClient::try_connect(project_root_path).await? else {
        return Ok(None);
    };
    match client.queue_hold(WireRequest { id: id.to_string(), reason: None }).await {
        Ok(_) => Ok(Some(())),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/hold wire returned unavailable; falling back to local");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn try_queue_release_via_control(project_root: &str, id: &str) -> Result<Option<()>> {
    use animus_control_protocol::types::QueueReleaseRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let Some(client) = ControlClient::try_connect(project_root_path).await? else {
        return Ok(None);
    };
    match client.queue_release(WireRequest { id: id.to_string() }).await {
        Ok(_) => Ok(Some(())),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "queue/release wire returned unavailable; falling back to local");
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use orchestrator_core::{
        builtin_agent_runtime_config, builtin_workflow_config, write_agent_runtime_config, write_workflow_config,
        InMemoryServiceHub,
    };

    use super::*;

    #[tokio::test]
    async fn resolve_enqueue_dispatch_missing_subject_shows_actionable_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workflow_config = builtin_workflow_config();
        write_workflow_config(temp.path(), &workflow_config).expect("write config");
        write_agent_runtime_config(temp.path(), &builtin_agent_runtime_config()).expect("write runtime config");

        let hub = Arc::new(InMemoryServiceHub::new());
        let err =
            resolve_enqueue_dispatch(hub, temp.path().to_string_lossy().as_ref(), None, None, None, None, None, None)
                .await
                .expect_err("missing subject should fail");

        let msg = err.to_string();
        assert!(msg.contains("--task-id"), "error should mention --task-id");
        assert!(msg.contains("--requirement-id"), "error should mention --requirement-id");
        assert!(msg.contains("--title"), "error should mention --title");
        assert!(msg.contains("custom dispatches"), "error should suggest custom dispatches");
    }

    #[tokio::test]
    async fn resolve_enqueue_dispatch_multiple_subjects_shows_mutual_exclusivity_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workflow_config = builtin_workflow_config();
        write_workflow_config(temp.path(), &workflow_config).expect("write config");
        write_agent_runtime_config(temp.path(), &builtin_agent_runtime_config()).expect("write runtime config");

        let hub = Arc::new(InMemoryServiceHub::new());
        let err = resolve_enqueue_dispatch(
            hub,
            temp.path().to_string_lossy().as_ref(),
            Some("TASK-1".to_string()),
            Some("REQ-1".to_string()),
            None,
            None,
            None,
            None,
        )
        .await
        .expect_err("multiple subjects should fail");

        let msg = err.to_string();
        assert!(msg.contains("mutually exclusive"), "error should mention mutual exclusivity");
    }
}
