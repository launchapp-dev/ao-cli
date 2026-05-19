//! In-tree (in-process) SubjectBackend adapters wrapping the native
//! `TaskProvider` and `RequirementsProvider` services as `kind=task` and
//! `kind=requirement` subject backends.
//!
//! The v0.4.0 cut moves the CLI/MCP front of the in-tree task and
//! requirements services from `animus task ...` / `animus requirements
//! ...` to the unified `animus subject --kind <kind> ...` surface. To
//! preserve the existing data model (file-backed task/requirement state
//! under `~/.animus/<repo-scope>/`) we expose those services as
//! subject_backend plugins WITHOUT spawning a child process — each
//! adapter runs in-process, speaks the same JSON-RPC frame protocol the
//! plugin host already understands, and registers under the
//! [`SubjectRouter`] alongside externally-installed plugins.
//!
//! Wire ID convention: subjects are addressed on the wire as
//! `task:TASK-NNNN` / `requirement:REQ-NNNN`. The adapter strips the
//! `<kind>:` prefix before calling the in-tree service and re-prefixes
//! on the way back out. The in-tree services themselves continue to
//! work with bare task/requirement IDs.
//!
//! External plugins claiming the same kind win: at router build time
//! [`SubjectRouter::from_initialized_hosts`] rejects duplicate kinds, so
//! callers that need to override the in-tree adapter just install an
//! external plugin claiming `subject_kind:task` (or `:requirement`) and
//! the in-tree adapter is skipped at startup. Per-adapter opt-out env
//! vars [`DISABLE_BUILTIN_TASK_ADAPTER_ENV`] and
//! [`DISABLE_BUILTIN_REQUIREMENTS_ADAPTER_ENV`] also turn the in-tree
//! adapters off explicitly.
//!
//! Anti-deadlock rules:
//!
//! - No mutex is held across `.await` in the adapter loops.
//! - The adapter task owns its `Arc<dyn TaskProvider>` (or
//!   `RequirementsProvider`) and drops it on shutdown.
//! - The duplex stream pair is closed by the `PluginHost` shutdown path
//!   when its inner handle is dropped, which signals EOF to the adapter
//!   loop's `read_line` call and causes graceful task exit.

use std::sync::Arc;

use anyhow::{Context, Result};
use orchestrator_plugin_host::PluginHost;
use serde_json::{json, Map, Value};
use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

use animus_plugin_protocol::{
    error_codes, InitializeResult, PluginCapabilities, PluginInfo, RpcError, RpcRequest, RpcResponse,
    PLUGIN_KIND_SUBJECT_BACKEND, PROTOCOL_VERSION,
};
use orchestrator_providers::{RequirementsProvider, TaskProvider};
use protocol::orchestrator::{
    OrchestratorTask, Priority, RequirementItem, RequirementPriority, RequirementStatus, RequirementType, RiskLevel,
    Scope, TaskCreateInput, TaskFilter, TaskStatus, TaskType, TaskUpdateInput,
};

/// Env var that disables the in-tree task subject adapter.
pub const DISABLE_BUILTIN_TASK_ADAPTER_ENV: &str = "ANIMUS_DAEMON_DISABLE_BUILTIN_TASK_ADAPTER";

/// Env var that disables the in-tree requirements subject adapter.
pub const DISABLE_BUILTIN_REQUIREMENTS_ADAPTER_ENV: &str = "ANIMUS_DAEMON_DISABLE_BUILTIN_REQUIREMENTS_ADAPTER";

/// Returns `true` when the given env var is set to a truthy value.
pub fn env_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => {
            let trimmed = value.trim().to_ascii_lowercase();
            !trimmed.is_empty() && trimmed != "0" && trimmed != "false" && trimmed != "no" && trimmed != "off"
        }
        Err(_) => false,
    }
}

/// Internal plugin name surfaced for the in-tree task adapter.
pub const BUILTIN_TASK_PLUGIN_NAME: &str = "animus-builtin-task";
/// Internal plugin name surfaced for the in-tree requirements adapter.
pub const BUILTIN_REQUIREMENTS_PLUGIN_NAME: &str = "animus-builtin-requirements";

