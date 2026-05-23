//! `SessionBackend` adapter that dispatches `agent/run` to a discovered AO STDIO plugin.
//!
//! Each call spawns the plugin binary, completes the handshake, sends an `agent/run`
//! request, drains the response into a single `SessionEvent::FinalText` followed by
//! `Finished`, and shuts the plugin down. Lightweight per-call lifecycle keeps the
//! v1 surface simple; pooling can come later.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use animus_plugin_protocol::{EnvRequirement, RpcError, RpcNotification};
use async_trait::async_trait;
use orchestrator_logging::Logger;
use orchestrator_plugin_host::{PluginHost, PluginSpawnOptions, PluginStderrSink};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::plugin_supervisor::{is_death_like_error, PluginSupervisor, SupervisorError};

/// Default cap on how long the plugin host will wait for a single `agent/run`
/// or `agent/resume` reply before giving up. Caller-supplied `timeout_secs` on
/// the SessionRequest takes precedence.
const DEFAULT_PLUGIN_RUN_TIMEOUT_SECS: u64 = 1_800; // 30 min
/// Default cap for the synchronous `agent/cancel` round trip.
const DEFAULT_PLUGIN_CANCEL_TIMEOUT_SECS: u64 = 10;
/// Wait at most this long for the plugin to gracefully shut down before we
/// drop its connection.
const PLUGIN_SHUTDOWN_TIMEOUT_SECS: u64 = 5;

use crate::error::{Error, Result};
use animus_session_backend::session::{
    session_backend::SessionBackend, session_backend_info::SessionBackendInfo,
    session_backend_kind::SessionBackendKind, session_capabilities::SessionCapabilities, session_event::SessionEvent,
    session_request::SessionRequest, session_run::SessionRun, session_stability::SessionStability,
};

/// Terminal outcome reported by `resume_agent_for_restart` after the daemon
/// drains a resumed session to completion or surfaces a failure.
#[derive(Debug)]
pub enum ResumeAgentOutcome {
    Resumed { session_id: Option<String> },
    Failed { reason: String },
}

/// In-memory record of a live `agent/run`-initiated session whose plugin host
/// must be reused for subsequent control-plane calls (currently just
/// `agent/cancel`).
///
/// Keyed by the `control_session_id` minted in [`PluginSessionBackend::dispatch`]
/// and returned to callers via [`SessionRun::session_id`].
///
/// TODO(audit-gap-7-followup): add TTL-based background eviction so handles
/// for plugins that crash or fail to send `Finished` don't accumulate.
pub(crate) struct SessionHandle {
    pub(crate) host: PluginHost,
    /// When the session was registered; reserved for TTL eviction in a
    /// follow-up commit.
    #[allow(dead_code)]
    pub(crate) started_at: Instant,
    /// Cached from the plugin's `initialize` response so cancel routing can
    /// short-circuit with [`HostError::CapabilityNotSupported`] without paying
    /// for a wasted RPC round trip.
    pub(crate) cancellation: bool,
}

/// Map of `control_session_id` -> live [`SessionHandle`]. Guarded by a sync
/// mutex because the operations are all CPU-bound (HashMap insert/get/remove);
/// we never hold this lock across `.await`.
pub(crate) type SessionMap = Arc<StdMutex<HashMap<String, SessionHandle>>>;

/// Wraps a discovered AO STDIO plugin so the resolver can route `agent/run`
/// through it as if it were any other in-tree backend.
#[derive(Clone)]
pub struct PluginSessionBackend {
    pub(crate) plugin_name: String,
    pub(crate) binary_path: PathBuf,
    pub(crate) provider_tool: String,
    pub(crate) display_name: String,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) env_required: Vec<EnvRequirement>,
    pub(crate) sessions: SessionMap,
    pub(crate) supervisor: Arc<PluginSupervisor>,
}

impl PluginSessionBackend {
    pub fn new(plugin_name: impl Into<String>, binary_path: PathBuf, provider_tool: impl Into<String>) -> Self {
        let plugin_name = plugin_name.into();
        let provider_tool = provider_tool.into();
        let display_name = format!("Plugin Provider ({plugin_name})");
        let supervisor = Arc::new(PluginSupervisor::with_defaults(plugin_name.clone()));
        Self {
            plugin_name,
            binary_path,
            provider_tool,
            display_name,
            project_root: None,
            env_required: Vec::new(),
            sessions: Arc::new(StdMutex::new(HashMap::new())),
            supervisor,
        }
    }

    #[must_use]
    pub fn with_supervisor(mut self, supervisor: Arc<PluginSupervisor>) -> Self {
        self.supervisor = supervisor;
        self
    }

    pub fn supervisor(&self) -> Arc<PluginSupervisor> {
        self.supervisor.clone()
    }

    /// Test-only: insert a `SessionHandle` directly into the session map so a
    /// fake host can be exercised by `dispatch_cancel` without first running
    /// `dispatch`. Production code never touches this.
    #[cfg(test)]
    pub(crate) fn insert_session_for_test(&self, control_session_id: impl Into<String>, handle: SessionHandle) {
        self.sessions.lock().expect("session map mutex poisoned in test").insert(control_session_id.into(), handle);
    }

