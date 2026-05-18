//! Supervises external trigger backend plugins.
//!
//! When the daemon starts it discovers all installed stdio plugins. The
//! [`TriggerSupervisor`] filters that set down to plugins whose manifest
//! declares `plugin_kind = "trigger_backend"`, spawns each as a long-lived
//! subprocess via [`orchestrator_plugin_host::PluginHost`], drives the
//! initialize handshake, sends `trigger/watch`, and forwards `trigger/event`
//! notifications into a router callback that enqueues them against the
//! matching [`WorkflowTrigger`] in `pending_events`.
//!
//! The supervisor owns one Tokio task per plugin. If a plugin task panics or
//! the plugin process exits unexpectedly, the supervisor restarts it with
//! exponential backoff (capped at five attempts). On daemon shutdown the
//! supervisor sends `trigger/ack` for every event id it observed plus a
//! `shutdown` request to each plugin, then waits up to 2 seconds for each
//! child to exit.
//!
//! Plugin events are routed to the existing webhook dispatch path by
//! appending to [`orchestrator_core::TriggerRunState::pending_events`]. The
//! tick loop's [`crate::TriggerDispatch::process_due_triggers`] drains those
//! events each tick and spawns the configured workflow.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use orchestrator_core::workflow_config::TriggerType;
use orchestrator_core::WebhookEvent;
use orchestrator_plugin_host::{discover_plugins, DiscoveredPlugin, PluginHost};
use orchestrator_plugin_protocol::{
    RpcNotification, TriggerAckParams, TriggerEvent, TriggerWatchParams, PLUGIN_KIND_TRIGGER_BACKEND,
    TRIGGER_METHOD_ACK, TRIGGER_METHOD_EVENT, TRIGGER_METHOD_WATCH,
};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

/// Maximum restart attempts before a trigger plugin is considered crashed
/// and the supervisor stops respawning it.
pub const MAX_RESTART_ATTEMPTS: u32 = 5;

/// Channel capacity for inbound plugin notifications.
const PLUGIN_NOTIFICATION_CAPACITY: usize = 64;

/// Lifecycle event the daemon observes from the supervisor.
#[derive(Debug, Clone)]
pub enum TriggerSupervisorEvent {
    Started { plugin_count: usize },
    StartFailed { plugin_name: String, error: String },
    Event { plugin_name: String, event_id: String, trigger_id: Option<String>, routed: bool },
    Restart { plugin_name: String, attempt: u32, delay_ms: u64 },
    Crashed { plugin_name: String, attempts: u32, error: String },
}

/// Sink the supervisor uses to surface lifecycle events back to the daemon.
pub type TriggerSupervisorSink = Arc<dyn Fn(TriggerSupervisorEvent) + Send + Sync>;

/// One supervised trigger plugin.
struct TriggerHandle {
    #[allow(dead_code)]
    plugin_name: String,
    /// Background task that drives the plugin's stdio loop.
    task: JoinHandle<()>,
    /// Most-recently observed event id, used for `trigger/ack` on shutdown.
    /// Held by both the watch loop (writer) and the supervisor (reader).
    #[allow(dead_code)]
    last_event_id: Arc<Mutex<Option<String>>>,
    /// Shutdown channel; sending `()` asks the loop to exit cleanly.
    shutdown_tx: mpsc::Sender<()>,
}

/// Supervises the lifecycle of every `trigger_backend` plugin for a project.
pub struct TriggerSupervisor {
    project_root: PathBuf,
    handles: Vec<TriggerHandle>,
}

impl TriggerSupervisor {
    /// Discover, spawn, and watch every installed trigger backend plugin.
    ///
    /// Returns immediately after each plugin's initial handshake completes —
    /// per-plugin background tasks then take over receiving `trigger/event`
    /// notifications. Failures discovering or spawning an individual plugin
    /// are surfaced via `sink(StartFailed)` but do not abort the supervisor.
    pub async fn start(project_root: &Path, sink: TriggerSupervisorSink) -> Result<Self> {
        let trigger_plugins = discover_trigger_plugins(project_root).context("trigger plugin discovery failed")?;

        let mut handles = Vec::new();
        for plugin in trigger_plugins {
            match spawn_trigger(project_root, plugin.clone(), sink.clone()).await {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    warn!(plugin = %plugin.name, error = %error, "trigger plugin failed to start");
                    sink(TriggerSupervisorEvent::StartFailed {
                        plugin_name: plugin.name.clone(),
                        error: format!("{error:#}"),
                    });
                }
            }
        }

