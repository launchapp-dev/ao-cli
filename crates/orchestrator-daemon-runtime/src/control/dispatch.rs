//! [`InProcessSurface`] — daemon-side [`ControlSurface`] implementation.
//!
//! Translates each control-protocol method into the existing in-process
//! service call and returns the wire-shaped response. **No behavior
//! change.** This commit is the wire wrapper only; CLI/MCP/WebAPI
//! migrations follow in C6/C7/C8.
//!
//! ## Method coverage
//!
//! - **Subject ops** (`subject/list`, `subject/get`, `subject/create`,
//!   `subject/update`, `subject/next`, `subject/status`, `subject/watch`):
//!   forwarded as `<kind>/<verb>` JSON-RPC calls through
//!   [`SubjectPluginDispatch::route_call`]. The kind is extracted from
//!   either the request filter, the embedded subject id, or the explicit
//!   `kind` field depending on the verb. When the requested kind has no
//!   mounted backend the wire response is
//!   [`ControlError::NotFound`].
//! - **Daemon ops** (`daemon/status`, `daemon/health`, `daemon/agents`):
//!   surfaced directly from process state and the active dispatch
//!   handles. Streaming variants (`daemon/events`, `daemon/logs`) drain
//!   the broadcast buses created at daemon startup.
//! - **Workflow / agent / queue / project / plugin ops**: return
//!   [`ControlError::NotSupported`] for now. C6 (CLI cutover) is the
//!   first commit that needs them on the wire; the underlying services
//!   exist and will be wired through then.
//!
//! The surface deliberately fails closed: a missing dispatch handle
//! returns [`ControlError::Unavailable`], an unknown subject kind returns
//! [`ControlError::NotFound`], and any in-process error becomes
//! [`ControlError::Internal`]. Transports never see panics from this
//! layer.
//!
//! ## Anti-deadlock rules
//!
//! - No `tokio::sync::Mutex` for surface state. The handles inside are
//!   `Arc` clones of services set once at daemon startup.
//! - No locks held across `.await`. JSON translation is synchronous;
//!   the only `.await` is on the underlying service call.
//! - No `Drop` impls perform lock work.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use futures_core::Stream;
use serde_json::{json, Value};

use animus_control_protocol::{
    control_trait::{DaemonEventStream, DaemonLogStream, SubjectWatchStream},
    types::{
        AgentCancelRequest, AgentRunRequest, AgentRunResult, AgentStatus, AgentStatusRequest, DaemonAgentsResponse,
        DaemonEventsRequest, DaemonHealthResponse, DaemonHealthStatus, DaemonLogEntry, DaemonLogsRequest,
        DaemonRunEvent as WireDaemonRunEvent, DaemonStatusResponse, PluginBrowseRequest, PluginCallRequest,
        PluginCallResponse, PluginHealth, PluginInfo, PluginInfoRequest, PluginInstallRequest, PluginInstallResponse,
        PluginListRequest, PluginListResponse, PluginPingRequest, PluginPingResponse, PluginSearchRequest,
        PluginSearchResponse, PluginUninstallRequest, PluginUpdateRequest, PluginUpdateResponse, ProjectInfo,
        ProjectInitRequest, ProjectSetupRequest, ProjectStatusResponse, QueueDropRequest, QueueEnqueueRequest,
        QueueEntry, QueueHoldRequest, QueueListRequest, QueueListResponse, QueueReleaseRequest, QueueReorderRequest,
        QueueStats, SubjectCreateRequest, SubjectGetRequest, SubjectListRequest, SubjectListResponse,
        SubjectNextRequest, SubjectNextResponse, SubjectStatusRequest, SubjectUpdateRequest, SubjectWatchRequest, Unit,
        WorkflowCancelRequest, WorkflowExecuteRequest, WorkflowGetRequest, WorkflowListRequest, WorkflowListResponse,
        WorkflowPauseRequest, WorkflowResumeRequest, WorkflowRun, WorkflowRunRequest, WorkflowRunStart,
    },
    ControlError, ControlSurface,
};
use animus_subject_protocol_wire::{Subject as WireSubject, SubjectChangedEvent};
use tokio::sync::broadcast;

use crate::log_storage::{resolve_log_storage_dispatch, LogStorageDispatch};
use crate::subject_dispatch::SubjectPluginDispatch;

use super::routing::{AgentRouting, DaemonOpsRouting, PluginRouting, QueueRouting, WorkflowRouting};
use super::streaming::{DaemonEventBus, DaemonLogBus};

/// In-process [`ControlSurface`] used by the daemon's control server.
///
/// Holds [`Arc`]-cloned handles into the daemon's running service set.
/// Construct one at daemon startup (after [`SubjectPluginDispatch`] and
/// the broadcast buses resolve) and pass it into [`super::ControlServer::start`].
///
/// Cheap to clone; clones share the same underlying handles. Surface
/// methods never mutate the surface itself — all mutations flow through
/// the in-process services the surface wraps.
#[derive(Clone)]
pub struct InProcessSurface {
    project_root: PathBuf,
    daemon_pid: u32,
    daemon_version: String,
    started_at: SystemTime,
    subject_dispatch: Option<Arc<SubjectPluginDispatch>>,
    event_bus: Option<DaemonEventBus>,
    log_bus: Option<DaemonLogBus>,
    plugin_routing: Option<Arc<dyn PluginRouting>>,
    daemon_ops_routing: Option<Arc<dyn DaemonOpsRouting>>,
    workflow_routing: Option<Arc<dyn WorkflowRouting>>,
    queue_routing: Option<Arc<dyn QueueRouting>>,
    agent_routing: Option<Arc<dyn AgentRouting>>,
}