    /// Test-only: peek at whether a session is currently registered.
    #[cfg(test)]
    pub(crate) fn has_session_for_test(&self, control_session_id: &str) -> bool {
        self.sessions.lock().expect("session map mutex poisoned in test").contains_key(control_session_id)
    }

    /// Bind a project root so structured log entries about every spawn / call /
    /// stderr line land in `~/.animus/<repo-scope>/logs/events.jsonl` for that project.
    #[must_use]
    pub fn with_project_root(mut self, project_root: impl Into<PathBuf>) -> Self {
        self.project_root = Some(project_root.into());
        self
    }

    /// Set the plugin's manifest-declared env requirements. The host scrubs
    /// the daemon environment at spawn time and forwards only these vars (on
    /// top of the universal base allowlist).
    #[must_use]
    pub fn with_env_required(mut self, env_required: Vec<EnvRequirement>) -> Self {
        self.env_required = env_required;
        self
    }

    fn spawn_options(&self, request_env_vars: &[(String, String)]) -> PluginSpawnOptions {
        let extras = request_env_vars.iter().map(|(name, _)| name.clone());
        PluginSpawnOptions::for_manifest(self.plugin_name.clone(), &self.env_required, extras, self.stderr_sink_for())
    }

    fn project_logger(&self) -> Option<Logger> {
        self.project_root.as_ref().map(|root| Logger::for_project(root))
    }

    fn stderr_sink_for(&self) -> Option<PluginStderrSink> {
        let root = self.project_root.clone()?;
        let plugin_name = self.plugin_name.clone();
        Some(Arc::new(move |emitting_plugin: &str, line: &str| {
            let logger = Logger::for_project(&root);
            logger
                .warn("plugin.stderr", line)
                .meta(json!({
                    "plugin": plugin_name,
                    "emitter": emitting_plugin,
                }))
                .emit();
        }))
    }

    fn build_run_params(&self, request: &SessionRequest, resume_session: Option<&str>) -> Value {
        let extras = request.extras.as_object().cloned().unwrap_or_default();
        let mut params = json!({
            "prompt": request.prompt,
            "model": request.model,
            "cwd": request.cwd,
            "env": request.env_vars.iter().cloned().collect::<std::collections::HashMap<_, _>>(),
        });

        if let Some(project_root) = &request.project_root {
            params["project_root"] = json!(project_root);
        }
        if let Some(mode) = &request.permission_mode {
            params["permission_mode"] = json!(mode);
        }
        if let Some(timeout) = request.timeout_secs {
            params["timeout_secs"] = json!(timeout);
        }
        if let Some(sid) = resume_session {
            params["session_id"] = json!(sid);
        } else if let Some(sid) = extras.get("session_id") {
            params["session_id"] = sid.clone();
        }
        for key in ["system_prompt", "claude_profile", "mcp_servers", "tools", "response_schema", "runtime_contract"] {
            if let Some(value) = extras.get(key) {
                params[key] = value.clone();
            }
        }
        params
    }

    async fn spawn_and_handshake(
        &self,
        request_env_vars: &[(String, String)],
    ) -> Result<(PluginHost, animus_plugin_protocol::InitializeResult)> {
        let spawn_options = self.spawn_options(request_env_vars);
        let host = match PluginHost::spawn_with_options(&self.binary_path, &[], spawn_options).await {
            Ok(host) => host,
            Err(error) => {
                let message = format!("plugin '{}' spawn failed: {error}", self.plugin_name);
                if let Some(logger) = self.project_logger() {
                    logger.error("plugin.dispatch.spawn", &message).err(error.to_string()).emit();
                }
                return Err(Error::execution_failed(message));
            }
        };

        let init_result = match host.handshake().await {
            Ok(result) => result,
            Err(error) => {
                graceful_shutdown(host).await;
                let message = format!(
                    "plugin '{}' handshake failed: {error}; to bypass this plugin and use the in-tree backend, set ANIMUS_PROVIDER_DISABLE_PLUGIN=1 and restart the daemon",
                    self.plugin_name
                );
                if let Some(logger) = self.project_logger() {
                    logger.error("plugin.dispatch.handshake", &message).err(error.to_string()).emit();
                }
                return Err(Error::execution_failed(message));
            }
        };
        Ok((host, init_result))
    }

