use orchestrator_config::workflow_config::{
    load_workflow_config_or_default, write_workflow_config, WorkflowPhaseConfig, WorkflowPhaseEntry, WorkflowVariable,
};
use orchestrator_core::{
    dispatch_workflow_event, workflow_ref_for_task, FileServiceHub, ListPage, OrchestratorWorkflow, ServiceHub,
    WorkflowDefinition, WorkflowEvent, WorkflowQuery, REQUIREMENT_TASK_GENERATION_WORKFLOW_REF, STANDARD_WORKFLOW_REF,
};
use protocol::orchestrator::{WorkflowRunInput, SUBJECT_KIND_CUSTOM};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;

use super::{parsing::parse_json_body, requests::WorkflowRunRequest, WebApiError, WebApiService};

async fn resolve_workflow_run_dispatch(
    hub: &dyn ServiceHub,
    project_root: &str,
    request: WorkflowRunRequest,
) -> Result<protocol::SubjectDispatch, WebApiError> {
    let WorkflowRunRequest { task_id, requirement_id, title, description, workflow_ref, input } = request;
    match (task_id, requirement_id, title) {
        (Some(task_id), None, None) => {
            let task = hub.tasks().get(&task_id).await.map_err(WebApiError::from)?;
            Ok(protocol::SubjectDispatch::for_task_with_metadata(
                task.id.clone(),
                workflow_ref.unwrap_or_else(|| workflow_ref_for_task(&task)),
                "web-api-run",
                chrono::Utc::now(),
            )
            .with_input(input))
        }
        (None, Some(requirement_id), None) => {
            hub.planning().get_requirement(&requirement_id).await.map_err(WebApiError::from)?;
            let workflow_ref = match workflow_ref {
                Some(workflow_ref) => workflow_ref,
                None => resolve_requirement_workflow_ref(project_root)
                    .map_err(|message| WebApiError::new("invalid_input", message, 2))?,
            };
            Ok(protocol::SubjectDispatch::for_requirement(requirement_id, workflow_ref, "web-api-run")
                .with_input(input))
        }
        (None, None, Some(title)) => Ok(protocol::SubjectDispatch::for_custom(
            title,
            description.unwrap_or_default(),
            workflow_ref.unwrap_or_else(|| STANDARD_WORKFLOW_REF.to_string()),
            input,
            "web-api-run",
        )),
        (None, None, None) => Err(WebApiError::new(
            "invalid_input",
            "one of task_id, requirement_id, or title must be provided".to_string(),
            2,
        )),
        _ => Err(WebApiError::new(
            "invalid_input",
            "task_id, requirement_id, and title are mutually exclusive".to_string(),
            2,
        )),
    }
}

async fn resolve_workflow_run_dispatch_from_input(
    hub: &dyn ServiceHub,
    project_root: &str,
    input: WorkflowRunInput,
) -> Result<protocol::SubjectDispatch, WebApiError> {
    let WorkflowRunInput { subject, workflow_ref, input, .. } = input;
    if let Some(id) = subject.task_id() {
        let task = hub.tasks().get(id).await.map_err(WebApiError::from)?;
        Ok(protocol::SubjectDispatch::for_task_with_metadata(
            task.id.clone(),
            workflow_ref.unwrap_or_else(|| workflow_ref_for_task(&task)),
            "web-api-run",
            chrono::Utc::now(),
        )
        .with_input(input))
    } else if let Some(id) = subject.requirement_id() {
        hub.planning().get_requirement(id).await.map_err(WebApiError::from)?;
        let workflow_ref = match workflow_ref {
            Some(workflow_ref) => workflow_ref,
            None => resolve_requirement_workflow_ref(project_root)
                .map_err(|message| WebApiError::new("invalid_input", message, 2))?,
        };
        Ok(protocol::SubjectDispatch::for_requirement(id.to_string(), workflow_ref, "web-api-run").with_input(input))
    } else if subject.kind().eq_ignore_ascii_case(SUBJECT_KIND_CUSTOM) {
        Ok(protocol::SubjectDispatch::for_custom(
            subject.title.unwrap_or_else(|| subject.id.clone()),
            subject.description.unwrap_or_default(),
            workflow_ref.unwrap_or_else(|| STANDARD_WORKFLOW_REF.to_string()),
            input,
            "web-api-run",
        ))
    } else {
        Err(WebApiError::new("invalid_input", format!("unsupported workflow subject kind '{}'", subject.kind()), 2))
    }
}