impl InProcessSurface {
    /// Start building a new surface for `project_root`.
    pub fn builder(project_root: PathBuf) -> InProcessSurfaceBuilder {
        InProcessSurfaceBuilder {
            project_root,
            daemon_pid: std::process::id(),
            daemon_version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: SystemTime::now(),
            subject_dispatch: None,
            event_bus: None,
            log_bus: None,
            plugin_routing: None,
            daemon_ops_routing: None,
            workflow_routing: None,
            queue_routing: None,
            agent_routing: None,
        }
    }

    /// Project root the surface is bound to.
    pub fn project_root(&self) -> &PathBuf {
        &self.project_root
    }

    /// Borrow the subject dispatch handle, if one was provided.
    pub fn subject_dispatch(&self) -> Option<&Arc<SubjectPluginDispatch>> {
        self.subject_dispatch.as_ref()
    }

    /// Borrow the daemon event bus, if one was provided.
    pub fn event_bus(&self) -> Option<&DaemonEventBus> {
        self.event_bus.as_ref()
    }

    /// Borrow the daemon log bus, if one was provided.
    pub fn log_bus(&self) -> Option<&DaemonLogBus> {
        self.log_bus.as_ref()
    }

    fn subject_dispatch_or_unavailable(&self) -> Result<&Arc<SubjectPluginDispatch>, ControlError> {
        self.subject_dispatch
            .as_ref()
            .ok_or_else(|| ControlError::Unavailable("subject dispatch not initialized".to_string()))
    }

    async fn route_subject_call(&self, kind: &str, verb: &str, params: Option<Value>) -> Result<Value, ControlError> {
        let dispatch = self.subject_dispatch_or_unavailable()?;
        let method = format!("{kind}/{verb}");
        dispatch.route_call(&method, params).await.map_err(rpc_error_to_control_error)
    }
}

impl std::fmt::Debug for InProcessSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcessSurface")
            .field("project_root", &self.project_root)
            .field("daemon_pid", &self.daemon_pid)
            .field("daemon_version", &self.daemon_version)
            .field("subject_dispatch", &self.subject_dispatch.is_some())
            .field("event_bus", &self.event_bus.is_some())
            .field("log_bus", &self.log_bus.is_some())
            .field("plugin_routing", &self.plugin_routing.is_some())
            .field("daemon_ops_routing", &self.daemon_ops_routing.is_some())
            .field("workflow_routing", &self.workflow_routing.is_some())
            .field("queue_routing", &self.queue_routing.is_some())
            .field("agent_routing", &self.agent_routing.is_some())
            .finish_non_exhaustive()
    }
}

/// Fluent builder for [`InProcessSurface`].
pub struct InProcessSurfaceBuilder {
    project_root: PathBuf,
    daemon_pid: u32,
    daemon_version: String,
    started_at: SystemTime,
    subject_dispatch: Option<Arc<SubjectPluginDispatch>>,
    event_bus: Option<DaemonEventBus>,
    log_bus: Option<DaemonLogBus>,
    plugin_routing: Option<Arc<dyn PluginRouting>>,
    daemon_ops_routing: Option<Arc<dyn DaemonOpsRouting>>,
    workflow_routing: Option<Arc<dyn WorkflowRouting>>,
    queue_routing: Option<Arc<dyn QueueRouting>>,
    agent_routing: Option<Arc<dyn AgentRouting>>,
}

impl InProcessSurfaceBuilder {
    /// Override the reported daemon PID (defaults to the current process).
    pub fn daemon_pid(mut self, pid: u32) -> Self {
        self.daemon_pid = pid;
        self
    }

    /// Override the reported daemon version (defaults to the runtime crate's
    /// `CARGO_PKG_VERSION`).
    pub fn daemon_version(mut self, version: impl Into<String>) -> Self {
        self.daemon_version = version.into();
        self
    }

    /// Override the daemon start timestamp (defaults to `SystemTime::now()`
    /// at builder construction).
    pub fn started_at(mut self, started_at: SystemTime) -> Self {
        self.started_at = started_at;
        self
    }

    /// Attach the active [`SubjectPluginDispatch`] handle.
    pub fn subject_dispatch(mut self, dispatch: Arc<SubjectPluginDispatch>) -> Self {
        self.subject_dispatch = Some(dispatch);
        self
    }

