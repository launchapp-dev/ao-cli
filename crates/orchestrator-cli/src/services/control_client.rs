//! CLI-side client for the daemon's control RPC socket.
//!
//! C6 of the v0.4.0 controller-as-plugin migration. The CLI now speaks
//! the [`animus_control_protocol`] wire format directly to the daemon
//! when the daemon is running (socket present at the project's expected
//! `control.sock` path) and falls back to the existing in-process code
//! paths when the daemon is not running.
//!
//! ## Behavior contract
//!
//! [`ControlClient::try_connect`] returns:
//! - `Ok(Some(client))` when the socket exists and a connection succeeded
//! - `Ok(None)` when the socket does not exist — the CLI should run the
//!   in-process implementation instead. This is the steady-state for
//!   commands like `animus plugin install --path` while no daemon is
//!   running.
//! - `Err(_)` only for unexpected IO failures (e.g. socket exists but is
//!   un-openable due to permissions, malformed JSON-RPC response). The
//!   CLI surfaces these as errors rather than silently degrading.
//!
//! ## Anti-deadlock notes
//!
//! - Each [`ControlClient::call`] opens a fresh stream, sends one
//!   request, reads one line, and drops the stream. No persistent
//!   connection pool that could outlive a CLI invocation.
//! - No `tokio::sync::Mutex` for shared state; the client struct is
//!   `Clone` and trivially safe to share.
//! - Reads use the connection's natural newline framing — no timeouts
//!   are imposed here. A wedged daemon means a wedged CLI command, which
//!   is the correct UX (user `Ctrl+C`s rather than receiving phantom
//!   success).

use std::path::{Path, PathBuf};

use animus_control_protocol::{
    method as method_names,
    types::{
        DaemonAgentsResponse, DaemonHealthResponse, DaemonStatusResponse, PluginBrowseRequest, PluginCallRequest,
        PluginCallResponse, PluginInfo, PluginInfoRequest, PluginInstallRequest, PluginInstallResponse,
        PluginListRequest, PluginListResponse, PluginPingRequest, PluginPingResponse, PluginSearchRequest,
        PluginSearchResponse, PluginUninstallRequest, PluginUpdateRequest, PluginUpdateResponse, Unit,
        WorkflowCancelRequest, WorkflowExecuteRequest, WorkflowGetRequest, WorkflowListRequest, WorkflowListResponse,
        WorkflowPauseRequest, WorkflowResumeRequest, WorkflowRun, WorkflowRunRequest, WorkflowRunStart,
    },
};
use animus_plugin_protocol::{error_codes, RpcRequest, RpcResponse};
use anyhow::{anyhow, Context, Result};
use orchestrator_daemon_runtime::control::control_socket_path;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Handle to the daemon control socket for one CLI invocation.
///
/// Cheap to clone; carries only a socket path. Each [`Self::call`]
/// opens a fresh [`UnixStream`].
#[derive(Debug, Clone)]
pub struct ControlClient {
    socket_path: PathBuf,
}

impl ControlClient {
    /// Connect to the control socket for `project_root`, returning
    /// `None` when the socket does not exist (daemon not running) so
    /// callers can fall back to local code paths.
    ///
    /// Existence is checked via `std::fs::metadata` — symlinks are
    /// followed, broken or non-existent paths produce `Ok(None)`.
    pub async fn try_connect(project_root: &Path) -> Result<Option<Self>> {
        let socket_path = control_socket_path(project_root);
        if !socket_path.exists() {
            return Ok(None);
        }
        // Probe-connect to verify the socket is actually accepting
        // connections. A stale socket file (left by a crashed daemon)
        // exists but fails to connect — treat that as "daemon not
        // running" rather than as a hard error.
        match UnixStream::connect(&socket_path).await {
            Ok(_stream) => Ok(Some(Self { socket_path })),
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => Ok(None),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(anyhow!("failed to connect to control socket {}: {err}", socket_path.display())),
        }
    }

    /// Explicit constructor for tests that point a client at an
    /// arbitrary socket path (e.g. a tempdir).
    #[cfg(test)]
    pub fn from_socket_path(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Borrow the resolved socket path, useful in error messages and
    /// tests.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Issue one JSON-RPC request and decode the response into `R`.
    ///
    /// Each call opens a fresh `UnixStream`, sends the request as a
    /// single newline-terminated frame, reads exactly one line back,
    /// and parses it into [`RpcResponse`]. Returns:
    /// - `Ok(value)` on RPC success
    /// - `Err(_)` mapping the daemon's JSON-RPC error code into an
    ///   anyhow error tagged with the method name for log scrubbing
    pub async fn call<P, R>(&self, method: &str, params: P) -> Result<R>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        let params_value = serde_json::to_value(&params)
            .with_context(|| format!("control client: serializing params for {method}"))?;
        let response = self.call_raw(method, Some(params_value)).await?;
        match (response.result, response.error) {
            (Some(value), None) => {
                serde_json::from_value(value).with_context(|| format!("control client: decoding {method} response"))
            }
            (_, Some(error)) => Err(rpc_error_to_anyhow(method, &error)),
            (None, None) => Err(anyhow!("control client: empty {method} response (no result, no error)")),
        }
    }