async fn resolve_workflow_run_dispatch_from_body(
    hub: &dyn ServiceHub,
    project_root: &str,
    body: Value,
) -> Result<protocol::SubjectDispatch, WebApiError> {
    if let Ok(dispatch) = serde_json::from_value::<protocol::SubjectDispatch>(body.clone()) {
        return Ok(dispatch);
    }
    if let Ok(input) = serde_json::from_value::<WorkflowRunInput>(body.clone()) {
        if workflow_run_input_has_explicit_subject(&input) {
            return resolve_workflow_run_dispatch_from_input(hub, project_root, input).await;
        }
    }
    let request: WorkflowRunRequest = parse_json_body(body)?;
    resolve_workflow_run_dispatch(hub, project_root, request).await
}

fn workflow_run_input_has_explicit_subject(input: &WorkflowRunInput) -> bool {
    input.subject.task_id().is_some_and(|id| !id.is_empty())
        || input.subject.requirement_id().is_some_and(|id| !id.is_empty())
        || !input.subject.id().is_empty()
}

fn resolve_requirement_workflow_ref(project_root: &str) -> Result<String, String> {
    let root = std::path::Path::new(project_root);
    orchestrator_core::ensure_workflow_config_compiled(root).map_err(|error| error.to_string())?;
    let workflow_config = orchestrator_core::load_workflow_config(root).map_err(|error| error.to_string())?;
    workflow_config
        .workflows
        .iter()
        .any(|workflow| workflow.id.eq_ignore_ascii_case(REQUIREMENT_TASK_GENERATION_WORKFLOW_REF))
        .then(|| REQUIREMENT_TASK_GENERATION_WORKFLOW_REF.to_string())
        .ok_or_else(|| {
            format!(
                "requirement workflow '{}' is not configured for requirement subjects",
                REQUIREMENT_TASK_GENERATION_WORKFLOW_REF
            )
        })
}

impl WebApiService {
    pub async fn workflows_list(&self, query: WorkflowQuery) -> Result<ListPage<OrchestratorWorkflow>, WebApiError> {
        // Kept as the full-shape entrypoint for GraphQL callers. The HTTP
        // handler at `/api/v1/workflows` no longer uses this — it routes
        // through [`Self::workflows_list_summary`] which serves the wire
        // [`WorkflowRunSummary`] shape (kebab-case status, no `phases` or
        // `machine_state`). GraphQL continues to expose the full
        // `OrchestratorWorkflow` because the existing GraphQL types and
        // web-UI codegen depend on it; the HTTP contract is the public
        // surface that gets the v0.4.10 lossy-wire migration.
        Ok(self.context.hub.workflows().query(query).await?)
    }

    /// v0.4.10: lossy-wire shape for the `/api/v1/workflows` HTTP handler.
    ///
    /// Returns a `ListPage<WorkflowRunSummary>` where each row mirrors the
    /// control-protocol [`animus_control_protocol::types::WorkflowRunSummary`]
    /// (id / definition / status / subject_id / started_at / finished_at).
    /// Local-only fields — `phases`, `machine_state`, `current_phase`,
    /// `checkpoint_metadata`, decision history — are dropped at this
    /// boundary so the public HTTP API does not leak daemon internals.
    ///
    /// **Breaking change vs v0.4.9**: the response items are smaller and
    /// the status enum is kebab-case (`in-progress`-style) rather than
    /// snake_case. `Escalated` collapses to `Paused` because the wire
    /// status enum lacks it. Callers that need the full shape should use
    /// the GraphQL `workflows` query, which still serves
    /// `OrchestratorWorkflow`.
    pub async fn workflows_list_summary(
        &self,
        query: WorkflowQuery,
    ) -> Result<ListPage<animus_control_protocol::types::WorkflowRunSummary>, WebApiError> {
        let page = self.context.hub.workflows().query(query).await?;
        let ListPage { items, returned, total, limit, offset, has_more, next_offset } = page;
        let projected: Vec<_> = items.into_iter().map(workflow_to_run_summary).collect();
        Ok(ListPage { items: projected, returned, total, limit, offset, has_more, next_offset })
    }