    async fn dispatch(
        &self,
        method: &str,
        request: SessionRequest,
        resume_session: Option<String>,
    ) -> Result<SessionRun> {
        let params = self.build_run_params(&request, resume_session.as_deref());
        let backend_label = format!("plugin:{}", self.plugin_name);
        let control_session_id = Uuid::new_v4().to_string();
        let run_timeout = Duration::from_secs(request.timeout_secs.unwrap_or(DEFAULT_PLUGIN_RUN_TIMEOUT_SECS));
        let started_at = Instant::now();

        if self.supervisor.is_disabled() {
            let retry_after = self.supervisor.disabled_remaining().unwrap_or_default();
            let message = format!(
                "plugin '{}' is currently disabled by supervisor (retry after {}s)",
                self.plugin_name,
                retry_after.as_secs()
            );
            if let Some(logger) = self.project_logger() {
                logger
                    .warn("plugin.dispatch.disabled", &message)
                    .meta(json!({
                        "plugin": self.plugin_name,
                        "retry_after_secs": retry_after.as_secs(),
                    }))
                    .emit();
            }
            return Err(Error::execution_failed(message));
        }

        if let Some(logger) = self.project_logger() {
            logger
                .info("plugin.dispatch.start", format!("{} → {}", self.plugin_name, method))
                .meta(json!({
                    "plugin": self.plugin_name,
                    "method": method,
                    "tool": request.tool,
                    "model": request.model,
                    "control_session_id": control_session_id,
                    "resume_session": resume_session,
                }))
                .emit();
        }

        let (host, init_result) = self.spawn_and_handshake(&request.env_vars).await?;
        let notifications = host.subscribe_notifications();

        // Register the session-keyed host so dispatch_cancel can route through
        // the SAME transport instead of spawning a brand-new plugin process
        // that has no knowledge of the in-flight session. Insert BEFORE the
        // streaming task starts so cancel callers observe the handle even on
        // very fast plugins. The streaming task removes the entry when the
        // run finishes (success, error, or timeout).
        {
            let mut guard = self.sessions.lock().expect("session map mutex poisoned");
            guard.insert(
                control_session_id.clone(),
                SessionHandle {
                    host: host.clone(),
                    started_at: Instant::now(),
                    cancellation: init_result.capabilities.cancellation,
                },
            );
        }
        let sessions_for_cleanup = self.sessions.clone();
        let control_session_id_for_cleanup = control_session_id.clone();

        // Hand the SessionRun receiver out IMMEDIATELY so the caller can read live
        // stream events while the plugin is still working. The actual JSON-RPC call
        // runs in a background task that forwards notifications and produces the
        // final result.
        let (tx, rx) = mpsc::channel::<SessionEvent>(64);
        let session_id_for_started = Some(control_session_id.clone());
        let backend_for_started = backend_label.clone();
        let _ = tx
            .send(SessionEvent::Started { backend: backend_for_started, session_id: session_id_for_started, pid: None })
            .await;

        let plugin_name_for_task = self.plugin_name.clone();
        let backend_label_for_task = backend_label.clone();
        let project_root_for_task = self.project_root.clone();
        let method_string = method.to_string();
        let stream_tx = tx.clone();
        let supervisor_for_task = self.supervisor.clone();
        let backend_for_retry = self.clone();
        let request_env_for_retry = request.env_vars.clone();
        let sessions_for_retry = self.sessions.clone();
        tokio::spawn(async move {
            let notifications_forwarded = Arc::new(AtomicBool::new(false));

            let response = run_request_with_notifications(
                host.clone(),
                notifications,
                method_string.clone(),
                params.clone(),
                run_timeout,
                stream_tx.clone(),
                plugin_name_for_task.clone(),
                notifications_forwarded.clone(),
            )
            .await;

            let logger = project_root_for_task.as_ref().map(|root| Logger::for_project(root));
            let duration_ms = started_at.elapsed().as_millis() as u64;

            // Retry-once on death-like errors only when:
            //   (a) the error is NOT a structured JSON-RPC error from the plugin
            //   (b) no notifications were forwarded yet (otherwise retry would
            //       double-emit deltas to the caller)
            //   (c) the supervisor's restart budget has not been exhausted
            //
            // Best-effort idempotency: if the plugin's first request had already
            // executed side effects before crashing (e.g. it spawned a sub-tool
            // that mutated state but died before writing the JSON-RPC response),
            // the retry could re-execute those side effects. Plugin authors must
            // make their request handlers idempotent or accept the duplicate.
            let (outcome, host_to_shutdown) = match response {
                ResponseOutcome::Ok(value) => (Some(value), Some(host)),
                ResponseOutcome::Timeout => {
                    let message = format!(
                        "plugin '{}' {method_string} timed out after {}s",
                        plugin_name_for_task,
                        run_timeout.as_secs()
                    );
                    let _ = stream_tx.send(SessionEvent::Error { message: message.clone(), recoverable: false }).await;
                    if let Some(logger) = logger.as_ref() {
                        logger
                            .error("plugin.dispatch.timeout", &message)
                            .duration(duration_ms)
                            .meta(json!({ "plugin": plugin_name_for_task, "method": method_string, "timeout_secs": run_timeout.as_secs() }))
                            .emit();
                    }
                    let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(124) }).await;
                    remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
                    graceful_shutdown(host).await;
                    return;
                }
                ResponseOutcome::Err(error) => {
                    let already_forwarded = notifications_forwarded.load(AtomicOrdering::SeqCst);
                    // TODO(v0.5): rewire to typed HostError once dispatch returns HostError
                    let can_retry = is_death_like_error(error.code, &error.message) && !already_forwarded;

                    if can_retry {
                        if let Some(logger) = logger.as_ref() {
                            logger
                                .warn(
                                    "plugin.dispatch.retry",
                                    format!(
                                        "plugin '{}' died mid-call ({}); attempting retry-once",
                                        plugin_name_for_task, error.message
                                    ),
                                )
                                .meta(json!({
                                    "plugin": plugin_name_for_task,
                                    "method": method_string,
                                    "code": error.code,
                                }))
                                .emit();
                        }
                        let _ = graceful_shutdown(host).await;

                        match supervisor_for_task.record_restart() {
                            Ok(()) => {}
                            Err(SupervisorError::TooManyRestarts { plugin, count, window }) => {
                                let message = format!(
                                    "plugin '{plugin}' exhausted restart budget ({count} restarts in {}s); marked disabled",
                                    window.as_secs()
                                );
                                let _ = stream_tx
                                    .send(SessionEvent::Error { message: message.clone(), recoverable: false })
                                    .await;
                                if let Some(logger) = logger.as_ref() {
                                    logger
                                        .error("plugin.dispatch.disabled", &message)
                                        .duration(duration_ms)
                                        .meta(json!({ "plugin": plugin, "method": method_string, "restart_count": count }))
                                        .emit();
                                }
                                let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(1) }).await;
                                remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
                                return;
                            }
                            Err(SupervisorError::PluginDisabled { plugin, retry_after }) => {
                                let message = format!(
                                    "plugin '{plugin}' disabled by supervisor (retry after {}s)",
                                    retry_after.as_secs()
                                );
                                let _ = stream_tx
                                    .send(SessionEvent::Error { message: message.clone(), recoverable: false })
                                    .await;
                                let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(1) }).await;
                                remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
                                return;
                            }
                        }