/// Subject kind handled by the in-tree task adapter.
pub const TASK_KIND: &str = "task";
/// Subject kind handled by the in-tree requirements adapter.
pub const REQUIREMENT_KIND: &str = "requirement";

/// Spawn an in-process [`PluginHost`] backed by the supplied
/// [`TaskProvider`], speaking `task/<verb>` JSON-RPC over a duplex
/// channel.
pub fn spawn_inproc_task_backend(provider: Arc<dyn TaskProvider>) -> PluginHost {
    spawn_inproc_subject_adapter(BUILTIN_TASK_PLUGIN_NAME.to_string(), TASK_KIND.to_string(), Backend::Task(provider))
}

/// Spawn an in-process [`PluginHost`] backed by the supplied
/// [`RequirementsProvider`], speaking `requirement/<verb>` JSON-RPC over
/// a duplex channel.
pub fn spawn_inproc_requirements_backend(provider: Arc<dyn RequirementsProvider>) -> PluginHost {
    spawn_inproc_subject_adapter(
        BUILTIN_REQUIREMENTS_PLUGIN_NAME.to_string(),
        REQUIREMENT_KIND.to_string(),
        Backend::Requirement(provider),
    )
}

enum Backend {
    Task(Arc<dyn TaskProvider>),
    Requirement(Arc<dyn RequirementsProvider>),
}

fn spawn_inproc_subject_adapter(plugin_name: String, kind: String, backend: Backend) -> PluginHost {
    let (host_reader, mut plugin_writer) = duplex(64 * 1024);
    let (plugin_reader, host_writer) = duplex(64 * 1024);

    let kind_for_task = kind.clone();
    let plugin_name_for_task = plugin_name.clone();

    tokio::spawn(async move {
        let mut reader = BufReader::new(plugin_reader);
        loop {
            let mut line = String::new();
            let n = match reader.read_line(&mut line).await {
                Ok(n) => n,
                Err(_) => break,
            };
            if n == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let request: RpcRequest = match serde_json::from_str(trimmed) {
                Ok(req) => req,
                Err(_err) => {
                    continue;
                }
            };
            let response = handle_request(&plugin_name_for_task, &kind_for_task, &backend, request).await;
            if let Some(resp) = response {
                let mut encoded = match serde_json::to_string(&resp) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                encoded.push('\n');
                if plugin_writer.write_all(encoded.as_bytes()).await.is_err() {
                    break;
                }
            }
        }
    });

    PluginHost::from_streams(plugin_name, host_reader, host_writer)
}

async fn handle_request(plugin_name: &str, kind: &str, backend: &Backend, request: RpcRequest) -> Option<RpcResponse> {
    match request.method.as_str() {
        "initialize" => Some(RpcResponse::ok(request.id, build_initialize_result(plugin_name, kind))),
        "initialized" => None,
        method => {
            let params = request.params.clone();
            let result = match backend {
                Backend::Task(provider) => dispatch_task_method(provider.clone(), method, params).await,
                Backend::Requirement(provider) => dispatch_requirement_method(provider.clone(), method, params).await,
            };
            Some(match result {
                Ok(value) => RpcResponse::ok(request.id, value),
                Err(error) => RpcResponse::err(request.id, error),
            })
        }
    }
}

fn build_initialize_result(plugin_name: &str, kind: &str) -> Value {
    let result = InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        plugin_info: PluginInfo {
            name: plugin_name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            plugin_kind: PLUGIN_KIND_SUBJECT_BACKEND.to_string(),
            description: Some(format!("in-tree {kind} subject adapter")),
        },
        capabilities: PluginCapabilities {
            subject_kinds: vec![kind.to_string()],
            methods: vec![
                format!("{kind}/list"),
                format!("{kind}/get"),
                format!("{kind}/create"),
                format!("{kind}/update"),
                format!("{kind}/next"),
                format!("{kind}/status"),
            ],
            ..PluginCapabilities::default()
        },
    };
    serde_json::to_value(result).expect("InitializeResult always serializes")
}