    pub async fn workflow_config(&self) -> Result<Value, WebApiError> {
        let project_root = std::path::Path::new(&self.context.project_root);
        let loaded = load_workflow_config_or_default(project_root);
        let config = &loaded.config;

        let mcp_servers: Vec<Value> = config
            .mcp_servers
            .iter()
            .map(|(name, def)| {
                let env: Vec<Value> = def.env.iter().map(|(k, v)| json!({ "key": k, "value": v })).collect();
                json!({
                    "name": name,
                    "command": def.command,
                    "args": def.args,
                    "transport": def.transport,
                    "tools": def.tools,
                    "env": env,
                })
            })
            .collect();

        let phase_catalog: Vec<Value> = config
            .phase_catalog
            .iter()
            .map(|(id, entry)| {
                json!({
                    "id": id,
                    "label": entry.label,
                    "description": entry.description,
                    "category": entry.category,
                    "tags": entry.tags,
                })
            })
            .collect();

        let tools: Vec<Value> = config
            .tools
            .iter()
            .map(|(name, def)| {
                json!({
                    "name": name,
                    "executable": def.executable,
                    "supportsMcp": def.supports_mcp,
                    "supportsWrite": def.supports_write,
                    "contextWindow": def.context_window,
                })
            })
            .collect();

        let agent_profiles: Vec<Value> = config
            .agent_profiles
            .iter()
            .map(|(name, profile)| {
                json!({
                    "name": name,
                    "description": profile.description,
                    "role": profile.role,
                    "mcpServers": profile.mcp_servers,
                    "skills": profile.skills,
                    "tool": profile.tool,
                    "model": profile.model,
                })
            })
            .collect();

        let schedules: Vec<Value> = config
            .schedules
            .iter()
            .map(|s| {
                json!({
                    "id": s.id,
                    "cron": s.cron,
                    "workflowRef": s.workflow_ref,
                    "command": s.command,
                    "enabled": s.enabled,
                })
            })
            .collect();

        Ok(json!({
            "mcpServers": mcp_servers,
            "phaseCatalog": phase_catalog,
            "tools": tools,
            "agentProfiles": agent_profiles,
            "schedules": schedules,
        }))
    }

    pub async fn workflow_definitions(&self) -> Result<Value, WebApiError> {
        let project_root = std::path::Path::new(&self.context.project_root);
        let loaded = orchestrator_config::workflow_config::load_workflow_config_or_default(project_root);
        let defs: Vec<Value> = loaded
            .config
            .workflows
            .iter()
            .map(|d| {
                json!({
                    "id": d.id,
                    "name": if d.name.is_empty() { &d.id } else { &d.name },
                    "description": d.description,
                    "phases": d.phase_ids(),
                })
            })
            .collect();
        Ok(json!(defs))
    }

    pub async fn project_workflows(&self, id: &str) -> Result<Value, WebApiError> {
        let project = self.context.hub.projects().get(id).await?;
        let hub = FileServiceHub::new(&project.path)?;
        let workflows = hub.workflows().list().await?;

        Ok(json!({
            "project": project,
            "workflows": workflows,
        }))
    }

    pub async fn workflows_get(&self, id: &str) -> Result<Value, WebApiError> {
        if let Some(wire) = try_workflow_get_via_control(&self.context.project_root, id).await {
            // wire shape differs from local; v0.4.x cleanup
            return Ok(serde_json::to_value(wire).unwrap_or(Value::Null));
        }
        Ok(json!(self.context.hub.workflows().get(id).await?))
    }