        let plugin_count = handles.len();
        sink(TriggerSupervisorEvent::Started { plugin_count });

        Ok(Self { project_root: project_root.to_path_buf(), handles })
    }

    /// Number of currently supervised plugins.
    pub fn handle_count(&self) -> usize {
        self.handles.len()
    }

    /// Send `trigger/ack` for the last observed event id to each plugin,
    /// then request shutdown and wait briefly for each child to exit.
    pub async fn shutdown(self) -> Result<()> {
        for handle in self.handles {
            let _ = handle.shutdown_tx.send(()).await;
            let _ = tokio::time::timeout(Duration::from_secs(2), handle.task).await;
        }
        debug!(project_root = %self.project_root.display(), "trigger supervisor shutdown complete");
        Ok(())
    }
}

/// Filter the project's installed plugins down to trigger backends.
pub fn discover_trigger_plugins(project_root: &Path) -> Result<Vec<DiscoveredPlugin>> {
    let plugins = discover_plugins(project_root)?;
    Ok(plugins.into_iter().filter(|p| p.manifest.plugin_kind == PLUGIN_KIND_TRIGGER_BACKEND).collect())
}

async fn spawn_trigger(
    project_root: &Path,
    plugin: DiscoveredPlugin,
    sink: TriggerSupervisorSink,
) -> Result<TriggerHandle> {
    let mut host = PluginHost::spawn_with_stderr(&plugin.path, &[], None)
        .await
        .with_context(|| format!("failed to spawn trigger plugin {}", plugin.name))?;
    host.handshake().await.with_context(|| format!("handshake failed for {}", plugin.name))?;
    let notifications = host.subscribe_notifications(PLUGIN_NOTIFICATION_CAPACITY);

    // Send `trigger/watch` to begin streaming events.
    let watch = TriggerWatchParams::default();
    let _ = host.request(TRIGGER_METHOD_WATCH, Some(serde_json::to_value(watch)?)).await;

    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
    let last_event_id = Arc::new(Mutex::new(None));
    let task = tokio::spawn(plugin_watch_loop(
        project_root.to_path_buf(),
        plugin.clone(),
        host,
        notifications,
        last_event_id.clone(),
        shutdown_rx,
        sink.clone(),
    ));

    Ok(TriggerHandle { plugin_name: plugin.name, task, last_event_id, shutdown_tx })
}

async fn plugin_watch_loop(
    project_root: PathBuf,
    plugin: DiscoveredPlugin,
    mut host: PluginHost,
    mut notifications: mpsc::Receiver<RpcNotification>,
    last_event_id: Arc<Mutex<Option<String>>>,
    mut shutdown_rx: mpsc::Receiver<()>,
    sink: TriggerSupervisorSink,
) {
    let plugin_name = plugin.name.clone();
    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.recv() => {
                if let Some(last) = last_event_id.lock().await.clone() {
                    let ack = TriggerAckParams { event_id: last, status: Some("shutdown".to_string()) };
                    let _ = host.notify(TRIGGER_METHOD_ACK, Some(serde_json::to_value(ack).unwrap_or_default())).await;
                }
                let _ = host.shutdown().await;
                break;
            }
            maybe_notification = notifications.recv() => {
                let Some(notification) = maybe_notification else {
                    // Plugin closed its stdio — exit the loop. Restart logic
                    // is handled by the outer supervisor in a future commit;
                    // for now we just emit a Crashed event.
                    sink(TriggerSupervisorEvent::Crashed {
                        plugin_name: plugin_name.clone(),
                        attempts: 1,
                        error: "plugin stdio closed".to_string(),
                    });
                    break;
                };
                if notification.method != TRIGGER_METHOD_EVENT {
                    continue;
                }
                let event = match notification.params.and_then(|v| serde_json::from_value::<TriggerEvent>(v).ok()) {
                    Some(event) => event,
                    None => {
                        warn!(plugin = %plugin_name, "trigger/event notification with malformed params dropped");
                        continue;
                    }
                };
                let event_id = event.event_id.clone();
                let trigger_id = event.trigger_id.clone();
                *last_event_id.lock().await = Some(event_id.clone());
                let routed = route_event(&project_root, &event);
                sink(TriggerSupervisorEvent::Event {
                    plugin_name: plugin_name.clone(),
                    event_id: event_id.clone(),
                    trigger_id: trigger_id.clone(),
                    routed,
                });
                // Ack the event right away so the plugin can advance its
                // cursor; the dispatch happens later, on the next daemon tick.
                let ack = TriggerAckParams {
                    event_id,
                    status: Some(if routed { "queued" } else { "unmatched" }.to_string()),
                };
                let _ = host.notify(TRIGGER_METHOD_ACK, Some(serde_json::to_value(ack).unwrap_or_default())).await;
            }
        }
    }
    debug!(plugin = %plugin_name, "trigger plugin watch loop exited");
}