// ===========================================================================
// Wire ID prefix helpers
// ===========================================================================

/// Strip a leading `<kind>:` prefix from a wire id, returning the bare
/// in-tree id. Inputs without the prefix are returned unchanged so
/// callers can pass either form.
pub fn strip_kind_prefix(kind: &str, wire_id: &str) -> String {
    let prefix = format!("{kind}:");
    wire_id.strip_prefix(&prefix).map(str::to_string).unwrap_or_else(|| wire_id.to_string())
}

/// Prepend a `<kind>:` prefix to a bare id when one is not already
/// present.
pub fn add_kind_prefix(kind: &str, bare_id: &str) -> String {
    let prefix = format!("{kind}:");
    if bare_id.starts_with(&prefix) {
        bare_id.to_string()
    } else {
        format!("{prefix}{bare_id}")
    }
}

// ===========================================================================
// Task dispatch
// ===========================================================================

async fn dispatch_task_method(
    provider: Arc<dyn TaskProvider>,
    method: &str,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let verb = method.strip_prefix("task/").ok_or_else(|| not_found_method(method))?;
    match verb {
        "list" => task_list(provider, params).await,
        "get" => task_get(provider, params).await,
        "create" => task_create(provider, params).await,
        "update" => task_update(provider, params).await,
        "next" => task_next(provider, params).await,
        "status" => task_status(provider, params).await,
        _ => Err(not_found_method(method)),
    }
}

async fn task_list(provider: Arc<dyn TaskProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let limit = params.as_ref().and_then(|p| p.get("limit")).and_then(Value::as_u64).map(|n| n as usize);
    let mut filter = TaskFilter::default();
    if let Some(p) = params.as_ref() {
        if let Some(status_values) = p.get("status").and_then(Value::as_array) {
            if let Some(first) = status_values.first().and_then(Value::as_str) {
                if let Ok(parsed) = first.parse::<TaskStatus>() {
                    filter.status = Some(parsed);
                }
            }
        } else if let Some(status_str) = p.get("status").and_then(Value::as_str) {
            if let Ok(parsed) = status_str.parse::<TaskStatus>() {
                filter.status = Some(parsed);
            }
        }
    }
    let tasks = provider.list_filtered(filter).await.map_err(|e| internal_error(format!("task list failed: {e}")))?;
    let truncated: Vec<Value> =
        tasks.into_iter().take(limit.unwrap_or(usize::MAX)).map(|task| task_to_subject_json(&task)).collect();
    Ok(json!({
        "subjects": truncated,
        "next_cursor": Value::Null,
    }))
}

async fn task_get(provider: Arc<dyn TaskProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let id = require_id(&params, "task")?;
    let task = provider.get(&id).await.map_err(|e| not_found(format!("task {id} not found: {e}")))?;
    Ok(task_to_subject_json(&task))
}

async fn task_create(provider: Arc<dyn TaskProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let params = params.unwrap_or_else(|| Value::Object(Map::new()));
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("task create requires --title"))?
        .to_string();
    let description = params.get("body").and_then(Value::as_str).unwrap_or_default().to_string();
    let priority = params.get("priority").and_then(Value::as_str).and_then(parse_priority);
    let tags = params
        .get("labels")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect::<Vec<_>>())
        .unwrap_or_default();
    let input = TaskCreateInput {
        title,
        description,
        task_type: Some(TaskType::Feature),
        priority,
        created_by: Some("animus.subject.cli".to_string()),
        tags,
        linked_requirements: Vec::new(),
        linked_architecture_entities: Vec::new(),
    };
    let task = provider.create(input).await.map_err(|e| internal_error(format!("task create failed: {e}")))?;
    // Apply status patch separately if provided (TaskCreateInput has no
    // status field; new tasks start in Backlog and need an explicit
    // status set).
    let status_override = params.get("status").and_then(Value::as_str).and_then(|s| s.parse::<TaskStatus>().ok());
    let task = if let Some(status) = status_override {
        provider
            .set_status(&task.id, status, false)
            .await
            .map_err(|e| internal_error(format!("task status set failed: {e}")))?
    } else {
        task
    };
    Ok(task_to_subject_json(&task))
}

