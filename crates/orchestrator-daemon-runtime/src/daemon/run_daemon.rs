use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::RwLock;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use orchestrator_core::DaemonStatus;
use tokio::time::sleep;
use workflow_runner_v2::workflow_event_emitter::SharedWorkflowEventEmitter;

use crate::control::{BroadcastWorkflowEventEmitter, WorkflowEventBroadcaster};
use crate::run_plugin_preflight;
use crate::run_project_tick;
use crate::DaemonRunEvent;
use crate::DaemonRunGuard;
use crate::DaemonRunHooks;
use crate::DaemonRuntimeOptions;
use crate::DaemonRuntimeState;
use crate::DiscoveredPluginSummary;
use crate::ProjectTickHooks;
use crate::ProjectTickRunMode;
use crate::TriggerSupervisor;
use crate::TriggerSupervisorEvent;
use crate::TriggerSupervisorSink;

/// Process-global holder for the daemon's broadcast-backed
/// [`SharedWorkflowEventEmitter`]. Installed by [`run_daemon`] at startup
/// (after [`WorkflowEventBroadcaster`] construction) and consumed by any
/// in-process call site that builds [`workflow_runner_v2::WorkflowExecuteParams`]
/// inside the daemon process.
///
/// SUBPROCESS GAP: workflow runs launched via `ao-workflow-runner` (the
/// scheduler's normal path) live in a separate process and cannot see this
/// holder. They emit no workflow_events. A subprocess back-channel
/// (per-run pipe / event log tail) is required for full coverage and is
/// scheduled for v0.5.
static DAEMON_WORKFLOW_EVENT_EMITTER: OnceLock<RwLock<Option<SharedWorkflowEventEmitter>>> = OnceLock::new();

/// Process-global slot for the daemon's [`WorkflowEventBroadcaster`]. This
/// is the *broadcaster* itself (not the trait-obj emitter), exposed so the
/// scheduler's subprocess spawn path can attach a per-run back-channel
/// reader without needing to downcast through
/// [`SharedWorkflowEventEmitter`]. Lifecycle: installed alongside the
/// emitter in [`run_daemon`]; cleared in
/// [`clear_workflow_event_emitter`].
static DAEMON_WORKFLOW_EVENT_BROADCASTER: OnceLock<RwLock<Option<Arc<WorkflowEventBroadcaster>>>> = OnceLock::new();

fn emitter_slot() -> &'static RwLock<Option<SharedWorkflowEventEmitter>> {
    DAEMON_WORKFLOW_EVENT_EMITTER.get_or_init(|| RwLock::new(None))
}

fn broadcaster_slot() -> &'static RwLock<Option<Arc<WorkflowEventBroadcaster>>> {
    DAEMON_WORKFLOW_EVENT_BROADCASTER.get_or_init(|| RwLock::new(None))
}

/// Returns the daemon's `WorkflowEventBroadcaster` when one has been
/// installed by [`run_daemon`]. Subprocess-dispatch callers use this to
/// attach a per-run pipe reader that forwards subprocess workflow_events
/// into the broadcaster. Returns `None` from CLI / one-shot processes
/// that never started a daemon.
pub fn current_workflow_event_broadcaster() -> Option<Arc<WorkflowEventBroadcaster>> {
    broadcaster_slot().read().ok().and_then(|guard| guard.clone())
}

fn install_workflow_event_broadcaster(broadcaster: Arc<WorkflowEventBroadcaster>) {
    if let Ok(mut guard) = broadcaster_slot().write() {
        *guard = Some(broadcaster);
    }
}

fn clear_workflow_event_broadcaster() {
    if let Ok(mut guard) = broadcaster_slot().write() {
        *guard = None;
    }
}

/// Returns the process-global daemon workflow event emitter when one has
/// been installed by [`run_daemon`]. Returns `None` when called from a
/// process that hasn't started the daemon (CLI one-shot commands,
/// `ao-workflow-runner` subprocess, etc.) — callers should default to a
/// noop emitter in that case.
pub fn current_workflow_event_emitter() -> Option<SharedWorkflowEventEmitter> {
    emitter_slot().read().ok().and_then(|guard| guard.clone())
}