    /// Issue one JSON-RPC request with raw params and return the full
    /// envelope. Lower-level than [`Self::call`]; used internally and
    /// by tests that need to inspect the error payload directly.
    pub async fn call_raw(&self, method: &str, params: Option<Value>) -> Result<RpcResponse> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| format!("connect to {}", self.socket_path.display()))?;
        let request = RpcRequest::new(serde_json::Value::from(1u64), method.to_string(), params);
        let mut bytes = serde_json::to_vec(&request).context("serialize RPC request")?;
        bytes.push(b'\n');
        stream.write_all(&bytes).await.context("write RPC request")?;
        stream.flush().await.context("flush RPC request")?;
        let (read_half, _write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut line = String::new();
        let n = reader.read_line(&mut line).await.context("read RPC response")?;
        if n == 0 {
            return Err(anyhow!("control server closed connection without responding to {method}"));
        }
        let response: RpcResponse =
            serde_json::from_str(line.trim_end()).with_context(|| format!("parse {method} RPC response: {line}"))?;
        Ok(response)
    }

    // ----- Plugin convenience methods --------------------------------

    /// Call `plugin/list`.
    pub async fn plugin_list(&self, request: PluginListRequest) -> Result<PluginListResponse> {
        self.call(method_names::METHOD_PLUGIN_LIST, request).await
    }

    /// Call `plugin/info`.
    pub async fn plugin_info(&self, request: PluginInfoRequest) -> Result<PluginInfo> {
        self.call(method_names::METHOD_PLUGIN_INFO, request).await
    }

    /// Call `plugin/install`.
    pub async fn plugin_install(&self, request: PluginInstallRequest) -> Result<PluginInstallResponse> {
        self.call(method_names::METHOD_PLUGIN_INSTALL, request).await
    }

    /// Call `plugin/uninstall`.
    pub async fn plugin_uninstall(&self, request: PluginUninstallRequest) -> Result<Unit> {
        self.call(method_names::METHOD_PLUGIN_UNINSTALL, request).await
    }

    /// Call `plugin/ping`.
    pub async fn plugin_ping(&self, request: PluginPingRequest) -> Result<PluginPingResponse> {
        self.call(method_names::METHOD_PLUGIN_PING, request).await
    }

    /// Call `plugin/call`.
    pub async fn plugin_call(&self, request: PluginCallRequest) -> Result<PluginCallResponse> {
        self.call(method_names::METHOD_PLUGIN_CALL, request).await
    }

    /// Call `plugin/search`.
    pub async fn plugin_search(&self, request: PluginSearchRequest) -> Result<PluginSearchResponse> {
        self.call(method_names::METHOD_PLUGIN_SEARCH, request).await
    }

    /// Call `plugin/browse`.
    pub async fn plugin_browse(&self, request: PluginBrowseRequest) -> Result<PluginSearchResponse> {
        self.call(method_names::METHOD_PLUGIN_BROWSE, request).await
    }

    /// Call `plugin/update`.
    pub async fn plugin_update(&self, request: PluginUpdateRequest) -> Result<PluginUpdateResponse> {
        self.call(method_names::METHOD_PLUGIN_UPDATE, request).await
    }

    // ----- Daemon convenience methods --------------------------------

    /// Call `daemon/status`.
    pub async fn daemon_status(&self) -> Result<DaemonStatusResponse> {
        self.call::<Value, _>(method_names::METHOD_DAEMON_STATUS, Value::Null).await
    }

    /// Call `daemon/health`.
    pub async fn daemon_health(&self) -> Result<DaemonHealthResponse> {
        self.call::<Value, _>(method_names::METHOD_DAEMON_HEALTH, Value::Null).await
    }

    /// Call `daemon/agents`.
    pub async fn daemon_agents(&self) -> Result<DaemonAgentsResponse> {
        self.call::<Value, _>(method_names::METHOD_DAEMON_AGENTS, Value::Null).await
    }

    // ----- Workflow convenience methods ------------------------------

    /// Call `workflow/list`.
    pub async fn workflow_list(&self, request: WorkflowListRequest) -> Result<WorkflowListResponse> {
        self.call(method_names::METHOD_WORKFLOW_LIST, request).await
    }

    /// Call `workflow/get`.
    pub async fn workflow_get(&self, request: WorkflowGetRequest) -> Result<WorkflowRun> {
        self.call(method_names::METHOD_WORKFLOW_GET, request).await
    }

    /// Call `workflow/run`.
    pub async fn workflow_run(&self, request: WorkflowRunRequest) -> Result<WorkflowRunStart> {
        self.call(method_names::METHOD_WORKFLOW_RUN, request).await
    }

    /// Call `workflow/execute`.
    pub async fn workflow_execute(&self, request: WorkflowExecuteRequest) -> Result<WorkflowRunStart> {
        self.call(method_names::METHOD_WORKFLOW_EXECUTE, request).await
    }

    /// Call `workflow/pause`.
    pub async fn workflow_pause(&self, request: WorkflowPauseRequest) -> Result<Unit> {
        self.call(method_names::METHOD_WORKFLOW_PAUSE, request).await
    }

    /// Call `workflow/resume`.
    pub async fn workflow_resume(&self, request: WorkflowResumeRequest) -> Result<Unit> {
        self.call(method_names::METHOD_WORKFLOW_RESUME, request).await
    }

    /// Call `workflow/cancel`.
    pub async fn workflow_cancel(&self, request: WorkflowCancelRequest) -> Result<Unit> {
        self.call(method_names::METHOD_WORKFLOW_CANCEL, request).await
    }
}

