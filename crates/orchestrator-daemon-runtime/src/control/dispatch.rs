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
        DaemonEventsRequest, DaemonHealthResponse, DaemonHealthStatus, DaemonLogsRequest,
        DaemonRunEvent as WireDaemonRunEvent, DaemonStatusResponse, PluginBrowseRequest, PluginCallRequest,
        PluginCallResponse, PluginInfo, PluginInfoRequest, PluginInstallRequest, PluginInstallResponse,
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

use crate::subject_dispatch::SubjectPluginDispatch;

use super::routing::{DaemonOpsRouting, PluginRouting};
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
        Ok(DaemonHealthResponse { status: DaemonHealthStatus::Healthy, plugins: Vec::new(), last_error: None })
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

    async fn daemon_logs(&self, _request: DaemonLogsRequest) -> Result<DaemonLogStream, ControlError> {
        let bus = self
            .log_bus
            .as_ref()
            .ok_or_else(|| ControlError::Unavailable("daemon log bus not initialized".to_string()))?;
        let rx = bus.subscribe();
        let stream = broadcast_to_stream(rx).map(|value| {
            serde_json::from_value(value).map_err(|e| ControlError::Internal(format!("daemon/logs decode: {e}")))
        });
        Ok(Box::pin(stream))
    }

    // ----- Workflow ---------------------------------------------------

    async fn workflow_list(&self, _request: WorkflowListRequest) -> Result<WorkflowListResponse, ControlError> {
        Err(ControlError::NotSupported("workflow/list will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn workflow_get(&self, _request: WorkflowGetRequest) -> Result<WorkflowRun, ControlError> {
        Err(ControlError::NotSupported(
            "workflow/get will be wired in C6; detail will pass through as opaque Value".to_string(),
        ))
    }

    async fn workflow_run(&self, _request: WorkflowRunRequest) -> Result<WorkflowRunStart, ControlError> {
        Err(ControlError::NotSupported("workflow/run will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn workflow_execute(&self, _request: WorkflowExecuteRequest) -> Result<WorkflowRunStart, ControlError> {
        Err(ControlError::NotSupported("workflow/execute will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn workflow_pause(&self, _request: WorkflowPauseRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("workflow/pause will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn workflow_resume(&self, _request: WorkflowResumeRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("workflow/resume will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn workflow_cancel(&self, _request: WorkflowCancelRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("workflow/cancel will be wired in C6 (CLI cutover)".to_string()))
    }

    // ----- Agent ------------------------------------------------------

    async fn agent_run(&self, _request: AgentRunRequest) -> Result<AgentRunResult, ControlError> {
        Err(ControlError::NotSupported("agent/run will be wired in C7 (MCP cutover)".to_string()))
    }

    async fn agent_status(&self, _request: AgentStatusRequest) -> Result<AgentStatus, ControlError> {
        Err(ControlError::NotSupported("agent/status will be wired in C7 (MCP cutover)".to_string()))
    }

    async fn agent_cancel(&self, _request: AgentCancelRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("agent/cancel will be wired in C7 (MCP cutover)".to_string()))
    }

    // ----- Queue ------------------------------------------------------

    async fn queue_list(&self, _request: QueueListRequest) -> Result<QueueListResponse, ControlError> {
        Err(ControlError::NotSupported("queue/list will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn queue_enqueue(&self, _request: QueueEnqueueRequest) -> Result<QueueEntry, ControlError> {
        Err(ControlError::NotSupported("queue/enqueue will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn queue_drop(&self, _request: QueueDropRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("queue/drop will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn queue_hold(&self, _request: QueueHoldRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("queue/hold will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn queue_release(&self, _request: QueueReleaseRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("queue/release will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn queue_reorder(&self, _request: QueueReorderRequest) -> Result<Unit, ControlError> {
        Err(ControlError::NotSupported("queue/reorder will be wired in C6 (CLI cutover)".to_string()))
    }

    async fn queue_stats(&self) -> Result<QueueStats, ControlError> {
        Err(ControlError::NotSupported("queue/stats will be wired in C6 (CLI cutover)".to_string()))
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