                        match backend_for_retry.spawn_and_handshake(&request_env_for_retry).await {
                            Ok((new_host, new_init)) => {
                                {
                                    let mut guard = sessions_for_retry.lock().expect("session map mutex poisoned");
                                    guard.insert(
                                        control_session_id_for_cleanup.clone(),
                                        SessionHandle {
                                            host: new_host.clone(),
                                            started_at: Instant::now(),
                                            cancellation: new_init.capabilities.cancellation,
                                        },
                                    );
                                }
                                let new_notifications = new_host.subscribe_notifications();
                                let retry_response = run_request_with_notifications(
                                    new_host.clone(),
                                    new_notifications,
                                    method_string.clone(),
                                    params.clone(),
                                    run_timeout,
                                    stream_tx.clone(),
                                    plugin_name_for_task.clone(),
                                    notifications_forwarded.clone(),
                                )
                                .await;
                                match retry_response {
                                    ResponseOutcome::Ok(value) => (Some(value), Some(new_host)),
                                    ResponseOutcome::Timeout => {
                                        let message = format!(
                                            "plugin '{}' {method_string} timed out after retry",
                                            plugin_name_for_task
                                        );
                                        let _ = stream_tx
                                            .send(SessionEvent::Error { message: message.clone(), recoverable: false })
                                            .await;
                                        let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(124) }).await;
                                        remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
                                        graceful_shutdown(new_host).await;
                                        return;
                                    }
                                    ResponseOutcome::Err(retry_error) => {
                                        let message = format!(
                                            "plugin '{}' {method_string} failed after retry ({}): {}",
                                            plugin_name_for_task, retry_error.code, retry_error.message
                                        );
                                        let _ = stream_tx
                                            .send(SessionEvent::Error { message: message.clone(), recoverable: false })
                                            .await;
                                        if let Some(logger) = logger.as_ref() {
                                            logger
                                                .error("plugin.dispatch.retry_failed", &message)
                                                .duration(duration_ms)
                                                .meta(json!({
                                                    "plugin": plugin_name_for_task,
                                                    "method": method_string,
                                                    "code": retry_error.code,
                                                }))
                                                .emit();
                                        }
                                        let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(1) }).await;
                                        remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
                                        graceful_shutdown(new_host).await;
                                        return;
                                    }
                                }
                            }
                            Err(spawn_error) => {
                                let message =
                                    format!("plugin '{}' retry spawn failed: {spawn_error}", plugin_name_for_task);
                                let _ = stream_tx
                                    .send(SessionEvent::Error { message: message.clone(), recoverable: false })
                                    .await;
                                let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(1) }).await;
                                remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
                                return;
                            }
                        }
                    } else {
                        let message = format!(
                            "plugin '{}' {method_string} failed ({}): {}",
                            plugin_name_for_task, error.code, error.message
                        );
                        let _ =
                            stream_tx.send(SessionEvent::Error { message: message.clone(), recoverable: false }).await;
                        if let Some(logger) = logger.as_ref() {
                            logger
                                .error("plugin.dispatch.error", &message)
                                .duration(duration_ms)
                                .meta(json!({ "plugin": plugin_name_for_task, "method": method_string }))
                                .emit();
                        }
                        let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(1) }).await;
                        remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
                        graceful_shutdown(host).await;
                        return;
                    }
                }
            };

            remove_session(&sessions_for_cleanup, &control_session_id_for_cleanup);
            if let Some(host) = host_to_shutdown {
                graceful_shutdown(host).await;
            }

            if let Some(response) = outcome {
                let exit_code = response.get("exit_code").and_then(Value::as_i64).map(|n| n as i32);
                let output = response.get("output").and_then(Value::as_str).unwrap_or_default().to_string();
                let metadata = response.get("metadata").cloned().unwrap_or(Value::Null);

                if !output.is_empty() {
                    let _ = stream_tx.send(SessionEvent::FinalText { text: output }).await;
                }
                if !matches!(metadata, Value::Null) {
                    let _ = stream_tx.send(SessionEvent::Metadata { metadata }).await;
                }

                if let Some(logger) = logger.as_ref() {
                    logger
                        .info("plugin.dispatch.complete", format!("{} → {} ok", plugin_name_for_task, method_string))
                        .duration(duration_ms)
                        .meta(json!({
                            "plugin": plugin_name_for_task,
                            "method": method_string,
                            "exit_code": exit_code,
                            "backend": backend_label_for_task,
                        }))
                        .emit();
                }
                let _ = stream_tx.send(SessionEvent::Finished { exit_code }).await;
            }
        });

        Ok(SessionRun {
            session_id: Some(control_session_id),
            events: rx,
            selected_backend: backend_label,
            fallback_reason: None,
            pid: None,
        })
    }

    /// Daemon-restart helper: rebuild a minimal `SessionRequest` from the
    /// `AgentRunRequest` JSON captured in the phase session checkpoint and
    /// dispatch `agent/resume` against the live plugin. Drains the resulting
    /// event stream synchronously until a terminal `Finished` / `Error` event
    /// arrives, then returns a [`ResumeAgentOutcome`] describing the result.
    ///
    /// Returns `ResumeAgentOutcome::Failed { reason }` on every error path
    /// (malformed checkpoint, spawn/handshake failure, RPC error, timeout) so
    /// the caller can record the reason verbatim in the blocked-state message.
    pub async fn resume_agent_for_restart(&self, session_id: &str, agent_run_request: &Value) -> ResumeAgentOutcome {
        let session_request = match session_request_from_agent_run_request(&self.provider_tool, agent_run_request) {
            Ok(req) => req,
            Err(reason) => return ResumeAgentOutcome::Failed { reason },
        };
        let mut run = match self.dispatch("agent/resume", session_request, Some(session_id.to_string())).await {
            Ok(run) => run,
            Err(err) => return ResumeAgentOutcome::Failed { reason: format!("resume_agent failed: {err}") },
        };
        let resumed_session_id = run.session_id.clone();
        while let Some(event) = run.events.recv().await {
            match event {
                SessionEvent::Finished { exit_code } => {
                    if exit_code.unwrap_or(0) == 0 {
                        return ResumeAgentOutcome::Resumed { session_id: resumed_session_id };
                    }
                    return ResumeAgentOutcome::Failed {
                        reason: format!(
                            "resume_agent failed: plugin exited with code {}",
                            exit_code.map(|c| c.to_string()).unwrap_or_else(|| "unknown".to_string())
                        ),
                    };
                }
                SessionEvent::Error { message, recoverable: false } => {
                    return ResumeAgentOutcome::Failed { reason: format!("resume_agent failed: {message}") };
                }
                _ => {}
            }
        }
        ResumeAgentOutcome::Failed { reason: "resume_agent failed: stream closed before Finished".to_string() }
    }

    async fn dispatch_cancel(&self, session_id: &str) -> Result<()> {
        let cancel_timeout = Duration::from_secs(DEFAULT_PLUGIN_CANCEL_TIMEOUT_SECS);
        if let Some(logger) = self.project_logger() {
            logger
                .info("plugin.cancel", format!("{} → cancel {}", self.plugin_name, session_id))
                .meta(json!({ "plugin": self.plugin_name, "session_id": session_id }))
                .emit();
        }

        // Look the session up under the lock; CLONE the host out (PluginHost is
        // an Arc<PluginHostInner>, so cloning is cheap and shares the same
        // transport with the original dispatch task), then DROP the lock
        // before doing any await. Holding a sync mutex across .await would
        // deadlock under tokio's current-thread scheduler.
        let (host, cancellation) = {
            let guard = self.sessions.lock().expect("session map mutex poisoned");
            match guard.get(session_id) {
                Some(handle) => (handle.host.clone(), handle.cancellation),
                None => {
                    return Err(Error::execution_failed(format!(
                        "no active session '{session_id}' for plugin '{}'; nothing to cancel",
                        self.plugin_name
                    )));
                }
            }
        };

        if !cancellation {
            return Err(Error::CapabilityNotSupported {
                plugin: self.plugin_name.clone(),
                capability: "cancellation".to_string(),
            });
        }

        let request_future = host.request("agent/cancel".to_string(), Some(json!({ "session_id": session_id })));
        let result = tokio::time::timeout(cancel_timeout, request_future).await;
        match result {
            Ok(Ok(_)) => {
                remove_session(&self.sessions, session_id);
                Ok(())
            }
            Ok(Err(error)) => Err(Error::execution_failed(format!(
                "plugin '{}' agent/cancel failed: {}",
                self.plugin_name, error.message
            ))),
            Err(_) => Err(Error::execution_failed(format!(
                "plugin '{}' agent/cancel timed out after {}s",
                self.plugin_name,
                cancel_timeout.as_secs()
            ))),
        }
    }
}