fn install_workflow_event_emitter(emitter: SharedWorkflowEventEmitter) {
    if let Ok(mut guard) = emitter_slot().write() {
        *guard = Some(emitter);
    }
}

fn clear_workflow_event_emitter() {
    if let Ok(mut guard) = emitter_slot().write() {
        *guard = None;
    }
}

pub async fn run_daemon<D, H>(
    project_root: &str,
    options: &mut DaemonRuntimeOptions,
    driver: &mut D,
    hooks: &mut H,
    mut active_process_count: impl FnMut(&D) -> usize,
) -> Result<()>
where
    D: ProjectTickHooks,
    H: DaemonRunHooks,
{
    let _run_guard = DaemonRunGuard::acquire(project_root)?;
    let daemon_pid = std::process::id();
    let primary_root = canonicalize_lossy(project_root);

    crate::metrics::install_workflow_runner_metrics_bridge();

    // Resolve and install the process-wide runtime quotas before any
    // subsystem that consults them (trigger backlog, subscriber buffers,
    // workflow concurrency, plugin process count). First-installer-wins:
    // tests that pre-install a tweaked quota set keep their values.
    crate::quotas::install_runtime_quotas(crate::quotas::RuntimeQuotas::from_env());

    // Wire the plugin host's spawn-site quota check into the runtime
    // quota counter. Without this install the plugin host falls back to
    // a no-op (no cap enforced); with it, every plugin spawn claims a
    // slot bounded by `RuntimeQuotas::plugin_process_max`.
    crate::quotas::install_runtime_quota_process_slot_factory();

    hooks.handle_event(DaemonRunEvent::Startup { project_root: primary_root.clone(), daemon_pid })?;

    // Preflight BEFORE flipping persisted daemon status to Running. A first-time
    // `animus daemon start` whose preflight fails must not leave behind a stale
    // "running" record that future `daemon status` calls report as live.
    let mut preflight_spec = hooks.plugin_preflight_spec();
    if options.auto_install_plugins {
        preflight_spec.auto_install = true;
    }
    let installer = hooks.plugin_installer();
    let preflight_outcome = run_plugin_preflight(
        project_root,
        &primary_root,
        preflight_spec,
        installer.as_deref(),
        options.skip_plugin_preflight,
        hooks,
    )
    .await?;
    if preflight_outcome.should_abort_startup() {
        let message = preflight_outcome.render_abort_message();
        return Err(anyhow::anyhow!("{message}"));
    }

    let initial_status = hooks.daemon_status(&primary_root).await?;
    let mut stop_daemon_on_exit = false;
    if !matches!(initial_status, DaemonStatus::Running | DaemonStatus::Paused) {
        hooks.start_daemon(&primary_root).await?;
        stop_daemon_on_exit = true;
    }
    let _ = DaemonRuntimeState::set_runtime_paused(project_root, false);

    hooks.handle_event(DaemonRunEvent::Status { project_root: primary_root.clone(), status: "running".to_string() })?;

    if options.startup_cleanup {
        hooks.handle_event(DaemonRunEvent::StartupCleanup { project_root: primary_root.clone() })?;

        let startup_orphans = hooks.recover_startup_orphans(&primary_root).await?;
        if startup_orphans > 0 {
            hooks.handle_event(DaemonRunEvent::OrphanDetection {
                project_root: primary_root.clone(),
                orphaned_workflows_recovered: startup_orphans,
            })?;
        }
    }

    discover_plugins_for_daemon(project_root, &primary_root, hooks)?;

    resolve_log_storage_dispatch_for_daemon(project_root, &primary_root, hooks);

    resolve_subject_dispatch_for_daemon(project_root, &primary_root, hooks).await;

    let workflow_event_broadcaster = WorkflowEventBroadcaster::new();
    install_workflow_event_emitter(BroadcastWorkflowEventEmitter::new(workflow_event_broadcaster.clone()));
    install_workflow_event_broadcaster(workflow_event_broadcaster.clone());

    let control_server_handle =
        start_control_server_for_daemon(project_root, &primary_root, hooks, workflow_event_broadcaster.clone()).await;

    // Trigger backend plugins. Off by default behind an env flag mirroring
    // the provider-plugin opt-out shape.
    let trigger_event_queue: Arc<Mutex<Vec<TriggerSupervisorEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let trigger_supervisor = if triggers_disabled() {
        None
    } else {
        let queue = trigger_event_queue.clone();
        let sink: TriggerSupervisorSink = Arc::new(move |event| {
            if let Ok(mut guard) = queue.lock() {
                guard.push(event);
            }
        });
        match TriggerSupervisor::start(Path::new(project_root), sink).await {
            Ok(supervisor) => Some(supervisor),
            Err(error) => {
                hooks.handle_event(DaemonRunEvent::TriggerPluginStartFailed {
                    project_root: primary_root.clone(),
                    plugin_name: "<supervisor>".to_string(),
                    error: format!("{error:#}"),
                })?;
                None
            }
        }
    };
    drain_trigger_events(&primary_root, &trigger_event_queue, hooks)?;

    match orchestrator_core::validate_and_compile_yaml_workflows(Path::new(project_root)) {
        Ok(Some(result)) => {
            hooks.handle_event(DaemonRunEvent::YamlCompileSucceeded {
                project_root: primary_root.clone(),
                source_files: result.source_files.len(),
                output_path: result.output_path.display().to_string(),
                phase_definitions: result.config.phase_definitions.len(),
                agent_profiles: result.config.agent_profiles.len(),
            })?;
        }
        Ok(None) => {}
        Err(error) => {
            hooks.handle_event(DaemonRunEvent::YamlCompileFailed {
                project_root: primary_root.clone(),
                error: error.to_string(),
            })?;
        }
    }

    let mut interval = Duration::from_secs(options.interval_secs.max(1));
    let mut sigterm_stream = SigtermStream::new()?;
    loop {
        // Hot-reload runtime-reconfigurable settings from persisted project config
        // so that `animus.daemon config-set` changes take effect without restart.
        let prev_interval = options.interval_secs;
        options.reload_from_project_config(Path::new(project_root));
        if options.interval_secs != prev_interval {
            interval = Duration::from_secs(options.interval_secs.max(1));
            hooks.handle_event(DaemonRunEvent::ConfigReloaded {
                project_root: primary_root.clone(),
                setting: "interval_secs".to_string(),
            })?;
        }

        let externally_paused = DaemonRuntimeState::is_runtime_paused(project_root).unwrap_or(false);
        let tick_result = run_project_tick(
            &primary_root,
            options,
            ProjectTickRunMode { active_process_count: active_process_count(driver) },
            externally_paused,
            driver,
        )
        .await;

        match tick_result {
            Ok(summary) => hooks.handle_event(DaemonRunEvent::TickSummary { summary })?,
            Err(error) => hooks.handle_event(DaemonRunEvent::TickError {
                project_root: primary_root.clone(),
                message: error.to_string(),
            })?,
        }

        if externally_paused {
            break;
        }

        drain_trigger_events(&primary_root, &trigger_event_queue, hooks)?;

        if let Err(error) = hooks.flush_notifications(&primary_root).await {
            hooks.handle_event(DaemonRunEvent::NotificationRuntimeError {
                project_root: Some(primary_root.clone()),
                stage: "flush".to_string(),
                message: error.to_string(),
            })?;
        }

        if options.once {
            break;
        }

        let shutdown = DaemonRuntimeState::is_shutdown_requested(project_root).unwrap_or((false, None));
        if shutdown.0 {
            hooks.handle_event(DaemonRunEvent::GracefulShutdown {
                project_root: primary_root.clone(),
                timeout_secs: shutdown.1,
            })?;
            let _ = DaemonRuntimeState::set_shutdown_requested(project_root, false, None);
            break;
        }

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                hooks.handle_event(DaemonRunEvent::Draining {
                    project_root: primary_root.clone(),
                    trigger: "ctrl_c".to_string(),
                })?;
                break;
            }
            _ = sigterm_stream.recv() => {
                hooks.handle_event(DaemonRunEvent::Draining {
                    project_root: primary_root.clone(),
                    trigger: "sigterm".to_string(),
                })?;
                break;
            }
            _ = sleep(interval) => {}
        }
    }

    if let Some(supervisor) = trigger_supervisor {
        let _ = supervisor.shutdown().await;
        drain_trigger_events(&primary_root, &trigger_event_queue, hooks)?;
    }

    if let Some(server) = control_server_handle {
        let _ = server.shutdown().await;
    }

    clear_workflow_event_emitter();
    clear_workflow_event_broadcaster();

    if stop_daemon_on_exit {
        let _ = hooks.stop_daemon(&primary_root).await;
    }

    hooks.handle_event(DaemonRunEvent::Status { project_root: primary_root.clone(), status: "stopped".to_string() })?;
    hooks.handle_event(DaemonRunEvent::Shutdown { project_root: primary_root, daemon_pid })?;
    Ok(())
}