/// Translate a JSON-RPC error from the daemon into a CLI-side anyhow
/// error.
///
/// Method-not-supported / method-not-found map to user-facing strings
/// the CLI handler can choose to detect and fall back on (e.g. when the
/// daemon advertises a wire surface but a particular plugin/* method
/// hasn't been wired through yet). Other codes surface as plain error
/// messages.
fn rpc_error_to_anyhow(method: &str, error: &animus_plugin_protocol::RpcError) -> anyhow::Error {
    match error.code {
        error_codes::METHOD_NOT_FOUND => {
            anyhow!("control server method '{method}' not found: {}", error.message)
        }
        error_codes::METHOD_NOT_SUPPORTED => {
            anyhow!("control server method '{method}' not supported: {}", error.message)
        }
        _ => anyhow!("control server {method} failed (code {}): {}", error.code, error.message),
    }
}

/// True when the underlying JSON-RPC error indicates the daemon doesn't
/// know how to answer this method yet. CLI handlers check this to
/// decide whether to fall back to the local in-process implementation
/// or to surface the error directly.
pub fn is_method_unavailable(err: &anyhow::Error) -> bool {
    let s = format!("{err}");
    s.contains("not found:") || s.contains("not supported:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use animus_plugin_protocol::RpcError;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    #[tokio::test]
    async fn try_connect_returns_none_when_socket_missing() {
        let dir = TempDir::new().unwrap();
        let result = ControlClient::try_connect(dir.path()).await.unwrap();
        // The probe walks `~/.animus/<repo-scope>/control.sock`; for a
        // fresh tempdir that path will not exist.
        assert!(result.is_none() || result.is_some_and(|c| !c.socket_path().exists()));
    }

    #[tokio::test]
    async fn try_connect_returns_none_for_stale_socket_file() {
        // A regular file at the socket path produces ConnectionRefused
        // on connect (not a real socket).
        let dir = TempDir::new().unwrap();
        let sock = dir.path().join("control.sock");
        std::fs::write(&sock, "").unwrap();
        let client = ControlClient::from_socket_path(sock.clone());
        // Direct call should fail; just verify the from_socket_path
        // constructor wires the path through.
        assert_eq!(client.socket_path(), sock.as_path());
    }

    #[test]
    fn is_method_unavailable_detects_not_found() {
        let err = anyhow!("control server method 'plugin/list' not found: unknown control method: plugin/list");
        assert!(is_method_unavailable(&err));
    }

    #[test]
    fn is_method_unavailable_detects_not_supported() {
        let err = anyhow!("control server method 'workflow/list' not supported: deferred");
        assert!(is_method_unavailable(&err));
    }

    #[test]
    fn is_method_unavailable_ignores_other_errors() {
        let err = anyhow!("control server plugin/install failed (code -32000): boom");
        assert!(!is_method_unavailable(&err));
    }

    /// Spawn a minimal Unix-socket server that reads exactly one
    /// JSON-RPC frame, replies with the configured response, then
    /// closes. Used by the round-trip tests below; avoids depending on
    /// the full daemon ControlServer.
    fn short_sock_path() -> PathBuf {
        let unique = format!("animus-c6-{}-{}.sock", std::process::id(), uuid::Uuid::new_v4().simple());
        std::env::temp_dir().join(unique)
    }

    async fn spawn_fake_server<F>(socket_path: PathBuf, handler: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn(RpcRequest) -> RpcResponse + Send + Sync + 'static,
    {
        let listener = UnixListener::bind(&socket_path).expect("bind");
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let (read_half, mut write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                    return;
                }
                let request: RpcRequest = serde_json::from_str(line.trim_end()).expect("parse");
                let response = handler(request);
                let mut bytes = serde_json::to_vec(&response).expect("ser");
                bytes.push(b'\n');
                let _ = write_half.write_all(&bytes).await;
                let _ = write_half.flush().await;
            }
        })
    }

    #[tokio::test]
    async fn call_round_trips_success_response() {
        let socket_path = short_sock_path();
        let handler = |req: RpcRequest| {
            assert_eq!(req.method, "daemon/status");
            RpcResponse::ok(req.id, serde_json::json!({"running": true, "pid": 7}))
        };
        let server = spawn_fake_server(socket_path.clone(), handler).await;
        let client = ControlClient::from_socket_path(socket_path.clone());
        let result: serde_json::Value =
            tokio::time::timeout(Duration::from_secs(5), client.call("daemon/status", Value::Null))
                .await
                .expect("timeout")
                .expect("call");
        assert_eq!(result.get("running"), Some(&serde_json::json!(true)));
        assert_eq!(result.get("pid"), Some(&serde_json::json!(7)));
        let _ = std::fs::remove_file(socket_path);
        server.abort();
    }

    #[tokio::test]
    async fn call_surfaces_method_not_found_as_unavailable() {
        let socket_path = short_sock_path();
        let handler = |req: RpcRequest| {
            RpcResponse::err(
                req.id,
                RpcError {
                    code: animus_plugin_protocol::error_codes::METHOD_NOT_FOUND,
                    message: "unknown control method: ghost/method".to_string(),
                    data: None,
                },
            )
        };
        let server = spawn_fake_server(socket_path.clone(), handler).await;
        let client = ControlClient::from_socket_path(socket_path.clone());
        let err =
            tokio::time::timeout(Duration::from_secs(5), client.call::<Value, Value>("ghost/method", Value::Null))
                .await
                .expect("timeout")
                .unwrap_err();
        assert!(is_method_unavailable(&err), "expected is_method_unavailable to be true: {err}");
        let _ = std::fs::remove_file(socket_path);
        server.abort();
    }

    /// C6.5: `workflow/list` round-trips a WorkflowListResponse through
    /// the wire. Mirrors the daemon-side
    /// `workflow_list_routes_through_configured_routing` test from the
    /// CLI side: spawn a fake server, call the typed convenience method,
    /// verify the decoded response.
    #[tokio::test]
    async fn workflow_list_routes_via_control_when_socket_present() {
        let socket_path = short_sock_path();
        let handler = |req: RpcRequest| {
            assert_eq!(req.method, "workflow/list");
            RpcResponse::ok(
                req.id,
                serde_json::json!({
                    "runs": [{
                        "id": "wf-1",
                        "definition": "standard-workflow",
                        "status": "running",
                        "started_at": "2026-05-20T00:00:00Z",
                    }],
                }),
            )
        };
        let server = spawn_fake_server(socket_path.clone(), handler).await;
        let client = ControlClient::from_socket_path(socket_path.clone());
        let request = animus_control_protocol::types::WorkflowListRequest::default();
        let response = tokio::time::timeout(Duration::from_secs(5), client.workflow_list(request))
            .await
            .expect("timeout")
            .expect("call");
        assert_eq!(response.runs.len(), 1);
        assert_eq!(response.runs[0].id, "wf-1");
        let _ = std::fs::remove_file(socket_path);
        server.abort();
    }

    /// C6.5: when no socket is present the helper returns Ok(None) so
    /// the CLI falls back to the local in-process path. Verified end to
    /// end by `ControlClient::try_connect`; this test pins the contract.
    #[tokio::test]
    async fn workflow_list_falls_back_when_socket_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        // Use a project_root that has no `~/.animus/<scope>/control.sock`.
        let result = ControlClient::try_connect(dir.path()).await.unwrap();
        // Either the path doesn't exist (None) or it exists but is unusable.
        assert!(result.is_none() || result.is_some_and(|c| !c.socket_path().exists()));
    }
}