/// Briefly lock the session map and drop the entry for `session_id`. The
/// dropped [`SessionHandle`] (and its embedded `PluginHost`) is released
/// outside the lock, so nothing async runs under the mutex guard.
fn remove_session(sessions: &SessionMap, session_id: &str) {
    let removed = {
        let mut guard = sessions.lock().expect("session map mutex poisoned");
        guard.remove(session_id)
    };
    drop(removed);
}

#[async_trait]
impl SessionBackend for PluginSessionBackend {
    fn info(&self) -> SessionBackendInfo {
        SessionBackendInfo {
            kind: SessionBackendKind::Subprocess,
            provider_tool: self.provider_tool.clone(),
            stability: SessionStability::Experimental,
            display_name: self.display_name.clone(),
        }
    }

    fn capabilities(&self) -> SessionCapabilities {
        SessionCapabilities {
            supports_resume: true,
            supports_terminate: true,
            supports_permissions: true,
            supports_mcp: true,
            supports_tool_events: false,
            supports_thinking_events: false,
            supports_artifact_events: false,
            supports_usage_metadata: true,
        }
    }

    async fn start_session(&self, request: SessionRequest) -> animus_session_backend::error::Result<SessionRun> {
        self.dispatch("agent/run", request, None).await.map_err(Into::into)
    }