async fn task_update(provider: Arc<dyn TaskProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let id = require_id(&params, "task")?;
    let patch =
        params.as_ref().and_then(|p| p.get("patch")).cloned().unwrap_or_else(|| params.clone().unwrap_or(Value::Null));
    let mut input = TaskUpdateInput {
        title: None,
        description: None,
        priority: None,
        status: None,
        assignee: None,
        tags: None,
        updated_by: Some("animus.subject.cli".to_string()),
        deadline: None,
        linked_architecture_entities: None,
    };
    if let Some(status_str) = patch.get("status").and_then(Value::as_str) {
        let status: TaskStatus = status_str
            .parse()
            .map_err(|err: String| invalid_params(format!("invalid task status '{status_str}': {err}")))?;
        input.status = Some(status);
    }
    if let Some(priority_str) = patch.get("priority").and_then(Value::as_str) {
        input.priority = parse_priority(priority_str);
    }
    if let Some(labels) = patch.get("labels").and_then(Value::as_array) {
        input.tags = Some(labels.iter().filter_map(|v| v.as_str().map(str::to_string)).collect());
    }
    if let Some(title) = patch.get("title").and_then(Value::as_str) {
        input.title = Some(title.to_string());
    }
    if let Some(body) = patch.get("body").and_then(Value::as_str) {
        input.description = Some(body.to_string());
    }
    let task = provider.update(&id, input).await.map_err(|e| internal_error(format!("task update failed: {e}")))?;
    Ok(task_to_subject_json(&task))
}

async fn task_next(provider: Arc<dyn TaskProvider>, _params: Option<Value>) -> Result<Value, RpcError> {
    let task = provider.next_task().await.map_err(|e| internal_error(format!("task next failed: {e}")))?;
    Ok(match task {
        Some(t) => task_to_subject_json(&t),
        None => Value::Null,
    })
}

