//! `SessionBackend` adapter that dispatches `agent/run` to a discovered AO STDIO plugin.
//!
//! Each call spawns the plugin binary, completes the handshake, sends an `agent/run`
//! request, drains the response into a single `SessionEvent::FinalText` followed by
//! `Finished`, and shuts the plugin down. Lightweight per-call lifecycle keeps the
//! v1 surface simple; pooling can come later.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use animus_plugin_protocol::RpcNotification;
use async_trait::async_trait;
use orchestrator_logging::Logger;
use orchestrator_plugin_host::{PluginHost, PluginStderrSink};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Default cap on how long the plugin host will wait for a single `agent/run`
/// or `agent/resume` reply before giving up. Caller-supplied `timeout_secs` on
/// the SessionRequest takes precedence.
const DEFAULT_PLUGIN_RUN_TIMEOUT_SECS: u64 = 1_800; // 30 min
/// Default cap for the synchronous `agent/cancel` round trip.
const DEFAULT_PLUGIN_CANCEL_TIMEOUT_SECS: u64 = 10;
/// Wait at most this long for the plugin to gracefully shut down before we
/// drop its connection.
const PLUGIN_SHUTDOWN_TIMEOUT_SECS: u64 = 5;

use super::{
    session_backend::SessionBackend, session_backend_info::SessionBackendInfo,
    session_backend_kind::SessionBackendKind, session_capabilities::SessionCapabilities, session_event::SessionEvent,
    session_request::SessionRequest, session_run::SessionRun, session_stability::SessionStability,
};
use crate::error::{Error, Result};

/// Wraps a discovered AO STDIO plugin so the resolver can route `agent/run`
/// through it as if it were any other in-tree backend.
#[derive(Clone)]
pub struct PluginSessionBackend {
    pub(crate) plugin_name: String,
    pub(crate) binary_path: PathBuf,
    pub(crate) provider_tool: String,
    pub(crate) display_name: String,
    pub(crate) project_root: Option<PathBuf>,
}

impl PluginSessionBackend {
    pub fn new(plugin_name: impl Into<String>, binary_path: PathBuf, provider_tool: impl Into<String>) -> Self {
        let plugin_name = plugin_name.into();
        let provider_tool = provider_tool.into();
        let display_name = format!("Plugin Provider ({plugin_name})");
        Self { plugin_name, binary_path, provider_tool, display_name, project_root: None }
    }