    async fn resume_session(
        &self,
        request: SessionRequest,
        session_id: &str,
    ) -> animus_session_backend::error::Result<SessionRun> {
        self.dispatch("agent/resume", request, Some(session_id.to_string())).await.map_err(Into::into)
    }

    async fn terminate_session(&self, session_id: &str) -> animus_session_backend::error::Result<()> {
        self.dispatch_cancel(session_id).await.map_err(Into::into)
    }
}

fn session_request_from_agent_run_request(
    fallback_tool: &str,
    agent_run_request: &Value,
) -> std::result::Result<SessionRequest, String> {
    let context = agent_run_request.get("context").cloned().unwrap_or(Value::Null);
    let model = agent_run_request.get("model").and_then(Value::as_str).unwrap_or_default().to_string();
    let timeout_secs = agent_run_request.get("timeout_secs").and_then(Value::as_u64);

    let tool = context
        .get("tool")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| context.pointer("/runtime_contract/cli/name").and_then(Value::as_str).map(ToOwned::to_owned))
        .unwrap_or_else(|| fallback_tool.to_string());
    let prompt = context.get("prompt").and_then(Value::as_str).unwrap_or_default().to_string();
    let cwd = context
        .get("cwd")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "resume_agent failed: checkpoint missing context.cwd".to_string())?;
    let project_root = context.get("project_root").and_then(Value::as_str).map(PathBuf::from);

    let mut extras = serde_json::Map::new();
    for key in ["runtime_contract", "agent_id", "phase_id", "workflow_id", "subject_id", "phase_capabilities"] {
        if let Some(value) = context.get(key) {
            extras.insert(key.to_string(), value.clone());
        }
    }

    Ok(SessionRequest {
        tool,
        model,
        prompt,
        cwd,
        project_root,
        mcp_endpoint: None,
        permission_mode: None,
        timeout_secs,
        env_vars: Vec::new(),
        extras: Value::Object(extras),
    })
}

async fn graceful_shutdown(host: PluginHost) {
    let _ = tokio::time::timeout(Duration::from_secs(PLUGIN_SHUTDOWN_TIMEOUT_SECS), host.shutdown()).await;
}

enum ResponseOutcome {
    Ok(Value),
    Err(RpcError),
    Timeout,
}