async fn task_status(provider: Arc<dyn TaskProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let id = require_id(&params, "task")?;
    let status_str = params
        .as_ref()
        .and_then(|p| p.get("status"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("task status requires --status"))?;
    let status: TaskStatus = status_str
        .parse()
        .map_err(|err: String| invalid_params(format!("invalid task status '{status_str}': {err}")))?;
    let task = provider
        .set_status(&id, status, true)
        .await
        .map_err(|e| internal_error(format!("task set_status failed: {e}")))?;
    Ok(task_to_subject_json(&task))
}

// ===========================================================================
// Requirement dispatch
// ===========================================================================

async fn dispatch_requirement_method(
    provider: Arc<dyn RequirementsProvider>,
    method: &str,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let verb = method.strip_prefix("requirement/").ok_or_else(|| not_found_method(method))?;
    match verb {
        "list" => requirement_list(provider, params).await,
        "get" => requirement_get(provider, params).await,
        "create" => requirement_create(provider, params).await,
        "update" => requirement_update(provider, params).await,
        "next" => requirement_next(provider, params).await,
        "status" => requirement_status(provider, params).await,
        _ => Err(not_found_method(method)),
    }
}

async fn requirement_list(provider: Arc<dyn RequirementsProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let limit = params.as_ref().and_then(|p| p.get("limit")).and_then(Value::as_u64).map(|n| n as usize);
    let status_filter: Option<RequirementStatus> = params
        .as_ref()
        .and_then(|p| p.get("status"))
        .and_then(|v| match v {
            Value::Array(arr) => arr.first().and_then(Value::as_str).map(str::to_string),
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .and_then(|s| s.parse::<RequirementStatus>().ok());
    let items =
        provider.list_requirements().await.map_err(|e| internal_error(format!("requirement list failed: {e}")))?;
    let filtered: Vec<Value> = items
        .into_iter()
        .filter(|r| match status_filter {
            Some(target) => r.status == target,
            None => true,
        })
        .take(limit.unwrap_or(usize::MAX))
        .map(|r| requirement_to_subject_json(&r))
        .collect();
    Ok(json!({
        "subjects": filtered,
        "next_cursor": Value::Null,
    }))
}

async fn requirement_get(provider: Arc<dyn RequirementsProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let id = require_id(&params, "requirement")?;
    let item =
        provider.get_requirement(&id).await.map_err(|e| not_found(format!("requirement {id} not found: {e}")))?;
    Ok(requirement_to_subject_json(&item))
}

async fn requirement_create(provider: Arc<dyn RequirementsProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let params = params.unwrap_or_else(|| Value::Object(Map::new()));
    let title = params
        .get("title")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("requirement create requires --title"))?
        .to_string();
    let description = params.get("body").and_then(Value::as_str).unwrap_or_default().to_string();
    let priority = params
        .get("priority")
        .and_then(Value::as_str)
        .and_then(parse_requirement_priority)
        .unwrap_or(RequirementPriority::Should);
    let status_override =
        params.get("status").and_then(Value::as_str).and_then(|s| s.parse::<RequirementStatus>().ok());
    let now = chrono::Utc::now();
    let id = format!("REQ-{}", now.timestamp_millis());
    let item = RequirementItem {
        id,
        title,
        description,
        body: None,
        legacy_id: None,
        category: None,
        requirement_type: Some(RequirementType::Functional),
        acceptance_criteria: Vec::new(),
        priority,
        status: status_override.unwrap_or(RequirementStatus::Draft),
        source: "animus.subject.cli".to_string(),
        tags: params
            .get("labels")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        links: Default::default(),
        comments: Vec::new(),
        relative_path: None,
        linked_task_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    let item = provider
        .upsert_requirement(item)
        .await
        .map_err(|e| internal_error(format!("requirement create failed: {e}")))?;
    Ok(requirement_to_subject_json(&item))
}

async fn requirement_update(provider: Arc<dyn RequirementsProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let id = require_id(&params, "requirement")?;
    let patch =
        params.as_ref().and_then(|p| p.get("patch")).cloned().unwrap_or_else(|| params.clone().unwrap_or(Value::Null));

    let mut item =
        provider.get_requirement(&id).await.map_err(|e| not_found(format!("requirement {id} not found: {e}")))?;

    if let Some(status_str) = patch.get("status").and_then(Value::as_str) {
        item.status = status_str
            .parse::<RequirementStatus>()
            .map_err(|err| invalid_params(format!("invalid requirement status '{status_str}': {err}")))?;
    }
    if let Some(priority_str) = patch.get("priority").and_then(Value::as_str) {
        if let Some(parsed) = parse_requirement_priority(priority_str) {
            item.priority = parsed;
        }
    }
    if let Some(labels) = patch.get("labels").and_then(Value::as_array) {
        item.tags = labels.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    }
    if let Some(title) = patch.get("title").and_then(Value::as_str) {
        item.title = title.to_string();
    }
    if let Some(body) = patch.get("body").and_then(Value::as_str) {
        item.description = body.to_string();
    }
    item.updated_at = chrono::Utc::now();

    let item = provider
        .upsert_requirement(item)
        .await
        .map_err(|e| internal_error(format!("requirement update failed: {e}")))?;
    Ok(requirement_to_subject_json(&item))
}

async fn requirement_next(provider: Arc<dyn RequirementsProvider>, _params: Option<Value>) -> Result<Value, RpcError> {
    let items =
        provider.list_requirements().await.map_err(|e| internal_error(format!("requirement list failed: {e}")))?;
    // Pick the highest-priority Draft / Refined / Planned item; Critical
    // > High > Medium > Low.
    let mut ranked: Vec<RequirementItem> = items
        .into_iter()
        .filter(|r| {
            matches!(r.status, RequirementStatus::Draft | RequirementStatus::Refined | RequirementStatus::Planned)
        })
        .collect();
    ranked.sort_by_key(|r| match r.priority {
        RequirementPriority::Must => 0,
        RequirementPriority::Should => 1,
        RequirementPriority::Could => 2,
        RequirementPriority::Wont => 3,
    });
    Ok(match ranked.into_iter().next() {
        Some(item) => requirement_to_subject_json(&item),
        None => Value::Null,
    })
}

async fn requirement_status(provider: Arc<dyn RequirementsProvider>, params: Option<Value>) -> Result<Value, RpcError> {
    let id = require_id(&params, "requirement")?;
    let status_str = params
        .as_ref()
        .and_then(|p| p.get("status"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params("requirement status requires --status"))?;
    let parsed: RequirementStatus = status_str
        .parse()
        .map_err(|err| invalid_params(format!("invalid requirement status '{status_str}': {err}")))?;
    let mut item =
        provider.get_requirement(&id).await.map_err(|e| not_found(format!("requirement {id} not found: {e}")))?;
    item.status = parsed;
    item.updated_at = chrono::Utc::now();
    let item = provider
        .upsert_requirement(item)
        .await
        .map_err(|e| internal_error(format!("requirement update failed: {e}")))?;
    Ok(requirement_to_subject_json(&item))
}

// ===========================================================================
// Subject JSON shaping
// ===========================================================================

fn task_to_subject_json(task: &OrchestratorTask) -> Value {
    let labels = task.tags.clone();
    let status_label = task.status.to_string();
    let priority_bucket = match task.priority {
        Priority::Critical => 4,
        Priority::High => 3,
        Priority::Medium => 2,
        Priority::Low => 1,
    };
    let risk = match task.risk {
        RiskLevel::High => "high",
        RiskLevel::Medium => "medium",
        RiskLevel::Low => "low",
    };
    let scope = match task.scope {
        Scope::Large => "large",
        Scope::Medium => "medium",
        Scope::Small => "small",
    };
    json!({
        "id": add_kind_prefix(TASK_KIND, &task.id),
        "kind": TASK_KIND,
        "title": task.title,
        "description": task.description,
        "status": status_label,
        "priority": priority_bucket,
        "labels": labels,
        "custom": {
            "type": task.task_type.as_str(),
            "risk": risk,
            "scope": scope,
            "paused": task.paused,
            "cancelled": task.cancelled,
        },
        "created_at": task.metadata.created_at,
        "updated_at": task.metadata.updated_at,
    })
}

fn requirement_to_subject_json(item: &RequirementItem) -> Value {
    let priority_bucket = match item.priority {
        RequirementPriority::Must => 4,
        RequirementPriority::Should => 3,
        RequirementPriority::Could => 2,
        RequirementPriority::Wont => 1,
    };
    let status_label = item.status.to_string();
    let kind_label = match item.requirement_type {
        Some(RequirementType::Product) => "product",
        Some(RequirementType::Functional) => "functional",
        Some(RequirementType::NonFunctional) => "non-functional",
        Some(RequirementType::Technical) => "technical",
        Some(RequirementType::Other) => "other",
        None => "unspecified",
    };
    json!({
        "id": add_kind_prefix(REQUIREMENT_KIND, &item.id),
        "kind": REQUIREMENT_KIND,
        "title": item.title,
        "description": item.description,
        "status": status_label,
        "priority": priority_bucket,
        "labels": item.tags,
        "custom": {
            "type": kind_label,
            "acceptance_criteria_count": item.acceptance_criteria.len(),
            "source": item.source,
        },
        "created_at": item.created_at,
        "updated_at": item.updated_at,
    })
}

// ===========================================================================
// Param helpers
// ===========================================================================

fn require_id(params: &Option<Value>, kind: &str) -> Result<String, RpcError> {
    let wire_id = params
        .as_ref()
        .and_then(|p| p.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_params(format!("{kind} call requires --id")))?
        .trim()
        .to_string();
    if wire_id.is_empty() {
        return Err(invalid_params(format!("{kind} --id must not be empty")));
    }
    Ok(strip_kind_prefix(kind, &wire_id))
}

fn parse_priority(raw: &str) -> Option<Priority> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "critical" | "p0" => Some(Priority::Critical),
        "high" | "p1" => Some(Priority::High),
        "medium" | "p2" => Some(Priority::Medium),
        "low" | "p3" => Some(Priority::Low),
        _ => None,
    }
}

fn parse_requirement_priority(raw: &str) -> Option<RequirementPriority> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "must" | "p0" | "critical" => Some(RequirementPriority::Must),
        "should" | "p1" | "high" => Some(RequirementPriority::Should),
        "could" | "p2" | "medium" => Some(RequirementPriority::Could),
        "wont" | "won't" | "p3" | "low" => Some(RequirementPriority::Wont),
        _ => None,
    }
}