fn triggers_disabled() -> bool {
    std::env::var("ANIMUS_DAEMON_DISABLE_TRIGGERS").map(|v| v != "0" && !v.is_empty()).unwrap_or(false)
}

fn drain_trigger_events<H: DaemonRunHooks>(
    primary_root: &str,
    queue: &Arc<Mutex<Vec<TriggerSupervisorEvent>>>,
    hooks: &mut H,
) -> Result<()> {
    let drained: Vec<TriggerSupervisorEvent> = {
        let mut guard = match queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        std::mem::take(&mut *guard)
    };
    for event in drained {
        let daemon_event = match event {
            TriggerSupervisorEvent::Started { plugin_count } => {
                DaemonRunEvent::TriggerPluginsStarted { project_root: primary_root.to_string(), plugin_count }
            }
            TriggerSupervisorEvent::StartFailed { plugin_name, error } => {
                crate::metrics::incr(&crate::metrics::labeled(
                    "plugin_start_failures_total",
                    &[("plugin", plugin_name.as_str())],
                ));
                DaemonRunEvent::TriggerPluginStartFailed { project_root: primary_root.to_string(), plugin_name, error }
            }
            TriggerSupervisorEvent::Event { plugin_name, event_id, trigger_id, routed } => {
                crate::metrics::incr(&crate::metrics::labeled(
                    "trigger_events_total",
                    &[("plugin", plugin_name.as_str()), ("routed", if routed { "true" } else { "false" })],
                ));
                DaemonRunEvent::TriggerPluginEvent {
                    project_root: primary_root.to_string(),
                    plugin_name,
                    event_id,
                    trigger_id,
                    routed,
                }
            }
            TriggerSupervisorEvent::Restart { plugin_name, attempt, delay_ms } => {
                crate::metrics::incr(&crate::metrics::labeled(
                    "plugin_restarts_total",
                    &[("plugin", plugin_name.as_str())],
                ));
                DaemonRunEvent::TriggerPluginRestart {
                    project_root: primary_root.to_string(),
                    plugin_name,
                    attempt,
                    delay_ms,
                }
            }
            TriggerSupervisorEvent::Crashed { plugin_name, attempts, error } => {
                crate::metrics::incr(&crate::metrics::labeled(
                    "plugin_disabled_total",
                    &[("plugin", plugin_name.as_str())],
                ));
                DaemonRunEvent::TriggerPluginCrashed {
                    project_root: primary_root.to_string(),
                    plugin_name,
                    attempts,
                    error,
                }
            }
        };
        hooks.handle_event(daemon_event)?;
    }
    Ok(())
}