    /// Attach a [`DaemonEventBus`] for `daemon/events` streaming.
    pub fn event_bus(mut self, bus: DaemonEventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Attach a [`DaemonLogBus`] for `daemon/logs` streaming.
    pub fn log_bus(mut self, bus: DaemonLogBus) -> Self {
        self.log_bus = Some(bus);
        self
    }

    /// Attach a [`PluginRouting`] handle so the surface can answer
    /// `plugin/*` calls over the control wire. When absent, all
    /// `plugin/*` methods return [`ControlError::NotSupported`].
    pub fn plugin_routing(mut self, routing: Arc<dyn PluginRouting>) -> Self {
        self.plugin_routing = Some(routing);
        self
    }

    /// Attach a [`DaemonOpsRouting`] handle so `daemon/status`,
    /// `daemon/health`, and `daemon/agents` reflect live process state
    /// instead of the surface's stub responses.
    pub fn daemon_ops_routing(mut self, routing: Arc<dyn DaemonOpsRouting>) -> Self {
        self.daemon_ops_routing = Some(routing);
        self
    }

    /// Attach a [`WorkflowRouting`] handle so the surface can answer
    /// `workflow/*` calls over the control wire. When absent, all
    /// `workflow/*` methods return [`ControlError::NotSupported`].
    pub fn workflow_routing(mut self, routing: Arc<dyn WorkflowRouting>) -> Self {
        self.workflow_routing = Some(routing);
        self
    }

    /// Attach a [`QueueRouting`] handle so the surface can answer
    /// `queue/*` calls over the control wire. When absent, all
    /// `queue/*` methods return [`ControlError::NotSupported`].
    pub fn queue_routing(mut self, routing: Arc<dyn QueueRouting>) -> Self {
        self.queue_routing = Some(routing);
        self
    }

    /// Attach an [`AgentRouting`] handle so the surface can answer
    /// `agent/*` calls over the control wire. When absent, all
    /// `agent/*` methods return [`ControlError::NotSupported`] — which
    /// matches the historical pre-C6.7 stub behavior and lets CLI
    /// callers degrade to the local in-process path.
    pub fn agent_routing(mut self, routing: Arc<dyn AgentRouting>) -> Self {
        self.agent_routing = Some(routing);
        self
    }

    /// Finalize the surface.
    pub fn build(self) -> InProcessSurface {
        InProcessSurface {
            project_root: self.project_root,
            daemon_pid: self.daemon_pid,
            daemon_version: self.daemon_version,
            started_at: self.started_at,
            subject_dispatch: self.subject_dispatch,
            event_bus: self.event_bus,
            log_bus: self.log_bus,
            plugin_routing: self.plugin_routing,
            daemon_ops_routing: self.daemon_ops_routing,
            workflow_routing: self.workflow_routing,
            queue_routing: self.queue_routing,
            agent_routing: self.agent_routing,
        }
    }
}

/// Translate a plugin-protocol [`RpcError`] (from
/// [`SubjectPluginDispatch::route_call`]) into a wire-shape
/// [`ControlError`].
///
/// Method-not-found at the routing layer is the dispatcher saying "no
/// plugin handles this kind" → [`ControlError::NotFound`]. Method-not-
/// supported is the plugin recognizing the verb but not implementing it
/// (e.g. polling-only backends rejecting `subject/watch`) → maps to
/// [`ControlError::NotSupported`]. Everything else lands in
/// [`ControlError::Internal`] with the plugin's message preserved.
fn rpc_error_to_control_error(error: animus_plugin_protocol::RpcError) -> ControlError {
    use animus_plugin_protocol::error_codes;
    match error.code {
        error_codes::METHOD_NOT_FOUND => ControlError::NotFound(error.message),
        error_codes::METHOD_NOT_SUPPORTED => ControlError::NotSupported(error.message),
        error_codes::INVALID_PARAMS => ControlError::InvalidRequest(error.message),
        error_codes::INVALID_REQUEST => ControlError::InvalidRequest(error.message),
        error_codes::TIMEOUT | error_codes::PLUGIN_NOT_INITIALIZED => ControlError::Unavailable(error.message),
        _ => ControlError::Internal(error.message),
    }
}

/// Extract the subject `kind` from an id string of the form
/// `"<backend>:<native_id>"`.
///
/// Subject ids carry an implicit kind prefix that the daemon's in-tree
/// adapters encode (e.g. `"task:TASK-1"`, `"requirement:REQ-1"`). For
/// requests like `subject/get` that carry only the id, this is how we
/// recover the kind needed to route through [`SubjectPluginDispatch`].
fn extract_kind_from_subject_id(id: &str) -> Option<&str> {
    id.split(':').next().filter(|s| !s.is_empty())
}

#[async_trait]
impl ControlSurface for InProcessSurface {
    // ----- Subject ----------------------------------------------------

    async fn subject_list(&self, request: SubjectListRequest) -> Result<SubjectListResponse, ControlError> {
        let kinds = request.filter.kind.clone();
        if kinds.is_empty() {
            return Err(ControlError::InvalidRequest(
                "subject/list requires a kind filter (e.g. filter.kind=['task']); routing without a kind is not supported"
                    .to_string(),
            ));
        }

        let params = json!({"filter": request.filter});
        let mut all_subjects: Vec<WireSubject> = Vec::new();
        let mut last_cursor: Option<String> = None;
        for kind in kinds {
            let raw = self.route_subject_call(&kind, "list", Some(params.clone())).await?;
            let list: SubjectListResponseRaw =
                serde_json::from_value(raw).map_err(|e| ControlError::Internal(format!("subject/list decode: {e}")))?;
            all_subjects.extend(list.subjects);
            last_cursor = list.next_cursor.or(last_cursor);
        }
        Ok(SubjectListResponse { subjects: all_subjects, next_cursor: last_cursor, fetched_at: chrono::Utc::now() })
    }