fn invalid_params(msg: impl Into<String>) -> RpcError {
    RpcError { code: error_codes::INVALID_PARAMS, message: msg.into(), data: None }
}

fn not_found(msg: impl Into<String>) -> RpcError {
    RpcError { code: error_codes::INVALID_PARAMS, message: msg.into(), data: Some(json!({ "category": "not_found" })) }
}

fn not_found_method(method: &str) -> RpcError {
    RpcError {
        code: error_codes::METHOD_NOT_FOUND,
        message: format!("in-tree subject backend does not implement '{method}'"),
        data: None,
    }
}

fn internal_error(msg: impl Into<String>) -> RpcError {
    RpcError { code: error_codes::INTERNAL_ERROR, message: msg.into(), data: None }
}

// ===========================================================================
// Public helper: build the (kind, host) pairs the daemon should mount
// alongside externally-discovered plugins.
// ===========================================================================

/// Try to build the in-tree adapters for a project. Returns the pairs
/// `(kind, plugin_name, host)` ready to be inserted into the
/// [`orchestrator_plugin_host::SubjectRouter`] kind→host map.
///
/// Honors [`DISABLE_BUILTIN_TASK_ADAPTER_ENV`] and
/// [`DISABLE_BUILTIN_REQUIREMENTS_ADAPTER_ENV`].
///
/// The caller is responsible for instantiating a [`FileServiceHub`] from
/// the project root and threading the resulting providers in — we keep
/// the dependency here to a bare `Arc<dyn TaskProvider>` /
/// `Arc<dyn RequirementsProvider>` so this module remains independent
/// of the concrete service hub type.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltinAdapterOpts {
    /// Force the task adapter off regardless of env (useful for tests).
    pub force_disable_task: bool,
    /// Force the requirements adapter off regardless of env.
    pub force_disable_requirements: bool,
}