    pub async fn workflows_decisions(&self, id: &str) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.workflows().decisions(id).await?))
    }

    pub async fn workflows_checkpoints(&self, id: &str) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.workflows().list_checkpoints(id).await?))
    }

    pub async fn workflows_get_checkpoint(&self, id: &str, checkpoint: usize) -> Result<Value, WebApiError> {
        Ok(json!(self.context.hub.workflows().get_checkpoint(id, checkpoint).await?))
    }

    pub async fn workflows_run(&self, body: Value) -> Result<Value, WebApiError> {
        let dispatch =
            resolve_workflow_run_dispatch_from_body(self.context.hub.as_ref(), &self.context.project_root, body)
                .await?;
        // Route through control only when the dispatch is task-shaped;
        // the wire `workflow/run` requires a `task_id`. Requirement and
        // custom dispatches stay local. v0.4.x cleanup will broaden the
        // wire surface.
        if let Some(task_id) = dispatch.subject.task_id() {
            let definition = Some(dispatch.workflow_ref.clone());
            if let Some(wire) = try_workflow_run_via_control(&self.context.project_root, task_id, definition).await {
                self.publish_event(
                    "workflow-run",
                    json!({
                        "workflow_id": wire.workflow_id,
                        "subject_id": task_id,
                        "task_id": task_id,
                    }),
                );
                // wire shape differs from local; v0.4.x cleanup
                return Ok(serde_json::to_value(wire).unwrap_or(Value::Null));
            }
        }
        let workflow = self.context.hub.workflows().run(dispatch.to_workflow_run_input()).await?;
        let subject_id = workflow.subject.title.clone().unwrap_or_else(|| workflow.subject.id.clone());
        self.publish_event(
            "workflow-run",
            json!({
                "workflow_id": workflow.id,
                "subject_id": subject_id,
                "task_id": workflow.task_id,
            }),
        );
        Ok(json!(workflow))
    }

    pub async fn workflows_resume(&self, id: &str, feedback: Option<String>) -> Result<Value, WebApiError> {
        // animus-protocol v0.1.4 added an optional `feedback` field on
        // `WorkflowResumeRequest`, so the wire path now carries reviewer
        // comments. Fall back to the local dispatcher only if the daemon
        // is unreachable.
        if let Some(()) = try_workflow_resume_via_control(&self.context.project_root, id, feedback.clone()).await {
            self.publish_event("workflow-resume", json!({ "workflow_id": id }));
            return Ok(json!({ "workflow_id": id, "status": "resumed" }));
        }
        let outcome = dispatch_workflow_event(
            self.context.hub.clone(),
            &self.context.project_root,
            WorkflowEvent::Resume { workflow_id: id.to_string(), feedback },
        )
        .await?;
        let workflow =
            outcome.workflow.ok_or_else(|| WebApiError::new("not_found", "workflow not found".to_string(), 3))?;
        self.publish_event("workflow-resume", json!({ "workflow_id": workflow.id, "status": workflow.status }));
        Ok(json!(workflow))
    }

    pub async fn workflows_pause(&self, id: &str) -> Result<Value, WebApiError> {
        if let Some(()) = try_workflow_pause_via_control(&self.context.project_root, id).await {
            self.publish_event("workflow-pause", json!({ "workflow_id": id }));
            return Ok(json!({ "workflow_id": id, "status": "paused" }));
        }
        let outcome = dispatch_workflow_event(
            self.context.hub.clone(),
            &self.context.project_root,
            WorkflowEvent::Pause { workflow_id: id.to_string() },
        )
        .await?;
        let workflow =
            outcome.workflow.ok_or_else(|| WebApiError::new("not_found", "workflow not found".to_string(), 3))?;
        self.publish_event("workflow-pause", json!({ "workflow_id": workflow.id, "status": workflow.status }));
        Ok(json!(workflow))
    }

    pub async fn workflows_cancel(&self, id: &str) -> Result<Value, WebApiError> {
        if let Some(()) = try_workflow_cancel_via_control(&self.context.project_root, id).await {
            self.publish_event("workflow-cancel", json!({ "workflow_id": id }));
            return Ok(json!({ "workflow_id": id, "status": "cancelled" }));
        }
        let outcome = dispatch_workflow_event(
            self.context.hub.clone(),
            &self.context.project_root,
            WorkflowEvent::Cancel { workflow_id: id.to_string() },
        )
        .await?;
        let workflow =
            outcome.workflow.ok_or_else(|| WebApiError::new("not_found", "workflow not found".to_string(), 3))?;
        self.publish_event("workflow-cancel", json!({ "workflow_id": workflow.id, "status": workflow.status }));
        Ok(json!(workflow))
    }

    pub async fn workflows_phase_approve(
        &self,
        workflow_id: &str,
        phase_id: &str,
        note: Option<String>,
    ) -> Result<Value, WebApiError> {
        let outcome = dispatch_workflow_event(
            self.context.hub.clone(),
            &self.context.project_root,
            WorkflowEvent::ApproveManualPhase {
                workflow_id: workflow_id.to_string(),
                phase_id: phase_id.to_string(),
                note,
            },
        )
        .await?;
        let workflow =
            outcome.workflow.ok_or_else(|| WebApiError::new("not_found", "workflow not found".to_string(), 3))?;
        self.publish_event(
            "workflow-phase-approve",
            json!({ "workflow_id": workflow.id, "phase_id": phase_id, "status": workflow.status }),
        );
        Ok(json!(workflow))
    }

    pub async fn save_agent_profile(
        &self,
        name: String,
        model: Option<String>,
        tool: Option<String>,
        role: Option<String>,
    ) -> Result<(), WebApiError> {
        let project_root = std::path::Path::new(&self.context.project_root);
        let loaded = load_workflow_config_or_default(project_root);
        let mut config = loaded.config;
        let profile = config
            .agent_profiles
            .get_mut(&name)
            .ok_or_else(|| WebApiError::new("not_found", format!("agent profile '{name}' not found"), 3))?;
        if let Some(m) = model {
            profile.model = if m.is_empty() { None } else { Some(m) };
        }
        if let Some(t) = tool {
            profile.tool = if t.is_empty() { None } else { Some(t) };
        }
        if let Some(r) = role {
            profile.role = if r.is_empty() { None } else { Some(r) };
        }
        write_workflow_config(project_root, &config)
            .map_err(|e| WebApiError::new("internal", format!("failed to write workflow config: {e}"), 1))?;
        Ok(())
    }

    pub async fn save_workflow_config(&self, config_json: &str) -> Result<(), WebApiError> {
        let config: orchestrator_config::workflow_config::WorkflowConfig = serde_json::from_str(config_json)
            .map_err(|e| WebApiError::new("invalid_input", format!("invalid workflow config JSON: {e}"), 2))?;
        let project_root = std::path::Path::new(&self.context.project_root);
        write_workflow_config(project_root, &config)
            .map_err(|e| WebApiError::new("internal", format!("failed to write workflow config: {e}"), 1))?;
        Ok(())
    }

    pub async fn upsert_workflow_definition(
        &self,
        id: String,
        name: String,
        description: Option<String>,
        phases_json: String,
        variables_json: Option<String>,
    ) -> Result<bool, WebApiError> {
        let project_root = std::path::Path::new(&self.context.project_root);
        let loaded = load_workflow_config_or_default(project_root);
        let mut config = loaded.config;

        let phase_values: Vec<Value> = serde_json::from_str(&phases_json)
            .map_err(|e| WebApiError::new("invalid_input", format!("invalid phases JSON: {e}"), 2))?;

        let phases: Vec<WorkflowPhaseEntry> = phase_values
            .into_iter()
            .map(|v| {
                if let Some(s) = v.as_str() {
                    return Ok(WorkflowPhaseEntry::Simple(s.to_string()));
                }
                let obj = v.as_object().ok_or_else(|| {
                    WebApiError::new("invalid_input", "each phase must be a string or object".to_string(), 2)
                })?;
                let phase_id = obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        WebApiError::new("invalid_input", "phase object must have an 'id' field".to_string(), 2)
                    })?
                    .to_string();
                let has_extra_fields = obj.contains_key("on_verdict")
                    || obj.contains_key("max_rework_attempts")
                    || obj.contains_key("skip_if");
                if !has_extra_fields {
                    return Ok(WorkflowPhaseEntry::Simple(phase_id));
                }
                let max_rework_attempts = obj.get("max_rework_attempts").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
                let skip_if: Vec<String> =
                    obj.get("skip_if").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                let on_verdict: HashMap<String, orchestrator_config::workflow_config::PhaseTransitionConfig> =
                    obj.get("on_verdict").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
                Ok(WorkflowPhaseEntry::Rich(WorkflowPhaseConfig {
                    id: phase_id,
                    max_rework_attempts,
                    on_verdict,
                    skip_if,
                }))
            })
            .collect::<Result<Vec<_>, WebApiError>>()?;

        let variables: Vec<WorkflowVariable> = match variables_json {
            Some(vj) => serde_json::from_str(&vj)
                .map_err(|e| WebApiError::new("invalid_input", format!("invalid variables JSON: {e}"), 2))?,
            None => Vec::new(),
        };

        let definition = WorkflowDefinition {
            id: id.clone(),
            name,
            description: description.unwrap_or_default(),
            phases,
            post_success: None,
            variables,
        };

        if let Some(pos) = config.workflows.iter().position(|w| w.id == id) {
            config.workflows[pos] = definition;
        } else {
            config.workflows.push(definition);
        }

        write_workflow_config(project_root, &config)
            .map_err(|e| WebApiError::new("internal", format!("failed to write workflow config: {e}"), 1))?;

        Ok(true)
    }

    pub async fn delete_workflow_definition(&self, id: &str) -> Result<bool, WebApiError> {
        let project_root = std::path::Path::new(&self.context.project_root);
        let loaded = load_workflow_config_or_default(project_root);
        let mut config = loaded.config;

        let original_len = config.workflows.len();
        config.workflows.retain(|w| w.id != id);

        if config.workflows.len() == original_len {
            return Err(WebApiError::new("not_found", format!("workflow definition '{id}' not found"), 3));
        }

        write_workflow_config(project_root, &config)
            .map_err(|e| WebApiError::new("internal", format!("failed to write workflow config: {e}"), 1))?;

        Ok(true)
    }

    pub async fn workflows_phase_output(
        &self,
        workflow_id: &str,
        phase_id: Option<&str>,
        tail: Option<i32>,
    ) -> Result<Value, WebApiError> {
        if workflow_id.is_empty()
            || workflow_id.contains('/')
            || workflow_id.contains('\\')
            || workflow_id.contains("..")
        {
            return Err(WebApiError::new("invalid_input", "workflow id contains unsafe path segments".to_string(), 2));
        }
        if let Some(pid) = phase_id {
            if pid.contains('/') || pid.contains('\\') || pid.contains("..") {
                return Err(WebApiError::new("invalid_input", "phase id contains unsafe path segments".to_string(), 2));
            }
        }

        let project_root = std::path::Path::new(&self.context.project_root);
        let state_base = protocol::scoped_state_root(project_root).unwrap_or_else(|| project_root.join(".animus"));
        let output_dir = state_base.join("state").join("workflows").join(workflow_id).join("phase-outputs");

        let resolved_phase_id = match phase_id {
            Some(pid) => pid.to_string(),
            None => {
                let workflow = self.context.hub.workflows().get(workflow_id).await?;
                workflow.current_phase.unwrap_or_else(|| {
                    workflow.phases.last().map(|p| p.phase_id.clone()).unwrap_or_else(|| "unknown".to_string())
                })
            }
        };

        let file_path = output_dir.join(format!("{resolved_phase_id}.json"));
        if !file_path.exists() {
            return Ok(json!({
                "lines": Vec::<String>::new(),
                "phase_id": resolved_phase_id,
                "has_more": false,
            }));
        }

        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| WebApiError::new("internal", format!("failed to read phase output: {e}"), 1))?;

        let all_lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let tail_count = tail.unwrap_or(50).max(1) as usize;
        let has_more = all_lines.len() > tail_count;
        let lines: Vec<String> = if has_more { all_lines[all_lines.len() - tail_count..].to_vec() } else { all_lines };

        Ok(json!({
            "lines": lines,
            "phase_id": resolved_phase_id,
            "has_more": has_more,
        }))
    }
}

