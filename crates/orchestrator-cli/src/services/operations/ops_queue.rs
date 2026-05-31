mod control_routing;

pub(crate) use control_routing::build_queue_routing;

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
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
            // Codex R6 [P1]: when a v0.5 queue plugin is installed it
            // owns the queue (write path already routes there). Routing
            // reads through the plugin too keeps the CLI consistent with
            // the daemon's dispatch loop and the `queue enqueue` path.
            let plugin_list_req = animus_queue_protocol::QueueListRequest::default();
            if let Some(plugin_response) =
                crate::services::plugin_clients::call_queue_list(std::path::Path::new(project_root), &plugin_list_req)
                    .await?
            {
                return print_value(plugin_response, json);
            }
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
            // Codex R6 [P1]: same plugin-first routing as List.
            if let Some(plugin_stats) =
                crate::services::plugin_clients::call_queue_stats(std::path::Path::new(project_root)).await?
            {
                return print_value(plugin_stats, json);
            }
            if json {
                if let Some(response) = try_queue_stats_via_control(project_root).await? {
                    return print_value(response, true);
                }
            }
            print_value(queue_stats(project_root)?, json)
        }
        QueueCommand::Enqueue(args) => {
            let input = args.input_json.clone().map(|value| serde_json::from_str(&value)).transpose()?;
            let dispatch = resolve_enqueue_dispatch(
                hub.clone(),
                project_root,
                args.task_id.clone(),
                args.requirement_id.clone(),
                args.title.clone(),
                args.description.clone(),
                args.workflow_ref.clone(),
                input,
            )
            .await?;

            // Codex R3/R4 [P1]: when a v0.5 queue plugin is installed it
            // owns the queue. The dispatch loop in
            // `daemon_task_dispatch::dispatch_queued_entries_via_runner`
            // already reads from the plugin (`queue/list`); routing
            // `enqueue` through the plugin keeps reads and writes on the
            // same backend so CLI-enqueued work is visible to the daemon.
            // This branch runs BEFORE the control-wire shortcut so that
            // the wire path (which writes to the in-tree store) can't
            // produce split-brain writes when a plugin is installed.
            //
            // The in-tree `enqueue_subject_dispatch` returns a typed
            // `QueueEnqueueResult { enqueued, .. }`. Translate the
            // plugin's `QueueEnqueueResponse` to the same JSON shape so
            // CLI consumers see no surface change.
            let dispatch_value =
                serde_json::to_value(&dispatch).context("encoding subject_dispatch for queue plugin")?;
            let plugin_dispatch = serde_json::from_value(dispatch_value)
                .context("subject_dispatch shape drift vs animus_subject_protocol v0.5")?;
            let plugin_request = animus_queue_protocol::QueueEnqueueRequest { subject_dispatch: plugin_dispatch };
            if let Some(plugin_response) =
                crate::services::plugin_clients::call_queue_enqueue(std::path::Path::new(project_root), &plugin_request)
                    .await?
            {
                let translated = serde_json::json!({
                    "enqueued": plugin_response.enqueued,
                    "entry_id": plugin_response.entry_id,
                    "subject_id": plugin_response.subject_id,
                    "via": "plugin_host",
                });
                if !json {
                    if plugin_response.enqueued {
                        print_ok("subject dispatch enqueued (via queue plugin)", false);
                        return Ok(());
                    }
                    print_ok("subject dispatch already queued (via queue plugin)", false);
                    return Ok(());
                }
                return print_value(translated, true);
            }

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
        // Wave 3 follow-up (issue #239): when a queue plugin is installed
        // it owns the queue. The CLI surface accepts subject_id; the plugin
        // protocol takes entry_id. Resolve subject_id → entry_id(s) via
        // `queue/list` and route hold/release/drop/reorder through the
        // plugin. Falls back to the in-tree path + control wire shortcut
        // when no plugin is installed.
        QueueCommand::Hold(args) => {
            if let Some(result) = try_queue_hold_via_plugin(project_root, &args.subject_id).await? {
                let held = result.changed;
                if !json {
                    if held {
                        print_ok("queue subject held (via queue plugin)", false);
                        return Ok(());
                    }
                    return Err(anyhow!("queue subject not found or not pending"));
                }
                return print_value(
                    serde_json::json!({ "held": held, "subject_id": args.subject_id, "via": "plugin_host" }),
                    true,
                );
            }
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
            if let Some(result) = try_queue_release_via_plugin(project_root, &args.subject_id).await? {
                let released = result.changed;
                if !json {
                    if released {
                        print_ok("queue subject released (via queue plugin)", false);
                        return Ok(());
                    }
                    return Err(anyhow!("queue subject not found or not held"));
                }
                return print_value(
                    serde_json::json!({ "released": released, "subject_id": args.subject_id, "via": "plugin_host" }),
                    true,
                );
            }
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
            if let Some(removed) = try_queue_drop_via_plugin(project_root, &args.subject_id).await? {
                if !json {
                    if removed > 0 {
                        print_ok(
                            &format!(
                                "dropped {removed} queue entry/entries for {} (via queue plugin)",
                                args.subject_id
                            ),
                            false,
                        );
                        return Ok(());
                    }
                    return Err(anyhow!("queue subject not found"));
                }
                return print_value(
                    serde_json::json!({ "dropped": removed, "subject_id": args.subject_id, "via": "plugin_host" }),
                    true,
                );
            }
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
            if let Some(reordered_count) = try_queue_reorder_via_plugin(project_root, &args.subject_ids).await? {
                let reordered = reordered_count > 0;
                if !json {
                    if reordered {
                        print_ok("queue reordered (via queue plugin)", false);
                        return Ok(());
                    }
                    print_ok("queue order unchanged", false);
                    return Ok(());
                }
                return print_value(
                    serde_json::json!({
                        "reordered": reordered,
                        "reordered_count": reordered_count,
                        "subject_ids": args.subject_ids,
                        "via": "plugin_host",
                    }),
                    true,
                );
            }
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

// =====================================================================
// Wave 3 follow-up (issue #239) — plugin routing for hold/release/drop/reorder
// =====================================================================
//
// Each helper resolves the CLI's subject_id (or list of subject_ids) into
// the plugin's entry_id surface via `queue/list`, then invokes the
// corresponding `queue/*` mutation on the plugin. Returns `Ok(None)` when
// no queue plugin is installed so the caller falls back to the control
// wire / in-tree path.
//
// The CLI mutates by subject_id today; the plugin protocol mutates by
// entry_id. We list pending/held/assigned entries for the subject and
// fold the per-entry mutation results into a single CLI-shaped response.

/// List entries from the plugin filtered to the given subject_id, scoped
/// to the supplied statuses. Returns an empty Vec when the plugin is not
/// installed. Returns `Ok(None)` when no plugin is installed.
async fn lookup_plugin_entries_by_subject(
    project_root: &str,
    subject_id: &str,
    statuses: &[&'static str],
) -> Result<Option<Vec<animus_queue_protocol::QueueEntry>>> {
    let req = animus_queue_protocol::QueueListRequest {
        status: statuses.iter().map(|s| (*s).to_string()).collect(),
        limit: None,
        offset: None,
    };
    let Some(response) =
        crate::services::plugin_clients::call_queue_list(std::path::Path::new(project_root), &req).await?
    else {
        return Ok(None);
    };
    let matched = response.entries.into_iter().filter(|entry| entry.subject_id == subject_id).collect();
    Ok(Some(matched))
}

async fn try_queue_hold_via_plugin(
    project_root: &str,
    subject_id: &str,
) -> Result<Option<animus_queue_protocol::QueueMutationResponse>> {
    let Some(entries) =
        lookup_plugin_entries_by_subject(project_root, subject_id, &[animus_queue_protocol::status::PENDING]).await?
    else {
        return Ok(None);
    };
    if entries.is_empty() {
        return Ok(Some(animus_queue_protocol::QueueMutationResponse { changed: false, not_found: true }));
    }
    let mut changed = false;
    let mut not_found = true;
    for entry in entries {
        let req = animus_queue_protocol::QueueHoldRequest { entry_id: entry.entry_id, reason: None };
        if let Some(resp) =
            crate::services::plugin_clients::call_queue_hold(std::path::Path::new(project_root), &req).await?
        {
            changed |= resp.changed;
            not_found &= resp.not_found;
        }
    }
    Ok(Some(animus_queue_protocol::QueueMutationResponse { changed, not_found }))
}

async fn try_queue_release_via_plugin(
    project_root: &str,
    subject_id: &str,
) -> Result<Option<animus_queue_protocol::QueueMutationResponse>> {
    let Some(entries) =
        lookup_plugin_entries_by_subject(project_root, subject_id, &[animus_queue_protocol::status::HELD]).await?
    else {
        return Ok(None);
    };
    if entries.is_empty() {
        return Ok(Some(animus_queue_protocol::QueueMutationResponse { changed: false, not_found: true }));
    }
    let mut changed = false;
    let mut not_found = true;
    for entry in entries {
        let req = animus_queue_protocol::QueueReleaseRequest { entry_id: entry.entry_id };
        if let Some(resp) =
            crate::services::plugin_clients::call_queue_release(std::path::Path::new(project_root), &req).await?
        {
            changed |= resp.changed;
            not_found &= resp.not_found;
        }
    }
    Ok(Some(animus_queue_protocol::QueueMutationResponse { changed, not_found }))
}

async fn try_queue_drop_via_plugin(project_root: &str, subject_id: &str) -> Result<Option<usize>> {
    // Drop matches the in-tree semantics: remove ALL entries for the
    // subject regardless of status. The in-tree `drop_subject` returns
    // the number of entries removed.
    let Some(entries) = lookup_plugin_entries_by_subject(
        project_root,
        subject_id,
        &[
            animus_queue_protocol::status::PENDING,
            animus_queue_protocol::status::HELD,
            animus_queue_protocol::status::ASSIGNED,
        ],
    )
    .await?
    else {
        return Ok(None);
    };
    let mut dropped = 0usize;
    for entry in entries {
        let req = animus_queue_protocol::QueueDropRequest { entry_id: entry.entry_id };
        if let Some(resp) =
            crate::services::plugin_clients::call_queue_drop(std::path::Path::new(project_root), &req).await?
        {
            if resp.changed {
                dropped += 1;
            }
        }
    }
    Ok(Some(dropped))
}

async fn try_queue_reorder_via_plugin(project_root: &str, subject_ids: &[String]) -> Result<Option<usize>> {
    // Resolve each subject_id to its current entry_id via `queue/list`
    // (pending+held — assigned entries can't be reordered). If any
    // subject is missing on the plugin side, return Some(0) so the
    // caller reports "queue order unchanged".
    let Some(entries) = lookup_plugin_entries_by_subject_set(
        project_root,
        subject_ids,
        &[animus_queue_protocol::status::PENDING, animus_queue_protocol::status::HELD],
    )
    .await?
    else {
        return Ok(None);
    };
    if entries.is_empty() {
        return Ok(Some(0));
    }
    let req = animus_queue_protocol::QueueReorderRequest { entry_ids: entries };
    let Some(resp) =
        crate::services::plugin_clients::call_queue_reorder(std::path::Path::new(project_root), &req).await?
    else {
        return Ok(None);
    };
    Ok(Some(resp.reordered_count))
}

/// Resolve a list of subject_ids to entry_ids via a single `queue/list`
/// call. Preserves the input subject order; for subjects with multiple
/// queued entries, includes every entry in the queue's existing order
/// (matches the in-tree `reorder_subjects` semantics, which moves all
/// of a subject's queued entries together). Returns `Ok(None)` when no
/// plugin is installed.
async fn lookup_plugin_entries_by_subject_set(
    project_root: &str,
    subject_ids: &[String],
    statuses: &[&'static str],
) -> Result<Option<Vec<String>>> {
    let req = animus_queue_protocol::QueueListRequest {
        status: statuses.iter().map(|s| (*s).to_string()).collect(),
        limit: None,
        offset: None,
    };
    let Some(response) =
        crate::services::plugin_clients::call_queue_list(std::path::Path::new(project_root), &req).await?
    else {
        return Ok(None);
    };
    let mut by_subject: std::collections::HashMap<&str, Vec<String>> = std::collections::HashMap::new();
    for entry in &response.entries {
        by_subject.entry(entry.subject_id.as_str()).or_default().push(entry.entry_id.clone());
    }
    let mut entry_ids: Vec<String> = Vec::new();
    for subject_id in subject_ids {
        if let Some(ids) = by_subject.remove(subject_id.as_str()) {
            entry_ids.extend(ids);
        }
    }
    Ok(Some(entry_ids))
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