    async fn subject_get(&self, request: SubjectGetRequest) -> Result<WireSubject, ControlError> {
        let id = request.id.as_str().to_string();
        let kind = extract_kind_from_subject_id(&id)
            .ok_or_else(|| {
                ControlError::InvalidRequest(format!(
                    "subject/get id '{id}' is missing the '<kind>:' prefix; cannot route"
                ))
            })?
            .to_string();
        let raw = self.route_subject_call(&kind, "get", Some(json!({"id": id}))).await?;
        serde_json::from_value(raw).map_err(|e| ControlError::Internal(format!("subject/get decode: {e}")))
    }

    async fn subject_create(&self, request: SubjectCreateRequest) -> Result<WireSubject, ControlError> {
        let kind = request.kind.clone();
        let raw = self
            .route_subject_call(
                &kind,
                "create",
                Some(
                    serde_json::to_value(&request)
                        .map_err(|e| ControlError::Internal(format!("subject/create encode: {e}")))?,
                ),
            )
            .await?;
        serde_json::from_value(raw).map_err(|e| ControlError::Internal(format!("subject/create decode: {e}")))
    }

    async fn subject_update(&self, request: SubjectUpdateRequest) -> Result<WireSubject, ControlError> {
        let id = request.id.as_str().to_string();
        let kind = extract_kind_from_subject_id(&id)
            .ok_or_else(|| {
                ControlError::InvalidRequest(format!("subject/update id '{id}' is missing the '<kind>:' prefix"))
            })?
            .to_string();
        let raw = self.route_subject_call(&kind, "update", Some(json!({"id": id, "patch": request.patch}))).await?;
        serde_json::from_value(raw).map_err(|e| ControlError::Internal(format!("subject/update decode: {e}")))
    }

    async fn subject_next(&self, request: SubjectNextRequest) -> Result<SubjectNextResponse, ControlError> {
        let kind = request.kind.clone().ok_or_else(|| {
            ControlError::InvalidRequest(
                "subject/next requires an explicit kind to route through the matching backend".to_string(),
            )
        })?;
        let raw = self.route_subject_call(&kind, "next", Some(json!({}))).await?;
        // Backends typically return either `null`, `Subject`, or
        // `{"subject": Subject | null}`. Accept both shapes.
        if raw.is_null() {
            return Ok(SubjectNextResponse { subject: None });
        }
        if let Some(inner) = raw.get("subject") {
            let subject: Option<WireSubject> = serde_json::from_value(inner.clone())
                .map_err(|e| ControlError::Internal(format!("subject/next decode: {e}")))?;
            return Ok(SubjectNextResponse { subject });
        }
        let subject: WireSubject =
            serde_json::from_value(raw).map_err(|e| ControlError::Internal(format!("subject/next decode: {e}")))?;
        Ok(SubjectNextResponse { subject: Some(subject) })
    }

    async fn subject_status(&self, request: SubjectStatusRequest) -> Result<WireSubject, ControlError> {
        let id = request.id.as_str().to_string();
        let kind = extract_kind_from_subject_id(&id)
            .ok_or_else(|| {
                ControlError::InvalidRequest(format!("subject/status id '{id}' is missing the '<kind>:' prefix"))
            })?
            .to_string();
        let raw = self.route_subject_call(&kind, "status", Some(json!({"id": id, "status": request.status}))).await?;
        serde_json::from_value(raw).map_err(|e| ControlError::Internal(format!("subject/status decode: {e}")))
    }

    async fn subject_watch(&self, request: SubjectWatchRequest) -> Result<SubjectWatchStream, ControlError> {
        // C5 lands the wire wrapper; the persistent subject/watch stream
        // routing through plugins is out of scope. CLI clients today
        // poll via `subject/list`. Returning an empty stream lets the
        // ack-and-stream path on the connection drive without behavior
        // change.
        let _ = request;
        let stream = futures_util::stream::empty::<SubjectChangedEvent>();
        Ok(Box::pin(stream) as Pin<Box<dyn Stream<Item = SubjectChangedEvent> + Send>>)
    }

    // ----- Plugin -----------------------------------------------------