    /// Bind a project root so structured log entries about every spawn / call /
    /// stderr line land in `~/.animus/<repo-scope>/logs/events.jsonl` for that project.
    #[must_use]
    pub fn with_project_root(mut self, project_root: impl Into<PathBuf>) -> Self {
        self.project_root = Some(project_root.into());
        self
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

        let mut host = match PluginHost::spawn_with_stderr(&self.binary_path, &[], self.stderr_sink_for()).await {
            Ok(host) => host,
            Err(error) => {
                let message = format!("plugin '{}' spawn failed: {error}", self.plugin_name);
                if let Some(logger) = self.project_logger() {
                    logger.error("plugin.dispatch.spawn", &message).err(error.to_string()).emit();
                }
                return Err(Error::ExecutionFailed(message));
            }
        };

        // Subscribe before handshake so even handshake-time notifications are visible.
        let mut notifications = host.subscribe_notifications(64);

        if let Err(error) = host.handshake().await {
            graceful_shutdown(host).await;
            let message = format!("plugin '{}' handshake failed: {error}", self.plugin_name);
            if let Some(logger) = self.project_logger() {
                logger.error("plugin.dispatch.handshake", &message).err(error.to_string()).emit();
            }
            return Err(Error::ExecutionFailed(message));
        }

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
        tokio::spawn(async move {
            // Forward notifications until the request future completes. Scope the
            // borrow of `host` so we can call shutdown on it after the request
            // future resolves.
            let response = {
                let mut request_future = Box::pin(host.request(method_string.clone(), Some(params)));
                let timeout_future = tokio::time::sleep(run_timeout);
                tokio::pin!(timeout_future);
                loop {
                    tokio::select! {
                        biased;
                        Some(notification) = notifications.recv() => {
                            forward_plugin_notification(&plugin_name_for_task, notification, &stream_tx).await;
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

            // Drain any straggling notifications before producing the final frames.
            while let Ok(notification) = notifications.try_recv() {
                forward_plugin_notification(&plugin_name_for_task, notification, &stream_tx).await;
            }

            let logger = project_root_for_task.as_ref().map(|root| Logger::for_project(root));
            let duration_ms = started_at.elapsed().as_millis() as u64;

            let outcome = match response {
                Some(Ok(value)) => Some(value),
                Some(Err(error)) => {
                    let message = format!(
                        "plugin '{}' {method_string} failed ({}): {}",
                        plugin_name_for_task, error.code, error.message
                    );
                    let _ = stream_tx.send(SessionEvent::Error { message: message.clone(), recoverable: false }).await;
                    if let Some(logger) = logger.as_ref() {
                        logger
                            .error("plugin.dispatch.error", &message)
                            .duration(duration_ms)
                            .meta(json!({ "plugin": plugin_name_for_task, "method": method_string }))
                            .emit();
                    }
                    let _ = stream_tx.send(SessionEvent::Finished { exit_code: Some(1) }).await;
                    graceful_shutdown(host).await;
                    return;
                }
                None => {
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
                    graceful_shutdown(host).await;
                    return;
                }
            };

            graceful_shutdown(host).await;

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

    async fn dispatch_cancel(&self, session_id: &str) -> Result<()> {
        let cancel_timeout = Duration::from_secs(DEFAULT_PLUGIN_CANCEL_TIMEOUT_SECS);
        if let Some(logger) = self.project_logger() {
            logger
                .info("plugin.cancel", format!("{} → cancel {}", self.plugin_name, session_id))
                .meta(json!({ "plugin": self.plugin_name, "session_id": session_id }))
                .emit();
        }
        let mut host = PluginHost::spawn_with_stderr(&self.binary_path, &[], self.stderr_sink_for())
            .await
            .map_err(|error| Error::ExecutionFailed(format!("plugin '{}' spawn failed: {error}", self.plugin_name)))?;
        if let Err(error) = host.handshake().await {
            graceful_shutdown(host).await;
            return Err(Error::ExecutionFailed(format!("plugin '{}' handshake failed: {error}", self.plugin_name)));
        }
        let request_future = host.request("agent/cancel".to_string(), Some(json!({ "session_id": session_id })));
        let result = tokio::time::timeout(cancel_timeout, request_future).await;
        graceful_shutdown(host).await;
        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(Error::ExecutionFailed(format!(
                "plugin '{}' agent/cancel failed: {}",
                self.plugin_name, error.message
            ))),
            Err(_) => Err(Error::ExecutionFailed(format!(
                "plugin '{}' agent/cancel timed out after {}s",
                self.plugin_name,
                cancel_timeout.as_secs()
            ))),
        }
    }
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

    async fn start_session(&self, request: SessionRequest) -> Result<SessionRun> {
        self.dispatch("agent/run", request, None).await
    }

    async fn resume_session(&self, request: SessionRequest, session_id: &str) -> Result<SessionRun> {
        self.dispatch("agent/resume", request, Some(session_id.to_string())).await
    }

    async fn terminate_session(&self, session_id: &str) -> Result<()> {
        self.dispatch_cancel(session_id).await
    }
}

async fn graceful_shutdown(host: PluginHost) {
    let _ = tokio::time::timeout(Duration::from_secs(PLUGIN_SHUTDOWN_TIMEOUT_SECS), host.shutdown()).await;
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
}

impl DiscoveredProviderPlugin {
    pub fn into_backend(self) -> Arc<PluginSessionBackend> {
        let mut backend = PluginSessionBackend::new(self.plugin_name, self.binary_path, self.provider_tool);
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
            DiscoveredProviderPlugin {
                plugin_name: plugin.name,
                provider_tool,
                binary_path: plugin.path,
                project_root: Some(project_root.clone()),
            }
        })
        .collect()
}
