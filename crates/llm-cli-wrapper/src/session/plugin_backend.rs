//! `SessionBackend` adapter that dispatches `agent/run` to a discovered AO STDIO plugin.
//!
//! Each call spawns the plugin binary, completes the handshake, sends an `agent/run`
//! request, drains the response into a single `SessionEvent::FinalText` followed by
//! `Finished`, and shuts the plugin down. Lightweight per-call lifecycle keeps the
//! v1 surface simple; pooling can come later.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orchestrator_plugin_host::PluginHost;
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
    session_backend_kind::SessionBackendKind, session_capabilities::SessionCapabilities,
    session_event::SessionEvent, session_request::SessionRequest, session_run::SessionRun,
    session_stability::SessionStability,
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
}

impl PluginSessionBackend {
    pub fn new(
        plugin_name: impl Into<String>,
        binary_path: PathBuf,
        provider_tool: impl Into<String>,
    ) -> Self {
        let plugin_name = plugin_name.into();
        let provider_tool = provider_tool.into();
        let display_name = format!("Plugin Provider ({plugin_name})");
        Self { plugin_name, binary_path, provider_tool, display_name }
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
        for key in [
            "system_prompt",
            "claude_profile",
            "mcp_servers",
            "tools",
            "response_schema",
            "runtime_contract",
        ] {
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

        let mut host = PluginHost::spawn(&self.binary_path, &[]).await.map_err(|error| {
            Error::ExecutionFailed(format!("plugin '{}' spawn failed: {error}", self.plugin_name))
        })?;
        if let Err(error) = host.handshake().await {
            graceful_shutdown(host).await;
            return Err(Error::ExecutionFailed(format!(
                "plugin '{}' handshake failed: {error}",
                self.plugin_name
            )));
        }

        let request_future = host.request(method.to_string(), Some(params));
        let response = match tokio::time::timeout(run_timeout, request_future).await {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => {
                graceful_shutdown(host).await;
                return Err(Error::ExecutionFailed(format!(
                    "plugin '{}' {method} failed ({}): {}",
                    self.plugin_name, error.code, error.message
                )));
            }
            Err(_) => {
                graceful_shutdown(host).await;
                return Err(Error::ExecutionFailed(format!(
                    "plugin '{}' {method} timed out after {}s",
                    self.plugin_name,
                    run_timeout.as_secs()
                )));
            }
        };
        graceful_shutdown(host).await;

        let session_id = response
            .get("session_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or(Some(control_session_id));
        let exit_code = response.get("exit_code").and_then(Value::as_i64).map(|n| n as i32);
        let output = response.get("output").and_then(Value::as_str).unwrap_or_default().to_string();
        let metadata = response.get("metadata").cloned().unwrap_or(Value::Null);

        let (tx, rx) = mpsc::channel(16);
        let plugin_label = backend_label.clone();
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            let _ = tx
                .send(SessionEvent::Started {
                    backend: plugin_label.clone(),
                    session_id: session_id_clone.clone(),
                    pid: None,
                })
                .await;
            if !output.is_empty() {
                let _ = tx.send(SessionEvent::FinalText { text: output }).await;
            }
            if !matches!(metadata, Value::Null) {
                let _ = tx.send(SessionEvent::Metadata { metadata }).await;
            }
            let _ = tx.send(SessionEvent::Finished { exit_code }).await;
        });

        Ok(SessionRun {
            session_id,
            events: rx,
            selected_backend: backend_label,
            fallback_reason: None,
            pid: None,
        })
    }

    async fn dispatch_cancel(&self, session_id: &str) -> Result<()> {
        let cancel_timeout = Duration::from_secs(DEFAULT_PLUGIN_CANCEL_TIMEOUT_SECS);
        let mut host = PluginHost::spawn(&self.binary_path, &[]).await.map_err(|error| {
            Error::ExecutionFailed(format!("plugin '{}' spawn failed: {error}", self.plugin_name))
        })?;
        if let Err(error) = host.handshake().await {
            graceful_shutdown(host).await;
            return Err(Error::ExecutionFailed(format!(
                "plugin '{}' handshake failed: {error}",
                self.plugin_name
            )));
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

/// Snapshot of a discovered provider plugin used to lazily build a `PluginSessionBackend`.
#[derive(Debug, Clone)]
pub struct DiscoveredProviderPlugin {
    pub plugin_name: String,
    pub provider_tool: String,
    pub binary_path: PathBuf,
}

impl DiscoveredProviderPlugin {
    pub fn into_backend(self) -> Arc<PluginSessionBackend> {
        Arc::new(PluginSessionBackend::new(self.plugin_name, self.binary_path, self.provider_tool))
    }
}

/// Inspect manifests of every discovered plugin under `project_root` and return only
/// the provider-kind plugins. Provider tool name defaults to the plugin name minus the
/// `ao-provider-` prefix.
pub fn discover_provider_plugins(project_root: &std::path::Path) -> Vec<DiscoveredProviderPlugin> {
    use orchestrator_plugin_host::discover_plugins;
    discover_plugins(project_root)
        .unwrap_or_default()
        .into_iter()
        .filter(|plugin| plugin.manifest.plugin_kind == orchestrator_plugin_protocol::PLUGIN_KIND_PROVIDER)
        .map(|plugin| {
            let provider_tool = plugin
                .name
                .strip_prefix("ao-provider-")
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| plugin.name.clone());
            DiscoveredProviderPlugin { plugin_name: plugin.name, provider_tool, binary_path: plugin.path }
        })
        .collect()
}