/// Resolve which subject-backend plugins the daemon will route through
/// and emit a [`DaemonRunEvent::SubjectRouterResolved`] so operators see
/// the choice on every startup. Failures (discovery error, plugin spawn
/// failure, duplicate-kind collision) are degraded to an empty router
/// plus a warning rather than aborting startup — a misbehaving subject
/// plugin must never block the daemon from coming up. CLI `animus subject`
/// calls against unrouted kinds will surface `NotFound`.
async fn resolve_subject_dispatch_for_daemon<H: DaemonRunHooks>(project_root: &str, primary_root: &str, hooks: &mut H) {
    let disable_env_set = crate::subject_plugins_disable_env_set();
    match crate::resolve_subject_dispatch(Path::new(project_root)).await {
        Ok(resolution) => {
            let plugin_count = resolution.selected.plugin_count();
            let kinds = resolution.selected.kinds().to_vec();
            let _ = hooks.handle_event(DaemonRunEvent::SubjectRouterResolved {
                project_root: primary_root.to_string(),
                plugin_count,
                kinds,
                disable_env_set,
                warnings: resolution.warnings,
            });
        }
        Err(error) => {
            let _ = hooks.handle_event(DaemonRunEvent::SubjectRouterResolved {
                project_root: primary_root.to_string(),
                plugin_count: 0,
                kinds: Vec::new(),
                disable_env_set,
                warnings: vec![format!(
                    "subject_backend discovery failed; subject CLI calls will return NotFound: {error:#}"
                )],
            });
        }
    }
}