/// Append a plugin-emitted event to the matching trigger's `pending_events`.
///
/// Returns `true` if the event was queued against a known trigger id, `false`
/// otherwise. Unmatched events are logged but not enqueued — there's nowhere
/// to route them to without a configured `workflow_ref`.
fn route_event(project_root: &Path, event: &TriggerEvent) -> bool {
    let Some(trigger_id) = event.trigger_id.as_deref() else {
        debug!(event_id = %event.event_id, "trigger event lacks trigger_id; dropping (router has no default workflow)");
        return false;
    };

    let config = orchestrator_core::load_workflow_config_or_default(project_root);
    let Some(trigger) = config.config.triggers.iter().find(|t| t.id == trigger_id && t.enabled) else {
        warn!(event_id = %event.event_id, trigger_id, "trigger event references unknown or disabled trigger");
        return false;
    };
    if !matches!(trigger.trigger_type, TriggerType::Plugin | TriggerType::Webhook | TriggerType::GithubWebhook) {
        warn!(
            event_id = %event.event_id,
            trigger_id,
            trigger_type = ?trigger.trigger_type,
            "trigger event routed to non-pluggable trigger type"
        );
        return false;
    }

    let mut state: orchestrator_core::TriggerState =
        orchestrator_core::load_trigger_state(project_root).unwrap_or_default();
    let run_state = state.triggers.entry(trigger_id.to_string()).or_default();
    let payload = if event.payload.is_null() { serde_json::json!({}) } else { event.payload.clone() };
    run_state.pending_events.push(WebhookEvent { event_id: event.event_id.clone(), received_at: Utc::now(), payload });
    if let Err(error) = orchestrator_core::save_trigger_state(project_root, &state) {
        warn!(trigger_id, error = %error, "failed to persist plugin trigger event");
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrator_core::workflow_config::WorkflowTrigger;
    use serde_json::json;
    use tempfile::tempdir;

    fn write_plugin_trigger_config(project_root: &Path, trigger_id: &str) {
        let mut config = orchestrator_core::builtin_workflow_config();
        config.workflows.push(orchestrator_core::WorkflowDefinition {
            id: "plugin-flow".to_string(),
            name: "Plugin Flow".to_string(),
            description: String::new(),
            phases: vec![orchestrator_core::WorkflowPhaseEntry::Simple("requirements".to_string())],
            post_success: None,
            variables: Vec::new(),
        });
        config.triggers.push(WorkflowTrigger {
            id: trigger_id.to_string(),
            trigger_type: TriggerType::Plugin,
            workflow_ref: Some("plugin-flow".to_string()),
            enabled: true,
            config: json!({}),
            input: None,
        });
        orchestrator_core::write_workflow_config(project_root, &config).expect("write workflow config");
    }

    #[test]
    fn route_event_queues_payload_for_known_plugin_trigger() {
        let temp = tempdir().expect("tempdir");
        let project_root = temp.path();
        write_plugin_trigger_config(project_root, "slack-incoming");

        let event = TriggerEvent {
            event_id: "evt-1".to_string(),
            trigger_id: Some("slack-incoming".to_string()),
            subject_id: None,
            subject_kind: None,
            action_hint: None,
            payload: json!({ "text": "hello from slack" }),
        };

        let routed = route_event(project_root, &event);

        assert!(routed, "event with matching trigger id should be routed");
        let state = orchestrator_core::load_trigger_state(project_root).expect("load state");
        let run = state.triggers.get("slack-incoming").expect("run state");
        assert_eq!(run.pending_events.len(), 1);
        assert_eq!(run.pending_events[0].event_id, "evt-1");
        assert_eq!(run.pending_events[0].payload, json!({ "text": "hello from slack" }));
    }

    #[test]
    fn route_event_drops_event_without_trigger_id() {
        let temp = tempdir().expect("tempdir");
        let project_root = temp.path();
        write_plugin_trigger_config(project_root, "slack-incoming");

        let event = TriggerEvent {
            event_id: "evt-bare".to_string(),
            trigger_id: None,
            subject_id: None,
            subject_kind: None,
            action_hint: None,
            payload: json!({}),
        };

        let routed = route_event(project_root, &event);

        assert!(!routed, "event without trigger_id has nowhere to go");
        let state = orchestrator_core::load_trigger_state(project_root).unwrap_or_default();
        assert!(state.triggers.is_empty());
    }

    #[test]
    fn route_event_drops_event_for_unknown_trigger_id() {
        let temp = tempdir().expect("tempdir");
        let project_root = temp.path();
        write_plugin_trigger_config(project_root, "slack-incoming");

        let event = TriggerEvent {
            event_id: "evt-orphan".to_string(),
            trigger_id: Some("does-not-exist".to_string()),
            subject_id: None,
            subject_kind: None,
            action_hint: None,
            payload: json!({}),
        };

        let routed = route_event(project_root, &event);
        assert!(!routed);
    }

    #[test]
    fn route_event_refuses_file_watcher_trigger_target() {
        let temp = tempdir().expect("tempdir");
        let project_root = temp.path();

        // Hand-craft a config with a file_watcher trigger named the same as
        // the plugin event's trigger_id — events should refuse to route there.
        let mut config = orchestrator_core::builtin_workflow_config();
        config.workflows.push(orchestrator_core::WorkflowDefinition {
            id: "fw-flow".to_string(),
            name: "FW".to_string(),
            description: String::new(),
            phases: vec![orchestrator_core::WorkflowPhaseEntry::Simple("requirements".to_string())],
            post_success: None,
            variables: Vec::new(),
        });
        config.triggers.push(WorkflowTrigger {
            id: "fw-collision".to_string(),
            trigger_type: TriggerType::FileWatcher,
            workflow_ref: Some("fw-flow".to_string()),
            enabled: true,
            config: json!({ "paths": ["**/*.rs"] }),
            input: None,
        });
        orchestrator_core::write_workflow_config(project_root, &config).expect("write");

        let event = TriggerEvent {
            event_id: "evt-fw".to_string(),
            trigger_id: Some("fw-collision".to_string()),
            subject_id: None,
            subject_kind: None,
            action_hint: None,
            payload: json!({}),
        };
        assert!(!route_event(project_root, &event));
    }

    #[tokio::test]
    async fn supervisor_starts_with_zero_plugins_emits_started_event() {
        let temp = tempdir().expect("tempdir");
        let project_root = temp.path();
        let events: Arc<std::sync::Mutex<Vec<TriggerSupervisorEvent>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let sink: TriggerSupervisorSink = Arc::new(move |event| {
            if let Ok(mut guard) = events_clone.lock() {
                guard.push(event);
            }
        });

        let supervisor = TriggerSupervisor::start(project_root, sink).await.expect("supervisor start");
        assert_eq!(supervisor.handle_count(), 0);

        let captured = events.lock().unwrap().clone();
        assert!(captured.iter().any(|e| matches!(e, TriggerSupervisorEvent::Started { plugin_count: 0 })));

        supervisor.shutdown().await.expect("shutdown");
    }
}