// ---- try-control helpers (C8: web-api routes via daemon control) ----
//
// Each helper opens the daemon's Unix socket, attempts the RPC, and
// returns Some(wire_response) on success. Returns None when the daemon
// is not running, the socket is missing/stale, the RPC method is
// unavailable, or the RPC errors — callers then fall back to the local
// code path.

async fn try_workflow_get_via_control(
    project_root: &str,
    id: &str,
) -> Option<animus_control_protocol::types::WorkflowRun> {
    use animus_control_protocol::types::WorkflowGetRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.workflow_get(WireRequest { id: id.to_string() }).await {
        Ok(response) => Some(response),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "workflow/get wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "workflow/get wire error; falling back to local");
            None
        }
    }
}

async fn try_workflow_run_via_control(
    project_root: &str,
    task_id: &str,
    definition: Option<String>,
) -> Option<animus_control_protocol::types::WorkflowRunStart> {
    use animus_control_protocol::types::WorkflowRunRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    let request = WireRequest { task_id: task_id.to_string(), definition, params: Default::default() };
    match client.workflow_run(request).await {
        Ok(response) => Some(response),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "workflow/run wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "workflow/run wire error; falling back to local");
            None
        }
    }
}

async fn try_workflow_pause_via_control(project_root: &str, id: &str) -> Option<()> {
    use animus_control_protocol::types::WorkflowPauseRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.workflow_pause(WireRequest { id: id.to_string() }).await {
        Ok(_) => Some(()),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "workflow/pause wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "workflow/pause wire error; falling back to local");
            None
        }
    }
}