/// Resolve which log storage backend the daemon will route through and
/// emit a [`DaemonRunEvent::LogStorageDispatchResolved`] so operators see
/// the choice on every startup. Failures are degraded to in-tree + a
/// warning rather than aborting startup — a misbehaving log_storage
/// plugin must never block the daemon from coming up.
fn resolve_log_storage_dispatch_for_daemon<H: DaemonRunHooks>(project_root: &str, primary_root: &str, hooks: &mut H) {
    let disable_env_set = crate::log_storage_disable_env_set();
    match crate::resolve_log_storage_dispatch(Path::new(project_root)) {
        Ok(resolution) => {
            let plugin_name = resolution.selected.plugin_name().map(|s| s.to_string());
            let candidate_count = resolution.all_candidates.len();
            let _ = hooks.handle_event(DaemonRunEvent::LogStorageDispatchResolved {
                project_root: primary_root.to_string(),
                plugin_name,
                candidate_count,
                disable_env_set,
                warnings: resolution.warnings,
            });
        }
        Err(error) => {
            let _ = hooks.handle_event(DaemonRunEvent::LogStorageDispatchResolved {
                project_root: primary_root.to_string(),
                plugin_name: None,
                candidate_count: 0,
                disable_env_set,
                warnings: vec![format!(
                    "log_storage_backend discovery failed; falling back to in-tree Logger: {error:#}"
                )],
            });
        }
    }
}

