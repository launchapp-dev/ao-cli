//! [`ControlConnection`] — per-client JSON-RPC dispatch loop.
//!
//! Owns a single [`tokio::net::UnixStream`]. Reads newline-delimited
//! JSON-RPC 2.0 frames, dispatches each via the shared
//! [`Arc<dyn ControlSurface>`], and writes the response back. Streaming
//! methods (`subject/watch`, `daemon/events`, `daemon/logs`) emit an
//! immediate `{"watching": true}` ack and then push subsequent items as
//! JSON-RPC notifications on the same socket.
//!
//! Per-connection state tracks active stream-driver tasks keyed by the
//! originating request id. On client disconnect every driver task is
//! aborted so a backend that keeps producing events doesn't leak forever.
//!
//! Anti-deadlock rules:
//!
//! - Connection state is per-task; no shared mutex across connections.
//! - Writes go through an `Arc<Mutex<WriteHalf>>` (acquired briefly to
//!   serialize a single frame, never held across `.await` aside from
//!   the write itself).
//! - Stream drivers own `JoinHandle`s and are aborted on connection
//!   teardown via `Drop` of [`ConnectionWriter`].

use std::collections::HashMap;
use std::sync::Arc;

use animus_control_protocol::{
    control_trait::ControlSurface,
    method as method_names,
    types::{
        AgentCancelRequest, AgentRunRequest, AgentStatusRequest, DaemonEventsRequest, DaemonLogsRequest,
        PluginBrowseRequest, PluginCallRequest, PluginInfoRequest, PluginInstallRequest, PluginListRequest,
        PluginPingRequest, PluginSearchRequest, PluginUninstallRequest, PluginUpdateRequest, ProjectInitRequest,
        ProjectSetupRequest, QueueDropRequest, QueueEnqueueRequest, QueueHoldRequest, QueueListRequest,
        QueueReleaseRequest, QueueReorderRequest, SubjectCreateRequest, SubjectGetRequest, SubjectListRequest,
        SubjectNextRequest, SubjectStatusRequest, SubjectUpdateRequest, SubjectWatchRequest, WorkflowCancelRequest,
        WorkflowExecuteRequest, WorkflowGetRequest, WorkflowListRequest, WorkflowPauseRequest, WorkflowResumeRequest,
        WorkflowRunRequest,
    },
    ControlError,
};
use animus_plugin_protocol::{error_codes, RpcError, RpcRequest, RpcResponse};
use futures_util::StreamExt;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Per-client JSON-RPC dispatch loop.
///
/// Hold one per accepted [`UnixStream`] and drive [`ControlConnection::serve`]
/// to completion. Returns `Ok(())` on clean client disconnect; returns
/// `Err` only on unrecoverable IO failures (the accept loop logs and moves
/// on either way).
pub struct ControlConnection {
    stream: UnixStream,
    surface: Arc<dyn ControlSurface>,
}

impl ControlConnection {
    /// Build a new connection bound to `stream`, dispatching via
    /// `surface`.
    pub fn new(stream: UnixStream, surface: Arc<dyn ControlSurface>) -> Self {
        Self { stream, surface }
    }