async fn try_workflow_resume_via_control(project_root: &str, id: &str, feedback: Option<String>) -> Option<()> {
    use animus_control_protocol::types::WorkflowResumeRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.workflow_resume(WireRequest { id: id.to_string(), feedback }).await {
        Ok(_) => Some(()),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "workflow/resume wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "workflow/resume wire error; falling back to local");
            None
        }
    }
}

async fn try_workflow_cancel_via_control(project_root: &str, id: &str) -> Option<()> {
    use animus_control_protocol::types::WorkflowCancelRequest as WireRequest;
    use orchestrator_daemon_runtime::control::{is_method_unavailable, ControlClient};

    let project_root_path = Path::new(project_root);
    let client = match ControlClient::try_connect(project_root_path).await {
        Ok(Some(c)) => c,
        _ => return None,
    };
    match client.workflow_cancel(WireRequest { id: id.to_string(), reason: None }).await {
        Ok(_) => Some(()),
        Err(err) if is_method_unavailable(&err) => {
            tracing::debug!(error = %err, "workflow/cancel wire unavailable; falling back to local");
            None
        }
        Err(err) => {
            tracing::debug!(error = %err, "workflow/cancel wire error; falling back to local");
            None
        }
    }
}

// ---- v0.4.10 wire projection ----
//
// Local `OrchestratorWorkflow` rows are projected to the wire
// `WorkflowRunSummary` shape exposed on `/api/v1/workflows`. The wire
// status enum is kebab-case and lacks `Escalated`; we map `Escalated` to
// `Paused` because both states are "waiting on human / external action"
// from an operator's standpoint. Local-only fields (phases, machine
// state, decision history, current_phase) are intentionally dropped.