#[allow(clippy::too_many_arguments)]
async fn run_request_with_notifications(
    host: PluginHost,
    mut notifications: tokio::sync::broadcast::Receiver<RpcNotification>,
    method: String,
    params: Value,
    run_timeout: Duration,
    stream_tx: mpsc::Sender<SessionEvent>,
    plugin_name: String,
    notifications_forwarded: Arc<AtomicBool>,
) -> ResponseOutcome {
    let outcome = {
        let mut request_future = Box::pin(host.request(method.clone(), Some(params)));
        let timeout_future = tokio::time::sleep(run_timeout);
        tokio::pin!(timeout_future);
        loop {
            tokio::select! {
                biased;
                notification = notifications.recv() => {
                    match notification {
                        Ok(notification) => {
                            forward_plugin_notification(&plugin_name, notification, &stream_tx).await;
                            notifications_forwarded.store(true, AtomicOrdering::SeqCst);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                plugin = %plugin_name,
                                skipped,
                                "plugin notification subscriber lagged; some events were dropped"
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // Reader task exited (plugin closed). Keep awaiting the
                            // request future; it'll resolve with ConnectionLost soon.
                        }
                    }
                }
                result = &mut request_future => {
                    break Some(result);
                }
                _ = &mut timeout_future => {
                    break None;
                }
            }
        }
    };

    loop {
        match notifications.try_recv() {
            Ok(notification) => {
                forward_plugin_notification(&plugin_name, notification, &stream_tx).await;
                notifications_forwarded.store(true, AtomicOrdering::SeqCst);
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                tracing::warn!(
                    plugin = %plugin_name,
                    skipped,
                    "plugin notification subscriber lagged during drain; some events were dropped"
                );
            }
            Err(_) => break,
        }
    }

    match outcome {
        Some(Ok(value)) => ResponseOutcome::Ok(value),
        Some(Err(error)) => ResponseOutcome::Err(error),
        None => ResponseOutcome::Timeout,
    }
}

/// Translate a JSON-RPC notification emitted by a provider plugin into the
/// matching SessionEvent and forward it to the SessionRun receiver.
async fn forward_plugin_notification(
    plugin_name: &str,
    notification: RpcNotification,
    tx: &mpsc::Sender<SessionEvent>,
) {
    match notification.method.as_str() {
        "agent/output" => {
            if let Some(text) = notification.params.as_ref().and_then(|p| p.get("text")).and_then(Value::as_str) {
                let _ = tx.send(SessionEvent::TextDelta { text: text.to_string() }).await;
            }
        }
        "agent/thinking" => {
            if let Some(text) = notification.params.as_ref().and_then(|p| p.get("text")).and_then(Value::as_str) {
                let _ = tx.send(SessionEvent::Thinking { text: text.to_string() }).await;
            }
        }
        "agent/toolCall" => {
            let params = notification.params.unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
            let server = params.get("server").and_then(Value::as_str).map(ToOwned::to_owned);
            let _ = tx.send(SessionEvent::ToolCall { tool_name, arguments, server }).await;
        }
        "agent/toolResult" => {
            let params = notification.params.unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let output = params.get("output").cloned().unwrap_or(Value::Null);
            let success = params.get("success").and_then(Value::as_bool).unwrap_or(true);
            let _ = tx.send(SessionEvent::ToolResult { tool_name, output, success }).await;
        }
        "agent/error" => {
            let params = notification.params.unwrap_or(Value::Null);
            let message = params.get("message").and_then(Value::as_str).unwrap_or_default().to_string();
            let recoverable = params.get("recoverable").and_then(Value::as_bool).unwrap_or(true);
            let _ = tx.send(SessionEvent::Error { message, recoverable }).await;
        }
        "$/progress" | "agent/progress" => {
            if let Some(params) = notification.params {
                let _ = tx.send(SessionEvent::Metadata { metadata: params }).await;
            }
        }
        other => {
            // Unrecognized notification: surface as metadata so consumers can still inspect.
            tracing::debug!(plugin = %plugin_name, method = %other, "unrecognized plugin notification");
            if let Some(params) = notification.params {
                let _ =
                    tx.send(SessionEvent::Metadata { metadata: json!({ "method": other, "params": params }) }).await;
            }
        }
    }
}

/// Snapshot of a discovered provider plugin used to lazily build a `PluginSessionBackend`.
#[derive(Debug, Clone)]
pub struct DiscoveredProviderPlugin {
    pub plugin_name: String,
    pub provider_tool: String,
    pub binary_path: PathBuf,
    pub project_root: Option<PathBuf>,
    pub env_required: Vec<EnvRequirement>,
}

impl DiscoveredProviderPlugin {
    pub fn into_backend(self) -> Arc<PluginSessionBackend> {
        let mut backend = PluginSessionBackend::new(self.plugin_name, self.binary_path, self.provider_tool)
            .with_env_required(self.env_required);
        if let Some(root) = self.project_root {
            backend = backend.with_project_root(root);
        }
        Arc::new(backend)
    }
}