    /// Run the read-dispatch-write loop until the client disconnects or
    /// an unrecoverable IO error occurs.
    ///
    /// Each inbound frame is parsed as a JSON-RPC 2.0 request. Parse
    /// failures emit a `parse_error` response carrying `id: null` per
    /// the spec. Method-dispatch failures surface as proper
    /// [`RpcResponse`] error frames with the original request id echoed.
    /// Streaming methods return an immediate `{"watching": true}` ack
    /// and then push notifications on the same connection until the
    /// client disconnects or the stream completes.
    pub async fn serve(self) -> std::io::Result<()> {
        let (read_half, write_half) = self.stream.into_split();
        let mut reader = BufReader::new(read_half);
        let writer = Arc::new(ConnectionWriter::new(write_half));

        let mut line = String::new();
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await?;
            if n == 0 {
                // Client closed the connection.
                return Ok(());
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let frame = match serde_json::from_str::<RpcRequest>(trimmed) {
                Ok(req) => req,
                Err(err) => {
                    let response = RpcResponse::err(
                        None,
                        RpcError { code: error_codes::PARSE_ERROR, message: format!("parse error: {err}"), data: None },
                    );
                    writer.write_frame(&response).await?;
                    continue;
                }
            };

            let surface = Arc::clone(&self.surface);
            let writer_clone = Arc::clone(&writer);
            // Dispatch on the current task. Each method awaits its
            // underlying service call serially — JSON-RPC clients are
            // free to open multiple connections for concurrency.
            // Streaming methods spawn detached driver tasks before
            // returning their ack, so they don't block the loop.
            dispatch_request(&surface, &writer_clone, frame).await?;
        }
    }
}