/// Start the daemon's control RPC server (Unix socket speaking the
/// `animus-control-protocol` wire format).
///
/// Honors [`crate::control::CONTROL_SERVER_DISABLE_ENV`]: when the env
/// var is set truthy the server is skipped and the
/// [`DaemonRunEvent::ControlServerResolved`] event carries
/// `disable_env_set = true`. Any bind / chmod / IO failure degrades to
/// "no server, warning emitted" rather than aborting the daemon — a
/// misbehaving socket must never block startup. The handle is returned
/// for graceful shutdown on daemon teardown.
async fn start_control_server_for_daemon<H: DaemonRunHooks>(
    project_root: &str,
    primary_root: &str,
    hooks: &mut H,
    workflow_event_broadcaster: Arc<WorkflowEventBroadcaster>,
) -> Option<crate::control::ControlServerHandle> {
    let project_root_path = Path::new(project_root);
    let socket_path = crate::control::control_socket_path(project_root_path);
    let disable_env_set = crate::control::control_server_disable_env_set();

    if disable_env_set {
        let _ = hooks.handle_event(DaemonRunEvent::ControlServerResolved {
            project_root: primary_root.to_string(),
            socket_path: socket_path.clone(),
            disable_env_set: true,
            warnings: vec![format!(
                "control server skipped because {} is set",
                crate::control::CONTROL_SERVER_DISABLE_ENV
            )],
        });
        return None;
    }

    let mut surface_builder = crate::control::InProcessSurface::builder(project_root_path.to_path_buf())
        .daemon_version(env!("CARGO_PKG_VERSION").to_string());
    if let Some(routing) = hooks.plugin_routing() {
        surface_builder = surface_builder.plugin_routing(routing);
    }
    if let Some(routing) = hooks.daemon_ops_routing() {
        surface_builder = surface_builder.daemon_ops_routing(routing);
    }
    if let Some(routing) = hooks.workflow_routing() {
        surface_builder = surface_builder.workflow_routing(routing);
    }
    if let Some(routing) = hooks.queue_routing() {
        surface_builder = surface_builder.queue_routing(routing);
    }
    if let Some(routing) = hooks.agent_routing() {
        surface_builder = surface_builder.agent_routing(routing);
    }
    let surface = surface_builder.build();
    let surface_arc: Arc<dyn animus_control_protocol::ControlSurface> = Arc::new(surface);

    match crate::control::ControlServer::start_with_workflow_events(
        project_root_path,
        surface_arc,
        workflow_event_broadcaster,
    )
    .await
    {
        Ok(handle) => {
            let _ = hooks.handle_event(DaemonRunEvent::ControlServerResolved {
                project_root: primary_root.to_string(),
                socket_path: handle.socket_path().to_path_buf(),
                disable_env_set: false,
                warnings: Vec::new(),
            });
            Some(handle)
        }
        Err(error) => {
            let _ = hooks.handle_event(DaemonRunEvent::ControlServerResolved {
                project_root: primary_root.to_string(),
                socket_path: socket_path.clone(),
                disable_env_set: false,
                warnings: vec![format!(
                    "control server failed to start; CLI/MCP must fall back to in-process services: {error}"
                )],
            });
            None
        }
    }
}

fn discover_plugins_for_daemon<H: DaemonRunHooks>(project_root: &str, primary_root: &str, hooks: &mut H) -> Result<()> {
    use orchestrator_plugin_host::DiscoverySource;
    match orchestrator_plugin_host::discover_plugins(Path::new(project_root)) {
        Ok(plugins) => {
            let summaries = plugins
                .into_iter()
                .map(|p| DiscoveredPluginSummary {
                    name: p.name,
                    version: p.manifest.version,
                    plugin_kind: p.manifest.plugin_kind,
                    source: match p.source {
                        DiscoverySource::ExplicitConfig => "explicit_config",
                        DiscoverySource::ProjectLocal => "project_local",
                        DiscoverySource::PluginPath => "plugin_path",
                        DiscoverySource::SystemPath => "system_path",
                    },
                    path: p.path.display().to_string(),
                })
                .collect::<Vec<_>>();
            hooks.handle_event(DaemonRunEvent::PluginsDiscovered {
                project_root: primary_root.to_string(),
                plugins: summaries,
            })?;
        }
        Err(error) => {
            hooks.handle_event(DaemonRunEvent::PluginsDiscoveryFailed {
                project_root: primary_root.to_string(),
                error: error.to_string(),
            })?;
        }
    }
    Ok(())
}

struct SigtermStream {
    #[cfg(unix)]
    inner: tokio::signal::unix::Signal,
}

impl SigtermStream {
    fn new() -> Result<Self> {
        #[cfg(unix)]
        {
            let inner = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .context("failed to subscribe to SIGTERM")?;
            Ok(Self { inner })
        }
        #[cfg(not(unix))]
        {
            Ok(Self {})
        }
    }

    async fn recv(&mut self) {
        #[cfg(unix)]
        {
            self.inner.recv().await;
        }
        #[cfg(not(unix))]
        {
            std::future::pending::<()>().await;
        }
    }
}

fn canonicalize_lossy(path: &str) -> String {
    let candidate = PathBuf::from(path);
    candidate.canonicalize().unwrap_or(candidate).to_string_lossy().to_string()
}