/// Inspect manifests of every discovered plugin under `project_root` and return only
/// the provider-kind plugins. Provider tool name defaults to the plugin name minus the
/// `animus-provider-` prefix.
pub fn discover_provider_plugins(project_root: &std::path::Path) -> Vec<DiscoveredProviderPlugin> {
    use orchestrator_plugin_host::discover_plugins;
    let project_root = project_root.to_path_buf();
    discover_plugins(&project_root)
        .unwrap_or_default()
        .into_iter()
        .filter(|plugin| plugin.manifest.plugin_kind == animus_plugin_protocol::PLUGIN_KIND_PROVIDER)
        .map(|plugin| {
            let provider_tool = plugin
                .name
                .strip_prefix("animus-provider-")
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| plugin.name.clone());
            let env_required = plugin.manifest.env_required.clone();
            DiscoveredProviderPlugin {
                plugin_name: plugin.name,
                provider_tool,
                binary_path: plugin.path,
                project_root: Some(project_root.clone()),
                env_required,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Minimum-viable coverage for the cancel-routes-through-existing-host
    //! property. Heavier scenarios (concurrent runs, in-flight cancel
    //! propagation) intentionally deferred — see audit gap #7 follow-up.
    use super::*;
    use animus_plugin_protocol::{RpcRequest, RpcResponse};
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

    /// Spin up an in-process fake plugin reachable through a `PluginHost`. The
    /// fake echoes every incoming JSON-RPC request back as a success response,
    /// recording how many requests it observed in a shared counter the caller
    /// can inspect.
    fn spawn_fake_host(name: &str) -> (PluginHost, Arc<AtomicUsize>) {
        let (host_reader, mut plugin_writer) = duplex(8192);
        let (plugin_reader, host_writer) = duplex(8192);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_for_task = counter.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(plugin_reader);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let request: RpcRequest = match serde_json::from_str(trimmed) {
                            Ok(value) => value,
                            Err(_) => continue,
                        };
                        if request.id.is_none() {
                            // Notification (e.g. `initialized`, `exit`); never echo.
                            continue;
                        }
                        counter_for_task.fetch_add(1, AtomicOrdering::SeqCst);
                        let response =
                            RpcResponse::ok(request.id, json!({ "method": request.method, "echo": request.params }));
                        let mut encoded = serde_json::to_string(&response).expect("encode response");
                        encoded.push('\n');
                        if plugin_writer.write_all(encoded.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let host = PluginHost::from_streams(name.to_string(), host_reader, host_writer);
        (host, counter)
    }

    fn fresh_backend() -> PluginSessionBackend {
        // binary_path is never spawned in these tests because we exclusively
        // exercise dispatch_cancel against pre-inserted handles. Any path works.
        PluginSessionBackend::new("test-plugin", PathBuf::from("/nonexistent/test-plugin"), "test")
    }

    #[tokio::test]
    async fn cancel_unknown_session_returns_not_found() {
        let backend = fresh_backend();
        let err = backend.dispatch_cancel("never-existed").await.expect_err("cancel must error");
        let message = format!("{err}");
        assert!(
            message.contains("no active session") && message.contains("never-existed"),
            "unexpected error message: {message}"
        );
    }

    #[tokio::test]
    async fn cancel_routes_through_existing_host() {
        let backend = fresh_backend();
        let (host, request_counter) = spawn_fake_host("test-plugin");
        // Sanity: brand-new fake host has not seen any requests yet.
        assert_eq!(request_counter.load(AtomicOrdering::SeqCst), 0);

        backend.insert_session_for_test(
            "session-1",
            SessionHandle { host, started_at: Instant::now(), cancellation: true },
        );

        backend.dispatch_cancel("session-1").await.expect("cancel must succeed against the fake host");

        // The fake host's request counter went up by exactly one (the
        // agent/cancel call). If dispatch_cancel had spawned a NEW host and
        // bypassed our session, the counter would still be zero.
        assert_eq!(
            request_counter.load(AtomicOrdering::SeqCst),
            1,
            "cancel must route through the registered host, not a fresh transport"
        );
    }

    #[tokio::test]
    async fn cancel_removes_session_from_map_on_success() {
        let backend = fresh_backend();
        let (host, _counter) = spawn_fake_host("test-plugin");
        backend.insert_session_for_test(
            "session-2",
            SessionHandle { host, started_at: Instant::now(), cancellation: true },
        );
        assert!(backend.has_session_for_test("session-2"), "precondition: session must be registered");

        backend.dispatch_cancel("session-2").await.expect("cancel must succeed");

        assert!(
            !backend.has_session_for_test("session-2"),
            "session must be removed from the map after a successful cancel"
        );
    }

    #[tokio::test]
    async fn cancel_rejects_when_plugin_lacks_capability() {
        let backend = fresh_backend();
        let (host, request_counter) = spawn_fake_host("test-plugin");
        backend.insert_session_for_test(
            "session-3",
            SessionHandle { host, started_at: Instant::now(), cancellation: false },
        );

        let err = backend.dispatch_cancel("session-3").await.expect_err("cancel must error");
        // Callers should be able to pattern-match the typed variant.
        match &err {
            Error::CapabilityNotSupported { capability, .. } => {
                assert_eq!(capability, "cancellation");
            }
            Error::Upstream(other) => panic!("expected CapabilityNotSupported, got upstream: {other:?}"),
        }
        // And the Display impl is still descriptive for human-readable logs.
        let message = format!("{err}");
        assert!(message.contains("does not support capability 'cancellation'"), "unexpected error message: {message}");
        // The fake plugin must NOT have been called at all.
        assert_eq!(
            request_counter.load(AtomicOrdering::SeqCst),
            0,
            "cancel must short-circuit before issuing an RPC when capability is unset"
        );
    }
}