/// Shared write end with serialized access.
///
/// Multiple stream-driver tasks may produce notifications on the same
/// connection concurrently; the mutex guarantees each frame writes
/// atomically (no interleaved JSON). The mutex is held only across the
/// individual write — never across an `.await` on the surface.
struct ConnectionWriter {
    write_half: Mutex<tokio::net::unix::OwnedWriteHalf>,
    drivers: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl ConnectionWriter {
    fn new(write_half: tokio::net::unix::OwnedWriteHalf) -> Self {
        Self { write_half: Mutex::new(write_half), drivers: Mutex::new(HashMap::new()) }
    }

    async fn write_frame<T: serde::Serialize>(&self, frame: &T) -> std::io::Result<()> {
        let mut bytes = serde_json::to_vec(frame).map_err(std::io::Error::other)?;
        bytes.push(b'\n');
        let mut guard = self.write_half.lock().await;
        guard.write_all(&bytes).await?;
        guard.flush().await?;
        Ok(())
    }

    async fn register_driver(&self, id: String, handle: JoinHandle<()>) {
        let mut guard = self.drivers.lock().await;
        if let Some(previous) = guard.insert(id, handle) {
            previous.abort();
        }
    }
}

impl Drop for ConnectionWriter {
    fn drop(&mut self) {
        // `try_lock` cannot fail when we own the only handle (the
        // owning task is being dropped).
        if let Ok(mut guard) = self.drivers.try_lock() {
            for (_id, handle) in guard.drain() {
                handle.abort();
            }
        }
    }
}

/// Dispatch a single inbound request to the surface.
///
/// All replies write through `writer`; the function returns
/// `Err(io::Error)` only when the write itself fails. Surface errors
/// are turned into [`RpcResponse::err`] frames and surfaced normally.
async fn dispatch_request(
    surface: &Arc<dyn ControlSurface>,
    writer: &Arc<ConnectionWriter>,
    frame: RpcRequest,
) -> std::io::Result<()> {
    let id = frame.id.clone();
    let result = invoke_surface(surface, writer, &frame).await;
    match result {
        Ok(Some(value)) => {
            let response = RpcResponse::ok(id, value);
            writer.write_frame(&response).await
        }
        Ok(None) => {
            // The handler already wrote its ack frame (streaming) — no
            // additional response from this call.
            Ok(())
        }
        Err(err) => {
            let response = RpcResponse::err(id, err);
            writer.write_frame(&response).await
        }
    }
}

/// Invoke the surface method matching `frame.method`.
///
/// Returns `Ok(Some(value))` for unary methods (caller writes the
/// JSON-RPC ok response). Returns `Ok(None)` for streaming methods that
/// emitted their own ack and spawned a driver task. Returns `Err` with
/// a populated [`RpcError`] for any failure surface side.
#[allow(clippy::too_many_lines)]
async fn invoke_surface(
    surface: &Arc<dyn ControlSurface>,
    writer: &Arc<ConnectionWriter>,
    frame: &RpcRequest,
) -> Result<Option<Value>, RpcError> {
    let params = frame.params.clone().unwrap_or(Value::Null);
    match frame.method.as_str() {
        // ----- Subject -----------------------------------------------
        method_names::METHOD_SUBJECT_LIST => {
            let req: SubjectListRequest = parse_params(params)?;
            surface.subject_list(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_SUBJECT_GET => {
            let req: SubjectGetRequest = parse_params(params)?;
            surface.subject_get(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_SUBJECT_CREATE => {
            let req: SubjectCreateRequest = parse_params(params)?;
            surface.subject_create(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_SUBJECT_UPDATE => {
            let req: SubjectUpdateRequest = parse_params(params)?;
            surface.subject_update(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_SUBJECT_NEXT => {
            let req: SubjectNextRequest = parse_params(params)?;
            surface.subject_next(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_SUBJECT_STATUS => {
            let req: SubjectStatusRequest = parse_params(params)?;
            surface.subject_status(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_SUBJECT_WATCH => {
            let req: SubjectWatchRequest = parse_params(params)?;
            let stream = surface.subject_watch(req).await.map_err(rpc_from_control)?;
            spawn_stream_driver(writer, frame.id.clone(), method_names::NOTIFICATION_SUBJECT_CHANGED, stream).await?;
            Ok(Some(ack_value()))
        }

        // ----- Plugin ------------------------------------------------
        method_names::METHOD_PLUGIN_LIST => {
            let req: PluginListRequest = parse_params(params)?;
            surface.plugin_list(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_INFO => {
            let req: PluginInfoRequest = parse_params(params)?;
            surface.plugin_info(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_INSTALL => {
            let req: PluginInstallRequest = parse_params(params)?;
            surface.plugin_install(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_UNINSTALL => {
            let req: PluginUninstallRequest = parse_params(params)?;
            surface.plugin_uninstall(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_PING => {
            let req: PluginPingRequest = parse_params(params)?;
            surface.plugin_ping(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_CALL => {
            let req: PluginCallRequest = parse_params(params)?;
            surface.plugin_call(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_SEARCH => {
            let req: PluginSearchRequest = parse_params(params)?;
            surface.plugin_search(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_BROWSE => {
            let req: PluginBrowseRequest = parse_params(params)?;
            surface.plugin_browse(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PLUGIN_UPDATE => {
            let req: PluginUpdateRequest = parse_params(params)?;
            surface.plugin_update(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }

        // ----- Daemon ------------------------------------------------
        method_names::METHOD_DAEMON_STATUS => {
            surface.daemon_status().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_DAEMON_HEALTH => {
            surface.daemon_health().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_DAEMON_START => {
            surface.daemon_start().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_DAEMON_STOP => {
            surface.daemon_stop().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_DAEMON_RESTART => {
            surface.daemon_restart().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_DAEMON_AGENTS => {
            surface.daemon_agents().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_DAEMON_EVENTS => {
            let req: DaemonEventsRequest = parse_params(params)?;
            let stream = surface.daemon_events(req).await.map_err(rpc_from_control)?;
            spawn_stream_driver(writer, frame.id.clone(), method_names::NOTIFICATION_DAEMON_EVENT, stream).await?;
            Ok(Some(ack_value()))
        }
        method_names::METHOD_DAEMON_LOGS => {
            let req: DaemonLogsRequest = parse_params(params)?;
            let stream = surface.daemon_logs(req).await.map_err(rpc_from_control)?;
            spawn_fallible_stream_driver(writer, frame.id.clone(), method_names::NOTIFICATION_DAEMON_LOG, stream)
                .await?;
            Ok(Some(ack_value()))
        }

        // ----- Workflow ----------------------------------------------
        method_names::METHOD_WORKFLOW_LIST => {
            let req: WorkflowListRequest = parse_params(params)?;
            surface.workflow_list(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_WORKFLOW_GET => {
            let req: WorkflowGetRequest = parse_params(params)?;
            surface.workflow_get(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_WORKFLOW_RUN => {
            let req: WorkflowRunRequest = parse_params(params)?;
            surface.workflow_run(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_WORKFLOW_EXECUTE => {
            let req: WorkflowExecuteRequest = parse_params(params)?;
            surface.workflow_execute(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_WORKFLOW_PAUSE => {
            let req: WorkflowPauseRequest = parse_params(params)?;
            surface.workflow_pause(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_WORKFLOW_RESUME => {
            let req: WorkflowResumeRequest = parse_params(params)?;
            surface.workflow_resume(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_WORKFLOW_CANCEL => {
            let req: WorkflowCancelRequest = parse_params(params)?;
            surface.workflow_cancel(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }

        // ----- Agent -------------------------------------------------
        method_names::METHOD_AGENT_RUN => {
            let req: AgentRunRequest = parse_params(params)?;
            surface.agent_run(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_AGENT_STATUS => {
            let req: AgentStatusRequest = parse_params(params)?;
            surface.agent_status(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_AGENT_CANCEL => {
            let req: AgentCancelRequest = parse_params(params)?;
            surface.agent_cancel(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }

        // ----- Queue -------------------------------------------------
        method_names::METHOD_QUEUE_LIST => {
            let req: QueueListRequest = parse_params(params)?;
            surface.queue_list(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_QUEUE_ENQUEUE => {
            let req: QueueEnqueueRequest = parse_params(params)?;
            surface.queue_enqueue(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_QUEUE_DROP => {
            let req: QueueDropRequest = parse_params(params)?;
            surface.queue_drop(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_QUEUE_HOLD => {
            let req: QueueHoldRequest = parse_params(params)?;
            surface.queue_hold(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_QUEUE_RELEASE => {
            let req: QueueReleaseRequest = parse_params(params)?;
            surface.queue_release(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_QUEUE_REORDER => {
            let req: QueueReorderRequest = parse_params(params)?;
            surface.queue_reorder(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_QUEUE_STATS => {
            surface.queue_stats().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }

        // ----- Project -----------------------------------------------
        method_names::METHOD_PROJECT_INIT => {
            let req: ProjectInitRequest = parse_params(params)?;
            surface.project_init(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PROJECT_SETUP => {
            let req: ProjectSetupRequest = parse_params(params)?;
            surface.project_setup(req).await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }
        method_names::METHOD_PROJECT_STATUS => {
            surface.project_status().await.map(|r| Some(serialize_result(r))).map_err(rpc_from_control)
        }

        unknown => Err(RpcError {
            code: error_codes::METHOD_NOT_FOUND,
            message: format!("unknown control method: {unknown}"),
            data: None,
        }),
    }
}

/// Deserialize JSON-RPC params into the typed request shape, mapping
/// failures to the `INVALID_PARAMS` JSON-RPC code.
fn parse_params<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    // Permissive: accept `null` as the default-constructed request when
    // the target type is `Default`. This requires deserializing through
    // `{}` to feed serde's default-field handling.
    let canonical = if params.is_null() { Value::Object(serde_json::Map::new()) } else { params };
    serde_json::from_value(canonical).map_err(|err| RpcError {
        code: error_codes::INVALID_PARAMS,
        message: format!("invalid params: {err}"),
        data: None,
    })
}

/// Serialize a typed response into JSON. Unwraps the `serde_json::to_value`
/// error because the protocol response types are all `Serialize` —
/// failures here would indicate a real bug, not a runtime input.
fn serialize_result<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or_else(|err| {
        serde_json::json!({
            "_serialize_error": err.to_string(),
        })
    })
}

/// Standard `{"watching": true}` ack returned for streaming methods.
fn ack_value() -> Value {
    serde_json::json!({"watching": true})
}

/// Convert a wire-shape [`ControlError`] into the JSON-RPC error frame.
///
/// The control-protocol crate (v0.1.3 from git) ships its own
/// `ControlError → RpcError` `From` impl, but that impl targets the
/// v0.1.3 plugin-protocol `RpcError`. The daemon-runtime crate uses the
/// in-tree v0.1.0 `RpcError`. Re-implement the mapping verbatim against
/// the in-tree codes so both sides share the same JSON-RPC wire shape.
fn rpc_from_control(err: ControlError) -> RpcError {
    let (code, category) = match &err {
        ControlError::NotFound(_) => (error_codes::INVALID_PARAMS, "not_found"),
        ControlError::InvalidRequest(_) => (error_codes::INVALID_PARAMS, "invalid_request"),
        ControlError::PermissionDenied(_) => (error_codes::INVALID_REQUEST, "permission_denied"),
        ControlError::Unavailable(_) => (error_codes::INTERNAL_ERROR, "unavailable"),
        ControlError::NotSupported(_) => (error_codes::METHOD_NOT_SUPPORTED, "not_supported"),
        ControlError::Conflict(_) => (error_codes::INVALID_REQUEST, "conflict"),
        ControlError::Internal(_) => (error_codes::INTERNAL_ERROR, "internal"),
    };
    RpcError { code, message: err.to_string(), data: Some(serde_json::json!({ "category": category })) }
}

/// Spawn a driver task that drains a stream of `T: Serialize` items and
/// emits each as a JSON-RPC notification under `notification_method`.
///
/// The notification params carry `{"id": <request_id>, "data": <T>}` so
/// clients can correlate notifications back to the originating request.
/// The driver registers itself with the [`ConnectionWriter`] keyed by
/// the stringified request id; client disconnect aborts it via the
/// writer's `Drop` impl.
async fn spawn_stream_driver<T, S>(
    writer: &Arc<ConnectionWriter>,
    request_id: Option<Value>,
    notification_method: &'static str,
    mut stream: S,
) -> Result<(), RpcError>
where
    T: serde::Serialize + Send + 'static,
    S: futures_core::Stream<Item = T> + Send + Unpin + 'static,
{
    let driver_key = stringify_id(&request_id);
    let writer_clone = Arc::clone(writer);
    let request_id_inner = request_id.clone();
    let handle = tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            let payload = serde_json::json!({
                "id": request_id_inner,
                "data": item,
            });
            let notification =
                animus_plugin_protocol::RpcNotification::new(notification_method.to_string(), Some(payload));
            if writer_clone.write_frame(&notification).await.is_err() {
                // Client disconnected; stop pushing.
                break;
            }
        }
    });
    writer.register_driver(driver_key, handle).await;
    Ok(())
}

/// Variant for streams of `Result<T, ControlError>` (used by `daemon/logs`).
///
/// Successful items become normal notifications. `Err` items terminate
/// the stream cleanly — the client sees the stream end without an
/// in-band error notification, matching the upstream
/// [`DaemonLogStream`](animus_control_protocol::control_trait::DaemonLogStream)
/// contract that closes on a hard failure.
async fn spawn_fallible_stream_driver<T, S>(
    writer: &Arc<ConnectionWriter>,
    request_id: Option<Value>,
    notification_method: &'static str,
    mut stream: S,
) -> Result<(), RpcError>
where
    T: serde::Serialize + Send + 'static,
    S: futures_core::Stream<Item = Result<T, ControlError>> + Send + Unpin + 'static,
{
    let driver_key = stringify_id(&request_id);
    let writer_clone = Arc::clone(writer);
    let request_id_inner = request_id.clone();
    let handle = tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match item {
                Ok(item) => {
                    let payload = serde_json::json!({
                        "id": request_id_inner,
                        "data": item,
                    });
                    let notification =
                        animus_plugin_protocol::RpcNotification::new(notification_method.to_string(), Some(payload));
                    if writer_clone.write_frame(&notification).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    writer.register_driver(driver_key, handle).await;
    Ok(())
}

/// Convert a JSON-RPC id into a stable string for the driver-task map key.
fn stringify_id(id: &Option<Value>) -> String {
    match id {
        None => "<notif>".to_string(),
        Some(v) => v.to_string(),
    }
}