    async fn plugin_list(&self, request: PluginListRequest) -> Result<PluginListResponse, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_list(request).await,
            None => Err(ControlError::NotSupported("plugin/list routing not configured".to_string())),
        }
    }

    async fn plugin_info(&self, request: PluginInfoRequest) -> Result<PluginInfo, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_info(request).await,
            None => Err(ControlError::NotSupported("plugin/info routing not configured".to_string())),
        }
    }

    async fn plugin_install(&self, request: PluginInstallRequest) -> Result<PluginInstallResponse, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_install(request).await,
            None => Err(ControlError::NotSupported("plugin/install routing not configured".to_string())),
        }
    }

    async fn plugin_uninstall(&self, request: PluginUninstallRequest) -> Result<Unit, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_uninstall(request).await,
            None => Err(ControlError::NotSupported("plugin/uninstall routing not configured".to_string())),
        }
    }

    async fn plugin_ping(&self, request: PluginPingRequest) -> Result<PluginPingResponse, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_ping(request).await,
            None => Err(ControlError::NotSupported("plugin/ping routing not configured".to_string())),
        }
    }

    async fn plugin_call(&self, request: PluginCallRequest) -> Result<PluginCallResponse, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_call(request).await,
            None => Err(ControlError::NotSupported("plugin/call routing not configured".to_string())),
        }
    }

    async fn plugin_search(&self, request: PluginSearchRequest) -> Result<PluginSearchResponse, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_search(request).await,
            None => Err(ControlError::NotSupported("plugin/search routing not configured".to_string())),
        }
    }

    async fn plugin_browse(&self, request: PluginBrowseRequest) -> Result<PluginSearchResponse, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_browse(request).await,
            None => Err(ControlError::NotSupported("plugin/browse routing not configured".to_string())),
        }
    }

    async fn plugin_update(&self, request: PluginUpdateRequest) -> Result<PluginUpdateResponse, ControlError> {
        match &self.plugin_routing {
            Some(routing) => routing.plugin_update(request).await,
            None => Err(ControlError::NotSupported("plugin/update routing not configured".to_string())),
        }
    }

    // ----- Daemon -----------------------------------------------------

    async fn daemon_status(&self) -> Result<DaemonStatusResponse, ControlError> {
        if let Some(routing) = &self.daemon_ops_routing {
            return routing.daemon_status().await;
        }
        let uptime_seconds = self.started_at.elapsed().map(|d| d.as_secs()).unwrap_or(0);
        Ok(DaemonStatusResponse {
            running: true,
            pid: Some(self.daemon_pid),
            uptime_seconds: Some(uptime_seconds),
            version: Some(self.daemon_version.clone()),
            project_root: Some(self.project_root.clone()),
            log_path: None,
        })
    }

    async fn daemon_health(&self) -> Result<DaemonHealthResponse, ControlError> {
        if let Some(routing) = &self.daemon_ops_routing {
            return routing.daemon_health().await;
        }
        // v0.4.9: enumerate installed plugins for the per-plugin section.
        // Each plugin's status is reported as `Healthy` on the basis that
        // it is discoverable / has a valid manifest. A live `health/check`
        // RPC fan-out per plugin is deferred until the daemon owns a
        // long-lived plugin host pool — without it, every health probe
        // here would spawn a one-shot process per plugin, which is
        // prohibitively expensive for a frequently-polled endpoint.
        let plugins = match orchestrator_plugin_host::discover_plugins(&self.project_root) {
            Ok(discovered) => discovered
                .into_iter()
                .map(|p| PluginHealth {
                    name: p.name.clone(),
                    kind: p.manifest.plugin_kind.clone(),
                    status: DaemonHealthStatus::Healthy,
                    uptime_ms: None,
                    last_error: None,
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        Ok(DaemonHealthResponse { status: DaemonHealthStatus::Healthy, plugins, last_error: None })
    }

    async fn daemon_start(&self) -> Result<Unit, ControlError> {
        // The daemon serving this request is already running; start is a
        // no-op per the protocol spec.
        Ok(Unit::default())
    }

    async fn daemon_stop(&self) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("daemon/stop must be issued via the CLI/MCP for now (C6/C7)".to_string()))
    }

    async fn daemon_restart(&self) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("daemon/restart must be issued via the CLI/MCP for now (C6/C7)".to_string()))
    }

    async fn daemon_agents(&self) -> Result<DaemonAgentsResponse, ControlError> {
        if let Some(routing) = &self.daemon_ops_routing {
            return routing.daemon_agents().await;
        }
        // No clean Arc reference to the live AgentPool yet; daemon-side
        // agent tracking lands as part of C7. Returning an empty list
        // matches the historical "no agents currently active" shape.
        Ok(DaemonAgentsResponse { agents: Vec::new() })
    }

    async fn daemon_events(&self, _request: DaemonEventsRequest) -> Result<DaemonEventStream, ControlError> {
        let bus = self
            .event_bus
            .as_ref()
            .ok_or_else(|| ControlError::Unavailable("daemon event bus not initialized".to_string()))?;
        let rx = bus.subscribe();
        let stream = broadcast_to_stream(rx)
            .filter_map(|value| async move { serde_json::from_value::<WireDaemonRunEvent>(value).ok() });
        Ok(Box::pin(stream))
    }

    async fn daemon_logs(&self, request: DaemonLogsRequest) -> Result<DaemonLogStream, ControlError> {
        // v0.4.7: route through the active LogStorageDispatch so the
        // historical tail is wire-driven (CLI `animus logs tail` no longer
        // opens events.jsonl directly when the daemon is up). When a
        // log_storage_backend plugin is installed the dispatch resolution
        // records the plugin identity in its warnings; until a long-lived
        // log-storage plugin host lands we still read the in-tree file
        // for content, but the call goes through the wire so MCP and web
        // surfaces share one transport.
        //
        // Streaming `follow=true` is not supported yet — the stream
        // completes after the historical batch. When the log_bus is
        // attached we tack live notifications onto the tail; otherwise we
        // close after the historical batch.
        let project_root = self.project_root.clone();
        let limit = log_request_limit(&request);
        let level_floor = request.level;
        let plugin_filter = request.plugin.clone();
        let since_filter = request.since;

        let resolution = resolve_log_storage_dispatch(&project_root)
            .map_err(|e| ControlError::Internal(format!("log dispatch resolution: {e}")))?;

        let historical: Vec<DaemonLogEntry> = match resolution.selected.as_ref() {
            LogStorageDispatch::InTree { project_root: pr } | LogStorageDispatch::Plugin { project_root: pr, .. } => {
                read_in_tree_log_entries(pr, limit, level_floor, since_filter, plugin_filter.as_deref())
            }
        };

        let historical_stream = futures_util::stream::iter(historical.into_iter().map(Ok));

        if request.follow {
            if let Some(bus) = self.log_bus.as_ref() {
                let rx = bus.subscribe();
                let live_stream = broadcast_to_stream(rx).map(|value| {
                    serde_json::from_value(value)
                        .map_err(|e| ControlError::Internal(format!("daemon/logs decode: {e}")))
                });
                return Ok(Box::pin(historical_stream.chain(live_stream)));
            }
            // follow=true but no bus: still send historical and close.
        }

        Ok(Box::pin(historical_stream))
    }

    // ----- Workflow ---------------------------------------------------

    async fn workflow_list(&self, request: WorkflowListRequest) -> Result<WorkflowListResponse, ControlError> {
        match &self.workflow_routing {
            Some(routing) => routing.workflow_list(request).await,
            None => Err(ControlError::NotSupported("workflow/list routing not configured".to_string())),
        }
    }

    async fn workflow_get(&self, request: WorkflowGetRequest) -> Result<WorkflowRun, ControlError> {
        match &self.workflow_routing {
            Some(routing) => routing.workflow_get(request).await,
            None => Err(ControlError::NotSupported("workflow/get routing not configured".to_string())),
        }
    }

    async fn workflow_run(&self, request: WorkflowRunRequest) -> Result<WorkflowRunStart, ControlError> {
        match &self.workflow_routing {
            Some(routing) => routing.workflow_run(request).await,
            None => Err(ControlError::NotSupported("workflow/run routing not configured".to_string())),
        }
    }

    async fn workflow_execute(&self, request: WorkflowExecuteRequest) -> Result<WorkflowRunStart, ControlError> {
        match &self.workflow_routing {
            Some(routing) => routing.workflow_execute(request).await,
            None => Err(ControlError::NotSupported("workflow/execute routing not configured".to_string())),
        }
    }

    async fn workflow_pause(&self, request: WorkflowPauseRequest) -> Result<Unit, ControlError> {
        match &self.workflow_routing {
            Some(routing) => routing.workflow_pause(request).await,
            None => Err(ControlError::NotSupported("workflow/pause routing not configured".to_string())),
        }
    }

    async fn workflow_resume(&self, request: WorkflowResumeRequest) -> Result<Unit, ControlError> {
        match &self.workflow_routing {
            Some(routing) => routing.workflow_resume(request).await,
            None => Err(ControlError::NotSupported("workflow/resume routing not configured".to_string())),
        }
    }

    async fn workflow_cancel(&self, request: WorkflowCancelRequest) -> Result<Unit, ControlError> {
        match &self.workflow_routing {
            Some(routing) => routing.workflow_cancel(request).await,
            None => Err(ControlError::NotSupported("workflow/cancel routing not configured".to_string())),
        }
    }

    // ----- Agent ------------------------------------------------------

    async fn agent_run(&self, request: AgentRunRequest) -> Result<AgentRunResult, ControlError> {
        match &self.agent_routing {
            Some(routing) => routing.agent_run(request).await,
            None => Err(ControlError::NotSupported("agent/run routing not configured".to_string())),
        }
    }

    async fn agent_status(&self, request: AgentStatusRequest) -> Result<AgentStatus, ControlError> {
        match &self.agent_routing {
            Some(routing) => routing.agent_status(request).await,
            None => Err(ControlError::NotSupported("agent/status routing not configured".to_string())),
        }
    }

    async fn agent_cancel(&self, request: AgentCancelRequest) -> Result<Unit, ControlError> {
        match &self.agent_routing {
            Some(routing) => routing.agent_cancel(request).await,
            None => Err(ControlError::NotSupported("agent/cancel routing not configured".to_string())),
        }
    }

    // ----- Queue ------------------------------------------------------

    async fn queue_list(&self, request: QueueListRequest) -> Result<QueueListResponse, ControlError> {
        match &self.queue_routing {
            Some(routing) => routing.queue_list(request).await,
            None => Err(ControlError::NotSupported("queue/list routing not configured".to_string())),
        }
    }

    async fn queue_enqueue(&self, request: QueueEnqueueRequest) -> Result<QueueEntry, ControlError> {
        match &self.queue_routing {
            Some(routing) => routing.queue_enqueue(request).await,
            None => Err(ControlError::NotSupported("queue/enqueue routing not configured".to_string())),
        }
    }

    async fn queue_drop(&self, request: QueueDropRequest) -> Result<Unit, ControlError> {
        match &self.queue_routing {
            Some(routing) => routing.queue_drop(request).await,
            None => Err(ControlError::NotSupported("queue/drop routing not configured".to_string())),
        }
    }

    async fn queue_hold(&self, request: QueueHoldRequest) -> Result<Unit, ControlError> {
        match &self.queue_routing {
            Some(routing) => routing.queue_hold(request).await,
            None => Err(ControlError::NotSupported("queue/hold routing not configured".to_string())),
        }
    }

    async fn queue_release(&self, request: QueueReleaseRequest) -> Result<Unit, ControlError> {
        match &self.queue_routing {
            Some(routing) => routing.queue_release(request).await,
            None => Err(ControlError::NotSupported("queue/release routing not configured".to_string())),
        }
    }

    async fn queue_reorder(&self, request: QueueReorderRequest) -> Result<Unit, ControlError> {
        match &self.queue_routing {
            Some(routing) => routing.queue_reorder(request).await,
            None => Err(ControlError::NotSupported("queue/reorder routing not configured".to_string())),
        }
    }

    async fn queue_stats(&self) -> Result<QueueStats, ControlError> {
        match &self.queue_routing {
            Some(routing) => routing.queue_stats().await,
            None => Err(ControlError::NotSupported("queue/stats routing not configured".to_string())),
        }
    }

    // ----- Project ----------------------------------------------------

    async fn project_init(&self, _request: ProjectInitRequest) -> Result<ProjectInfo, ControlError> {
        Err(ControlError::NotSupported("project/init will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn project_setup(&self, _request: ProjectSetupRequest) -> Result<ProjectInfo, ControlError> {
        Err(ControlError::NotSupported("project/setup will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn project_status(&self) -> Result<ProjectStatusResponse, ControlError> {
        Err(ControlError::NotSupported("project/status will be wired in C6 (CLI cutover)".to_string()))
    }
}

/// Permissive intermediate shape for `subject/list` plugin responses.
///
/// Plugin backends generally return `{"subjects": [...], "next_cursor": "..."}`
/// without the `fetched_at` field the control protocol adds. We add the
/// timestamp on the daemon side and re-pack into [`SubjectListResponse`].
#[derive(serde::Deserialize)]
struct SubjectListResponseRaw {
    #[serde(default)]
    subjects: Vec<WireSubject>,
    #[serde(default)]
    next_cursor: Option<String>,
}

use futures_util::StreamExt;

/// Adapt a [`tokio::sync::broadcast::Receiver`] into a `Stream`.
///
/// Slow subscribers may surface `Lagged` errors when the broadcast buffer
/// overflows. We drop the lagged batch and resume; the control protocol
/// has no in-band "you skipped events" signal yet, so a brief gap is the
/// best we can do.
fn broadcast_to_stream(rx: broadcast::Receiver<Value>) -> impl Stream<Item = Value> + Send + 'static {
    tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|res| async move { res.ok() })
}

/// Default cap on historical entries served by `daemon/logs`.
///
/// The wire request shape lacks a `limit` field, so we cap server-side to
/// keep a single CLI invocation bounded. CLI callers stream the historical
/// batch and stop after they've collected the limit they want.
const DAEMON_LOGS_DEFAULT_LIMIT: usize = 200;

/// Pluck the effective per-call limit from a [`DaemonLogsRequest`]. Today
/// the wire request has no explicit limit; reserve the function so a
/// future protocol bump that adds one needs to update exactly one place.
fn log_request_limit(_request: &DaemonLogsRequest) -> usize {
    DAEMON_LOGS_DEFAULT_LIMIT
}

/// Read historical entries from the project's in-tree `events.jsonl` and
/// convert each row from [`orchestrator_logging::LogEntry`] into the
/// protocol [`DaemonLogEntry`] shape. Filters mirror
/// [`DaemonLogsRequest`].
///
/// Returns entries in chronological order (oldest first) so the streamed
/// view matches how a human reads a tail. Plugin-backed dispatch falls
/// through to the same file today because the long-lived log-storage
/// plugin host is deferred — the file is the source of truth until a
/// supervisor exists to relay `log_storage/tail` against the plugin.
fn read_in_tree_log_entries(
    project_root: &std::path::Path,
    limit: usize,
    level_floor: Option<animus_log_storage_protocol::LogLevel>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    plugin_filter: Option<&str>,
) -> Vec<DaemonLogEntry> {
    use orchestrator_logging::Logger;
    let logger = Logger::for_project(project_root);
    // Map the wire level floor onto the in-tree Level enum. The wire has
    // a Trace level that the in-tree reader does not — Trace requests
    // degrade to Debug (the lowest in-tree level).
    let level_filter = level_floor.map(wire_level_to_local);
    let since_str = since.map(|ts| ts.to_rfc3339());

    // Overscan and trim, matching the CLI reader's strategy so the
    // returned set is the most recent `limit` matches even when many
    // entries fail the plugin filter.
    let pool = limit.saturating_mul(4).max(limit);
    let raw = logger.read_entries_since(pool, None, level_filter, since_str.as_deref());

    let mut filtered: Vec<orchestrator_logging::LogEntry> =
        raw.into_iter().filter(|entry| plugin_matches(entry, plugin_filter)).collect();
    if filtered.len() > limit {
        let drop = filtered.len() - limit;
        filtered.drain(0..drop);
    }

    filtered.into_iter().map(local_entry_to_wire).collect()
}

fn plugin_matches(entry: &orchestrator_logging::LogEntry, plugin_filter: Option<&str>) -> bool {
    let Some(name) = plugin_filter else {
        return true;
    };
    if entry.provider.as_deref() == Some(name) {
        return true;
    }
    entry.meta.as_ref().and_then(|v| v.get("plugin")).and_then(|v| v.as_str()).map(|p| p == name).unwrap_or(false)
}

fn wire_level_to_local(level: animus_log_storage_protocol::LogLevel) -> orchestrator_logging::Level {
    use animus_log_storage_protocol::LogLevel;
    match level {
        // The in-tree reader does not distinguish Trace from Debug; map
        // Trace down to Debug so a `--level=trace` request still returns
        // every Debug entry.
        LogLevel::Trace | LogLevel::Debug => orchestrator_logging::Level::Debug,
        LogLevel::Info => orchestrator_logging::Level::Info,
        LogLevel::Warn => orchestrator_logging::Level::Warn,
        LogLevel::Error => orchestrator_logging::Level::Error,
    }
}

fn local_level_to_wire(level: orchestrator_logging::Level) -> animus_log_storage_protocol::LogLevel {
    use animus_log_storage_protocol::LogLevel;
    match level {
        orchestrator_logging::Level::Debug => LogLevel::Debug,
        orchestrator_logging::Level::Info => LogLevel::Info,
        orchestrator_logging::Level::Warn => LogLevel::Warn,
        orchestrator_logging::Level::Error => LogLevel::Error,
    }
}

fn local_entry_to_wire(entry: orchestrator_logging::LogEntry) -> DaemonLogEntry {
    use animus_log_storage_protocol::LogSource;
    use chrono::DateTime;

    let ts = DateTime::parse_from_rfc3339(&entry.ts)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    // `provider` carries the plugin name in the in-tree schema, so a
    // provider-tagged entry classifies as Plugin source with the plugin
    // name in `source_name`.
    let (source, source_name) = match entry.provider.as_ref() {
        Some(name) => (LogSource::Plugin, Some(name.clone())),
        None => (LogSource::Daemon, None),
    };

    // Synthesize a stable id from (ts, cat, msg) so the stream consumer
    // can dedupe across overlapping historical+follow batches. UUIDs
    // would also work; the v0.4.7 stream is short-lived enough that the
    // hash is sufficient.
    let id = format!("{}|{}|{}", entry.ts, entry.cat, short_hash(&entry.msg));

    let fields = serde_json::to_value(&entry).unwrap_or(serde_json::Value::Null);

    DaemonLogEntry {
        id,
        ts,
        level: local_level_to_wire(entry.level),
        source,
        source_name,
        target: entry.cat,
        message: entry.msg,
        fields,
    }
}

fn short_hash(msg: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    msg.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod log_dispatch_tests {
    //! v0.4.7 Item 1: prove the daemon's `daemon/logs` historical reader
    //! projects in-tree `events.jsonl` rows into the protocol's
    //! `DaemonLogEntry` shape with level + plugin filtering preserved.

    use super::*;
    use animus_log_storage_protocol::{LogLevel, LogSource};
    use orchestrator_logging::{Level, Logger};
    use tempfile::TempDir;

    fn fixture_logger(temp: &TempDir) -> Logger {
        let logs_dir = temp.path().join(".animus/logs");
        std::fs::create_dir_all(&logs_dir).expect("mkdir");
        Logger::open(&logs_dir, "events.jsonl", Level::Debug)
    }

    #[test]
    fn read_in_tree_log_entries_projects_into_wire_shape() {
        let temp = TempDir::new().expect("tempdir");
        let logger = fixture_logger(&temp);
        logger.info("test", "info-line").emit();
        logger.warn("test", "warn-line").provider("kimi-code").emit();
        logger.error("test", "error-line").emit();

        let entries = read_in_tree_log_entries(temp.path(), 10, Some(LogLevel::Warn), None, None);
        assert_eq!(entries.len(), 2, "expected warn+error, got {entries:?}");
        // Chronological order: warn first, then error.
        assert_eq!(entries[0].message, "warn-line");
        assert_eq!(entries[0].level, LogLevel::Warn);
        assert_eq!(entries[0].source, LogSource::Plugin);
        assert_eq!(entries[0].source_name.as_deref(), Some("kimi-code"));
        assert_eq!(entries[1].message, "error-line");
        assert_eq!(entries[1].level, LogLevel::Error);
        assert_eq!(entries[1].source, LogSource::Daemon);
    }

    #[test]
    fn read_in_tree_log_entries_plugin_filter_narrows_set() {
        let temp = TempDir::new().expect("tempdir");
        let logger = fixture_logger(&temp);
        logger.warn("test", "no-plugin").emit();
        logger.warn("test", "kimi").provider("kimi-code").emit();
        logger.warn("test", "gpt").provider("gpt-code").emit();

        let entries = read_in_tree_log_entries(temp.path(), 10, Some(LogLevel::Warn), None, Some("kimi-code"));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "kimi");
    }

    #[test]
    fn read_in_tree_log_entries_synthesizes_unique_ids_per_row() {
        let temp = TempDir::new().expect("tempdir");
        let logger = fixture_logger(&temp);
        logger.info("test", "msg-a").emit();
        logger.info("test", "msg-b").emit();
        let entries = read_in_tree_log_entries(temp.path(), 10, Some(LogLevel::Info), None, None);
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].id, entries[1].id, "ids must disambiguate rows");
    }
}