/// Returns `true` when the task adapter should mount given the supplied
/// override + env state.
pub fn task_adapter_enabled(opts: &BuiltinAdapterOpts) -> bool {
    if opts.force_disable_task {
        return false;
    }
    !env_truthy(DISABLE_BUILTIN_TASK_ADAPTER_ENV)
}

/// Returns `true` when the requirements adapter should mount.
pub fn requirements_adapter_enabled(opts: &BuiltinAdapterOpts) -> bool {
    if opts.force_disable_requirements {
        return false;
    }
    !env_truthy(DISABLE_BUILTIN_REQUIREMENTS_ADAPTER_ENV)
}

/// Build the kind→host pairs for the in-tree adapters that are
/// currently enabled. The returned `Vec` is `(kind, host)`; the daemon
/// inserts these into the [`SubjectRouter`] before merging in external
/// plugin hosts so external plugins claiming the same kind win via the
/// router's duplicate-kind rejection.
pub fn build_inproc_subject_adapters(
    task: Option<Arc<dyn TaskProvider>>,
    requirements: Option<Arc<dyn RequirementsProvider>>,
    opts: &BuiltinAdapterOpts,
) -> Vec<(String, String, PluginHost)> {
    let mut out = Vec::new();
    if let Some(provider) = task {
        if task_adapter_enabled(opts) {
            out.push((
                TASK_KIND.to_string(),
                BUILTIN_TASK_PLUGIN_NAME.to_string(),
                spawn_inproc_task_backend(provider),
            ));
        }
    }
    if let Some(provider) = requirements {
        if requirements_adapter_enabled(opts) {
            out.push((
                REQUIREMENT_KIND.to_string(),
                BUILTIN_REQUIREMENTS_PLUGIN_NAME.to_string(),
                spawn_inproc_requirements_backend(provider),
            ));
        }
    }
    out
}