fn workflow_to_run_summary(w: OrchestratorWorkflow) -> animus_control_protocol::types::WorkflowRunSummary {
    use animus_control_protocol::types::{WorkflowRunSummary, WorkflowStatus as WireStatus};
    use animus_subject_protocol::SubjectId as WireSubjectId;
    use protocol::orchestrator::WorkflowStatus as LocalStatus;

    let status = match w.status {
        LocalStatus::Pending => WireStatus::Pending,
        LocalStatus::Running => WireStatus::Running,
        LocalStatus::Paused | LocalStatus::Escalated => WireStatus::Paused,
        LocalStatus::Completed => WireStatus::Completed,
        LocalStatus::Failed => WireStatus::Failed,
        LocalStatus::Cancelled => WireStatus::Cancelled,
    };

    let subject_id = if !w.task_id.is_empty() {
        Some(WireSubjectId::from(format!("task:{}", w.task_id)))
    } else if !w.subject.id.is_empty() {
        Some(WireSubjectId::from(format!("{}:{}", w.subject.kind, w.subject.id)))
    } else {
        None
    };

    WorkflowRunSummary {
        id: w.id,
        definition: w.workflow_ref.unwrap_or_default(),
        status,
        subject_id,
        started_at: w.started_at,
        finished_at: w.completed_at,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use orchestrator_core::{
        builtin_agent_runtime_config, builtin_workflow_config, write_agent_runtime_config, write_workflow_config,
        InMemoryServiceHub, RequirementItem, RequirementLinks, RequirementPriority, RequirementStatus,
        WorkflowDefinition, REQUIREMENT_TASK_GENERATION_WORKFLOW_REF,
    };

    use super::*;

    #[tokio::test]
    async fn resolve_workflow_run_dispatch_preserves_request_input_for_custom_subjects() {
        let hub = InMemoryServiceHub::new();

        let dispatch = resolve_workflow_run_dispatch(
            &hub,
            "/tmp/unused",
            WorkflowRunRequest {
                task_id: None,
                requirement_id: None,
                title: Some("custom".to_string()),
                description: Some("custom input".to_string()),
                workflow_ref: Some("ops".to_string()),
                input: Some(json!({"scope":"req-39"})),
            },
        )
        .await
        .expect("dispatch should resolve");

        assert_eq!(dispatch.input, Some(json!({"scope":"req-39"})));
    }

    #[tokio::test]
    async fn resolve_workflow_run_dispatch_from_body_accepts_subject_dispatch() {
        let hub = InMemoryServiceHub::new();
        let dispatch = protocol::SubjectDispatch::for_custom(
            "custom",
            "custom input",
            "ops",
            Some(json!({"scope":"req-39"})),
            "web-api-run",
        );

        let resolved = resolve_workflow_run_dispatch_from_body(
            &hub,
            "/tmp/unused",
            serde_json::to_value(dispatch.clone()).expect("dispatch should serialize"),
        )
        .await
        .expect("dispatch should resolve");

        assert_eq!(resolved.subject_id(), "custom");
        assert_eq!(resolved.workflow_ref, "ops");
        assert_eq!(resolved.input, Some(json!({"scope":"req-39"})));
    }

    #[tokio::test]
    async fn resolve_workflow_run_dispatch_uses_requirement_workflow_default() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut workflow_config = builtin_workflow_config();
        workflow_config.workflows.push(WorkflowDefinition {
            id: REQUIREMENT_TASK_GENERATION_WORKFLOW_REF.to_string(),
            name: "Requirement Task Generation".to_string(),
            description: "test workflow".to_string(),
            phases: vec!["requirements".to_string().into()],
            post_success: None,
            variables: Vec::new(),
        });
        write_workflow_config(temp.path(), &workflow_config).expect("write config");
        write_agent_runtime_config(temp.path(), &builtin_agent_runtime_config()).expect("write runtime config");

        let hub = Arc::new(InMemoryServiceHub::new());
        let now = chrono::Utc::now();
        hub.planning()
            .upsert_requirement(RequirementItem {
                id: "REQ-39".to_string(),
                title: "Dispatch requirement".to_string(),
                description: "requirement dispatch builder test".to_string(),
                body: None,
                legacy_id: None,
                category: None,
                requirement_type: None,
                acceptance_criteria: vec!["starts workflow".to_string()],
                priority: RequirementPriority::Must,
                status: RequirementStatus::Refined,
                source: "test".to_string(),
                tags: Vec::new(),
                links: RequirementLinks::default(),
                comments: Vec::new(),
                relative_path: None,
                linked_task_ids: Vec::new(),
                created_at: now,
                updated_at: now,
            })
            .await
            .expect("requirement should be created");

        let dispatch = resolve_workflow_run_dispatch(
            hub.as_ref(),
            temp.path().to_string_lossy().as_ref(),
            WorkflowRunRequest {
                task_id: None,
                requirement_id: Some("REQ-39".to_string()),
                title: None,
                description: None,
                workflow_ref: None,
                input: Some(json!({"scope":"shared-ingress"})),
            },
        )
        .await
        .expect("dispatch should resolve");

        assert_eq!(dispatch.workflow_ref, REQUIREMENT_TASK_GENERATION_WORKFLOW_REF);
        assert_eq!(dispatch.input, Some(json!({"scope":"shared-ingress"})));
    }
}