/// Builds a [`FileServiceHub`]-backed task adapter via the project root.
///
/// This is a small convenience wrapper used by the CLI `subject` command
/// (one-shot invocations) so it can keep going when the daemon isn't
/// running. It instantiates the hub, wraps it in the same
/// [`BuiltinTaskProvider`] the daemon uses, and returns the adapter
/// pairs ready to mount.
///
/// Returns `Ok(Vec::new())` when both adapters are disabled.
pub fn build_inproc_adapters_for_project(
    project_root: &std::path::Path,
    opts: &BuiltinAdapterOpts,
) -> Result<Vec<(String, String, PluginHost)>> {
    use orchestrator_core::FileServiceHub;
    use orchestrator_providers::{BuiltinRequirementsProvider, BuiltinTaskProvider};

    let task_on = task_adapter_enabled(opts);
    let req_on = requirements_adapter_enabled(opts);
    if !task_on && !req_on {
        return Ok(Vec::new());
    }
    let hub = Arc::new(
        FileServiceHub::new(project_root)
            .with_context(|| format!("FileServiceHub init for {}", project_root.display()))?,
    );
    let task_provider: Option<Arc<dyn TaskProvider>> =
        if task_on { Some(Arc::new(BuiltinTaskProvider::new(hub.clone())) as Arc<dyn TaskProvider>) } else { None };
    let req_provider: Option<Arc<dyn RequirementsProvider>> = if req_on {
        Some(Arc::new(BuiltinRequirementsProvider::new(hub.clone())) as Arc<dyn RequirementsProvider>)
    } else {
        None
    };
    Ok(build_inproc_subject_adapters(task_provider, req_provider, opts))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize tests that mutate process-wide env vars so parallel
    /// cargo runs do not race.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn strip_and_add_kind_prefix_round_trip() {
        let bare = "TASK-0042";
        let wire = add_kind_prefix("task", bare);
        assert_eq!(wire, "task:TASK-0042");
        let back = strip_kind_prefix("task", &wire);
        assert_eq!(back, bare);
        // Idempotent
        let still_bare = strip_kind_prefix("task", bare);
        assert_eq!(still_bare, bare);
        let still_wire = add_kind_prefix("task", &wire);
        assert_eq!(still_wire, wire);
    }

    #[test]
    fn parse_priority_canonicalizes_aliases() {
        assert_eq!(parse_priority("p0"), Some(Priority::Critical));
        assert_eq!(parse_priority("HIGH"), Some(Priority::High));
        assert_eq!(parse_priority("garbage"), None);
    }

    #[test]
    fn adapter_opts_default_enables_both() {
        let _g = ENV_LOCK.lock().unwrap();
        let opts = BuiltinAdapterOpts::default();
        std::env::remove_var(DISABLE_BUILTIN_TASK_ADAPTER_ENV);
        std::env::remove_var(DISABLE_BUILTIN_REQUIREMENTS_ADAPTER_ENV);
        assert!(task_adapter_enabled(&opts));
        assert!(requirements_adapter_enabled(&opts));
    }

    #[test]
    fn disable_env_overrides_default() {
        let _g = ENV_LOCK.lock().unwrap();
        let opts = BuiltinAdapterOpts::default();
        std::env::set_var(DISABLE_BUILTIN_TASK_ADAPTER_ENV, "1");
        assert!(!task_adapter_enabled(&opts));
        std::env::remove_var(DISABLE_BUILTIN_TASK_ADAPTER_ENV);
    }

    #[test]
    fn force_disable_overrides_env() {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::remove_var(DISABLE_BUILTIN_TASK_ADAPTER_ENV);
        std::env::remove_var(DISABLE_BUILTIN_REQUIREMENTS_ADAPTER_ENV);
        let opts = BuiltinAdapterOpts { force_disable_task: true, force_disable_requirements: true };
        assert!(!task_adapter_enabled(&opts));
        assert!(!requirements_adapter_enabled(&opts));
    }
}
