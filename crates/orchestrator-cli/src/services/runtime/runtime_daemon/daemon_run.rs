use crate::cli_types::DaemonRunArgs;
use crate::services::operations::{
    build_agent_routing, build_plugin_routing, build_queue_routing, build_workflow_routing, run_plugin_install,
    PluginInstallRequest,
};
use crate::services::runtime::runtime_daemon::build_daemon_ops_routing;
use crate::services::runtime::runtime_daemon::daemon_reconciliation::recover_orphaned_running_workflows;
use anyhow::Result;
use async_trait::async_trait;
use orchestrator_core::services::DaemonStartConfig;
use orchestrator_core::DaemonStatus;
use orchestrator_core::FileServiceHub;
use orchestrator_core::ServiceHub;
use orchestrator_core::{
    load_daemon_project_config, write_daemon_project_config, InstalledPluginSummary, PluginInstaller,
};
use orchestrator_daemon_runtime::control::{
    AgentRouting, DaemonOpsRouting, PluginRouting, QueueRouting, WorkflowRouting,
};
use orchestrator_daemon_runtime::{
    discover_installed_plugins, run_daemon, DaemonRunEvent, DaemonRunHooks, ProcessManager,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

#[cfg(test)]
use super::canonicalize_lossy;
use super::daemon_run_host::DefaultDaemonRunHost;
use super::daemon_scheduler::{runtime_options_from_cli, slim_project_tick_driver, SlimProjectTickDriver};

pub(super) struct CliPluginInstaller {
    project_root: String,
}

impl CliPluginInstaller {
    pub(super) fn new(project_root: impl Into<String>) -> Self {
        Self { project_root: project_root.into() }
    }
}

#[async_trait(?Send)]
impl PluginInstaller for CliPluginInstaller {
    async fn install(&self, repo_spec: &str) -> Result<String> {
        let req = PluginInstallRequest { source: Some(repo_spec.to_string()), yes: true, ..Default::default() };
        let output = run_plugin_install(req).await?;
        Ok(output.name)
    }

    async fn rediscover(&self) -> Result<Vec<InstalledPluginSummary>> {
        discover_installed_plugins(&self.project_root)
    }
}

struct CliDaemonRunHost {
    inner: DefaultDaemonRunHost,
    start_config: DaemonStartConfig,
    installer: Arc<CliPluginInstaller>,
    plugin_routing: Arc<dyn PluginRouting>,
    daemon_ops_routing: Arc<dyn DaemonOpsRouting>,
    workflow_routing: Arc<dyn WorkflowRouting>,
    queue_routing: Arc<dyn QueueRouting>,
    agent_routing: Arc<dyn AgentRouting>,
}

impl CliDaemonRunHost {
    fn new(project_root: &str, json: bool, start_config: DaemonStartConfig) -> Self {
        let project_root_path = PathBuf::from(project_root);
        let plugin_routing = build_plugin_routing(project_root_path.clone());
        let daemon_ops_routing = build_daemon_ops_routing(project_root_path.clone(), SystemTime::now());
        let workflow_routing = build_workflow_routing(project_root_path.clone());
        let queue_routing = build_queue_routing(project_root_path.clone());
        let agent_routing = build_agent_routing(project_root_path);
        let installer = Arc::new(CliPluginInstaller::new(project_root));
        Self {
            inner: DefaultDaemonRunHost::new(project_root, json),
            start_config,
            installer,
            plugin_routing,
            daemon_ops_routing,
            workflow_routing,
            queue_routing,
            agent_routing,
        }
    }

    fn logger(&self) -> std::sync::Arc<orchestrator_logging::Logger> {
        self.inner.logger.clone()
    }
}

#[async_trait::async_trait(?Send)]
impl DaemonRunHooks for CliDaemonRunHost {
    fn handle_event(&mut self, event: DaemonRunEvent) -> Result<()> {
        self.inner.handle_event(event)
    }

    async fn daemon_status(&mut self, project_root: &str) -> Result<DaemonStatus> {
        let hub = FileServiceHub::new(project_root)?;
        hub.daemon().status().await
    }

    async fn start_daemon(&mut self, project_root: &str) -> Result<()> {
        let hub = FileServiceHub::new(project_root)?;
        hub.daemon().start(self.start_config.clone()).await
    }

    async fn stop_daemon(&mut self, project_root: &str) -> Result<()> {
        let hub = FileServiceHub::new(project_root)?;
        hub.daemon().stop().await
    }

    async fn recover_startup_orphans(&mut self, project_root: &str) -> Result<usize> {
        let startup_hub = Arc::new(FileServiceHub::new(project_root)?);
        let orphans = recover_orphaned_running_workflows(
            startup_hub as Arc<dyn ServiceHub>,
            project_root,
            &std::collections::HashSet::new(),
        )
        .await;

        // v0.4.x scope: surface running CLI session checkpoints by marking them
        // blocked with an actionable reason. Auto-resume via provider.resume_agent
        // is deferred to v0.5; the checkpoint file + notification log provide the
        // durable evidence operators need to resolve manually with `--force`.
        if let Some(scoped_root) =
            protocol::repository_scope::scoped_state_root(std::path::Path::new(project_root))
        {
            if let Ok(running) = workflow_runner_v2::phase_session::list_running_checkpoints(&scoped_root) {
                for (_, checkpoint) in running {
                    let _ = workflow_runner_v2::phase_session::update_session_blocked(
                        &scoped_root,
                        &checkpoint.workflow_id,
                        &checkpoint.phase_id,
                        "daemon restarted with phase mid-execution; review notifications.jsonl and resume with `animus workflow resume <id> --force` if the phase is idempotent",
                    );
                }
            }
        }

        Ok(orphans)
    }

    async fn flush_notifications(&mut self, project_root: &str) -> Result<()> {
        self.inner.flush_notifications(project_root).await
    }

    fn plugin_routing(&self) -> Option<Arc<dyn PluginRouting>> {
        Some(self.plugin_routing.clone())
    }

    fn daemon_ops_routing(&self) -> Option<Arc<dyn DaemonOpsRouting>> {
        Some(self.daemon_ops_routing.clone())
    }

    fn workflow_routing(&self) -> Option<Arc<dyn WorkflowRouting>> {
        Some(self.workflow_routing.clone())
    }

    fn queue_routing(&self) -> Option<Arc<dyn QueueRouting>> {
        Some(self.queue_routing.clone())
    }

    fn agent_routing(&self) -> Option<Arc<dyn AgentRouting>> {
        Some(self.agent_routing.clone())
    }

    fn plugin_installer(&self) -> Option<Arc<dyn PluginInstaller>> {
        Some(self.installer.clone())
    }
}

fn apply_scheduler_overrides_to_pm_config(args: &DaemonRunArgs, project_root: &str) {
    let project_path = std::path::Path::new(project_root);
    let mut config = load_daemon_project_config(project_path).unwrap_or_default();
    let mut changed = false;

    if let Some(value) = args.scheduler.auto_merge {
        if config.auto_merge_enabled != value {
            config.auto_merge_enabled = value;
            changed = true;
        }
    }
    if let Some(value) = args.scheduler.auto_pr {
        if config.auto_pr_enabled != value {
            config.auto_pr_enabled = value;
            changed = true;
        }
    }
    if let Some(value) = args.scheduler.auto_commit_before_merge {
        if config.auto_commit_before_merge != value {
            config.auto_commit_before_merge = value;
            changed = true;
        }
    }
    if let Some(value) = args.scheduler.auto_prune_worktrees_after_merge {
        if config.auto_prune_worktrees_after_merge != value {
            config.auto_prune_worktrees_after_merge = value;
            changed = true;
        }
    }

    // Persist runtime-reconfigurable settings from CLI overrides so they survive
    // daemon restart and are available for hot-reload.
    if let Some(value) = args.scheduler.pool_size {
        if config.pool_size != Some(value) {
            config.pool_size = Some(value);
            changed = true;
        }
    }
    if let Some(value) = args.scheduler.auto_run_ready {
        if config.auto_run_ready != Some(value) {
            config.auto_run_ready = Some(value);
            changed = true;
        }
    }
    if let Some(value) = args.scheduler.interval_secs {
        if config.interval_secs != Some(value) {
            config.interval_secs = Some(value);
            changed = true;
        }
    }
    if let Some(value) = args.scheduler.max_tasks_per_tick {
        if config.max_tasks_per_tick != Some(value) {
            config.max_tasks_per_tick = Some(value);
            changed = true;
        }
    }
    if let Some(value) = args.scheduler.stale_threshold_hours {
        if config.stale_threshold_hours != Some(value) {
            config.stale_threshold_hours = Some(value);
            changed = true;
        }
    }
    if args.scheduler.phase_timeout_secs.is_some() && config.phase_timeout_secs != args.scheduler.phase_timeout_secs {
        config.phase_timeout_secs = args.scheduler.phase_timeout_secs;
        changed = true;
    }
    if args.scheduler.idle_timeout_secs.is_some() && config.idle_timeout_secs != args.scheduler.idle_timeout_secs {
        config.idle_timeout_secs = args.scheduler.idle_timeout_secs;
        changed = true;
    }

    if changed {
        let _ = write_daemon_project_config(project_path, &config);
    }
}

pub(super) async fn handle_daemon_run(args: DaemonRunArgs, project_root: &str, json: bool) -> Result<()> {
    apply_scheduler_overrides_to_pm_config(&args, project_root);
    let mut runtime_options = runtime_options_from_cli(&args, project_root);
    let start_config = DaemonStartConfig {
        pool_size: runtime_options.pool_size,
        skip_runner: args.skip_runner,
        runner_scope: args.runner_scope.as_ref().map(super::runner_scope_value).map(str::to_string),
    };
    let workflow_config = orchestrator_core::load_workflow_config_or_default(std::path::Path::new(project_root));
    let daemon_config = workflow_config.config.daemon.as_ref();
    let mut process_manager = ProcessManager::new().with_timeout(runtime_options.phase_timeout_secs);
    process_manager.phase_routing = daemon_config.and_then(|d| d.phase_routing.clone());
    process_manager.mcp_config = daemon_config.and_then(|d| d.mcp.clone());
    let mut host = CliDaemonRunHost::new(project_root, json, start_config);
    let logger = host.logger();
    let mut driver: SlimProjectTickDriver<'_> =
        slim_project_tick_driver(&runtime_options, &mut process_manager, logger);

    let run_result =
        run_daemon(project_root, &mut runtime_options, &mut driver, &mut host, |driver| driver.active_process_count())
            .await;

    run_result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]

    use super::*;
    use crate::services::runtime::runtime_daemon::{daemon_events_log_path, DaemonEventRecord};
    use crate::DaemonSchedulerArgs;
    use std::sync::MutexGuard;
    use tempfile::TempDir;

    fn lock_env() -> MutexGuard<'static, ()> {
        crate::shared::test_env_lock().lock().unwrap_or_else(|p| p.into_inner())
    }

    use protocol::test_utils::EnvVarGuard;

    #[tokio::test]
    async fn daemon_run_once_processes_current_project_root() {
        let _lock = lock_env();

        let config_root = TempDir::new().expect("config temp dir");
        let home_root = TempDir::new().expect("home temp dir");
        let _config_guard = EnvVarGuard::set("ANIMUS_CONFIG_DIR", Some(config_root.path().to_string_lossy().as_ref()));
        let _home_guard = EnvVarGuard::set("HOME", Some(home_root.path().to_string_lossy().as_ref()));
        let _legacy_guard = EnvVarGuard::set("AGENT_ORCHESTRATOR_CONFIG_DIR", None);

        let primary = TempDir::new().expect("primary project dir");
        let primary_root = primary.path().to_string_lossy().to_string();

        let args = DaemonRunArgs {
            scheduler: DaemonSchedulerArgs {
                pool_size: None,
                interval_secs: Some(1),

                auto_run_ready: Some(false),
                auto_merge: None,
                auto_pr: None,
                auto_commit_before_merge: None,
                auto_prune_worktrees_after_merge: None,
                startup_cleanup: true,
                resume_interrupted: false,
                reconcile_stale: false,
                stale_threshold_hours: Some(24),
                max_tasks_per_tick: Some(1),
                phase_timeout_secs: None,
                idle_timeout_secs: None,
            },
            skip_runner: true,
            runner_scope: None,
            once: true,
            auto_install: false,
            skip_preflight: true,
        };
        handle_daemon_run(args, &primary_root, true).await.expect("daemon run should succeed");

        let events_path = daemon_events_log_path();
        let events_content = std::fs::read_to_string(events_path).expect("daemon events log should exist");
        let events: Vec<DaemonEventRecord> = events_content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<DaemonEventRecord>(line).expect("event json"))
            .collect();

        let queue_event = events
            .iter()
            .find(|event| {
                event.event_type == "queue"
                    && event.project_root.as_deref() == Some(canonicalize_lossy(&primary_root).as_str())
            })
            .expect("queue event for primary project should exist");
        for field in [
            "stale_in_progress_count",
            "stale_in_progress_threshold_hours",
            "started_ready_workflows",
            "executed_workflow_phases",
            "failed_workflow_phases",
        ] {
            assert!(
                queue_event.data.get(field).and_then(serde_json::Value::as_u64).is_some(),
                "queue event field `{field}` should be present as an integer"
            );
        }
        assert!(
            queue_event.data.get("stale_in_progress_task_ids").and_then(serde_json::Value::as_array).is_some(),
            "queue event field `stale_in_progress_task_ids` should be present as an array"
        );
    }

    #[tokio::test]
    async fn daemon_run_emits_task_state_change_events() {
        let _lock = lock_env();

        let config_root = TempDir::new().expect("config temp dir");
        let home_root = TempDir::new().expect("home temp dir");
        let _config_guard = EnvVarGuard::set("ANIMUS_CONFIG_DIR", Some(config_root.path().to_string_lossy().as_ref()));
        let _home_guard = EnvVarGuard::set("HOME", Some(home_root.path().to_string_lossy().as_ref()));
        let _legacy_guard = EnvVarGuard::set("AGENT_ORCHESTRATOR_CONFIG_DIR", None);

        let primary = TempDir::new().expect("primary project dir");
        let primary_root = primary.path().to_string_lossy().to_string();
        let primary_hub = Arc::new(FileServiceHub::new(&primary_root).expect("primary hub"));

        let task = primary_hub
            .tasks()
            .create(orchestrator_core::TaskCreateInput {
                title: "transition task".to_string(),
                description: "verify task-state-change daemon events".to_string(),
                task_type: Some(orchestrator_core::TaskType::Feature),
                priority: Some(orchestrator_core::Priority::Medium),
                created_by: Some("test".to_string()),
                tags: Vec::new(),
                linked_requirements: Vec::new(),
                linked_architecture_entities: Vec::new(),
            })
            .await
            .expect("task should be created");

        let workflow = primary_hub
            .workflows()
            .run(orchestrator_core::WorkflowRunInput::for_task(task.id.clone(), None))
            .await
            .expect("workflow should run");
        // Cancel the workflow so all task workflows are terminal with no success.
        // The stale-in-progress reconciler only auto-transitions tasks to Blocked
        // when every workflow failed/cancelled (it never auto-completes tasks).
        let workflow = primary_hub.workflows().cancel(&workflow.id).await.expect("workflow should cancel");
        assert_eq!(workflow.status, orchestrator_core::WorkflowStatus::Cancelled);

        primary_hub
            .tasks()
            .set_status(&task.id, orchestrator_core::TaskStatus::InProgress, false)
            .await
            .expect("task should be stale in-progress");

        let args = DaemonRunArgs {
            scheduler: DaemonSchedulerArgs {
                pool_size: None,
                interval_secs: Some(1),

                auto_run_ready: Some(false),
                auto_merge: None,
                auto_pr: None,
                auto_commit_before_merge: None,
                auto_prune_worktrees_after_merge: None,
                startup_cleanup: false,
                resume_interrupted: false,
                reconcile_stale: true,
                stale_threshold_hours: Some(24),
                max_tasks_per_tick: Some(1),
                phase_timeout_secs: None,
                idle_timeout_secs: None,
            },
            skip_runner: true,
            runner_scope: None,
            once: true,
            auto_install: false,
            skip_preflight: true,
        };
        handle_daemon_run(args, &primary_root, true).await.expect("daemon run should emit transition event");

        let events_path = daemon_events_log_path();
        let events_content = std::fs::read_to_string(events_path).expect("daemon events log should exist");
        let events: Vec<DaemonEventRecord> = events_content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<DaemonEventRecord>(line).expect("event json"))
            .collect();

        let transition_event = events
            .iter()
            .find(|event| {
                event.event_type == "task-state-change"
                    && event.project_root.as_deref() == Some(canonicalize_lossy(&primary_root).as_str())
                    && event.data.get("task_id").and_then(serde_json::Value::as_str) == Some(task.id.as_str())
            })
            .expect("task-state-change event should be emitted");
        assert_eq!(transition_event.data.get("from_status").and_then(serde_json::Value::as_str), Some("in-progress"));
        assert_eq!(transition_event.data.get("to_status").and_then(serde_json::Value::as_str), Some("blocked"));
        assert!(transition_event
            .data
            .get("changed_at")
            .and_then(serde_json::Value::as_str)
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false));
    }

    #[tokio::test]
    async fn daemon_run_emits_selection_source_for_started_task_events() {
        let _lock = lock_env();

        let config_root = TempDir::new().expect("config temp dir");
        let home_root = TempDir::new().expect("home temp dir");
        let _config_guard = EnvVarGuard::set("ANIMUS_CONFIG_DIR", Some(config_root.path().to_string_lossy().as_ref()));
        let _home_guard = EnvVarGuard::set("HOME", Some(home_root.path().to_string_lossy().as_ref()));
        let _legacy_guard = EnvVarGuard::set("AGENT_ORCHESTRATOR_CONFIG_DIR", None);

        let test_bin_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .expect("test binary directory");
        let release_bin_dir = test_bin_dir.parent().unwrap_or(&test_bin_dir);
        let path_with_bin_dir = format!(
            "{}:{}:{}",
            release_bin_dir.display(),
            test_bin_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let _path_guard = EnvVarGuard::set("PATH", Some(&path_with_bin_dir));

        let primary = TempDir::new().expect("primary project dir");
        let primary_root = primary.path().to_string_lossy().to_string();
        let primary_hub = Arc::new(FileServiceHub::new(&primary_root).expect("primary hub"));

        let task = primary_hub
            .tasks()
            .create(orchestrator_core::TaskCreateInput {
                title: "start selection source task".to_string(),
                description: "verify daemon emits selection source on workflow start".to_string(),
                task_type: Some(orchestrator_core::TaskType::Feature),
                priority: Some(orchestrator_core::Priority::Medium),
                created_by: Some("test".to_string()),
                tags: Vec::new(),
                linked_requirements: Vec::new(),
                linked_architecture_entities: Vec::new(),
            })
            .await
            .expect("task should be created");
        primary_hub
            .tasks()
            .set_status(&task.id, orchestrator_core::TaskStatus::Ready, false)
            .await
            .expect("task should be ready");

        let args = DaemonRunArgs {
            scheduler: DaemonSchedulerArgs {
                pool_size: None,
                interval_secs: Some(1),

                auto_run_ready: Some(true),
                auto_merge: None,
                auto_pr: None,
                auto_commit_before_merge: None,
                auto_prune_worktrees_after_merge: None,
                startup_cleanup: false,
                resume_interrupted: false,
                reconcile_stale: false,
                stale_threshold_hours: Some(24),
                max_tasks_per_tick: Some(1),
                phase_timeout_secs: None,
                idle_timeout_secs: None,
            },
            skip_runner: true,
            runner_scope: None,
            once: true,
            auto_install: false,
            skip_preflight: true,
        };
        handle_daemon_run(args, &primary_root, true).await.expect("daemon run should emit selection source transition");

        let events_path = daemon_events_log_path();
        let events_content = std::fs::read_to_string(events_path).expect("daemon events log should exist");
        let events: Vec<DaemonEventRecord> = events_content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<DaemonEventRecord>(line).expect("event json"))
            .collect();

        let selection_event = events
            .iter()
            .find(|event| {
                event.event_type == "task-state-change"
                    && event.project_root.as_deref() == Some(canonicalize_lossy(&primary_root).as_str())
                    && event.data.get("task_id").and_then(serde_json::Value::as_str) == Some(task.id.as_str())
                    && event.data.get("selection_source").and_then(serde_json::Value::as_str).is_some()
            })
            .expect("task-state-change event with selection source should be emitted");

        assert_eq!(selection_event.data.get("selection_source").and_then(serde_json::Value::as_str), Some("queue"));
    }

    #[tokio::test]
    async fn daemon_run_continues_when_notification_delivery_fails() {
        let _lock = lock_env();

        let config_root = TempDir::new().expect("config temp dir");
        let home_root = TempDir::new().expect("home temp dir");
        let _config_guard = EnvVarGuard::set("ANIMUS_CONFIG_DIR", Some(config_root.path().to_string_lossy().as_ref()));
        let _home_guard = EnvVarGuard::set("HOME", Some(home_root.path().to_string_lossy().as_ref()));
        let _legacy_guard = EnvVarGuard::set("AGENT_ORCHESTRATOR_CONFIG_DIR", None);
        let _missing_url = EnvVarGuard::set("ANIMUS_NOTIFY_MISSING_URL", None);

        let primary = TempDir::new().expect("primary project dir");
        let primary_root = primary.path().to_string_lossy().to_string();

        let pm_config_path = orchestrator_core::daemon_project_config_path(std::path::Path::new(&primary_root));
        std::fs::create_dir_all(pm_config_path.parent().expect("pm-config path should have parent"))
            .expect("scoped daemon config directory should be created");
        let pm_config = serde_json::json!({
            "notification_config": {
                "schema": "animus.daemon-notification-config.v1",
                "version": 1,
                "connectors": [
                    {
                        "type": "webhook",
                        "id": "ops-webhook",
                        "enabled": true,
                        "url_env": "ANIMUS_NOTIFY_MISSING_URL"
                    }
                ],
                "subscriptions": [
                    {
                        "id": "all-events",
                        "enabled": true,
                        "connector_id": "ops-webhook",
                        "event_types": ["*"]
                    }
                ],
                "retry_policy": {
                    "max_attempts": 1,
                    "base_delay_secs": 1,
                    "max_delay_secs": 5
                },
                "max_deliveries_per_tick": 8
            }
        });
        std::fs::write(
            &pm_config_path,
            format!("{}\n", serde_json::to_string_pretty(&pm_config).expect("serialize config")),
        )
        .expect("pm-config should be written");

        let args = DaemonRunArgs {
            scheduler: DaemonSchedulerArgs {
                pool_size: None,
                interval_secs: Some(1),

                auto_run_ready: Some(false),
                auto_merge: None,
                auto_pr: None,
                auto_commit_before_merge: None,
                auto_prune_worktrees_after_merge: None,
                startup_cleanup: true,
                resume_interrupted: false,
                reconcile_stale: false,
                stale_threshold_hours: Some(24),
                max_tasks_per_tick: Some(1),
                phase_timeout_secs: None,
                idle_timeout_secs: None,
            },
            skip_runner: true,
            runner_scope: None,
            once: true,
            auto_install: false,
            skip_preflight: true,
        };
        handle_daemon_run(args, &primary_root, true)
            .await
            .expect("daemon run should succeed even when notification delivery fails");

        let events_path = daemon_events_log_path();
        let events_content = std::fs::read_to_string(events_path).expect("daemon events log should exist");
        let events: Vec<DaemonEventRecord> = events_content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<DaemonEventRecord>(line).expect("event json"))
            .collect();

        assert!(events.iter().any(|event| event.event_type == "notification-delivery-dead-lettered"));
    }

    #[test]
    fn daemon_run_does_not_clobber_auto_run_ready_when_omitted() {
        let _lock = lock_env();

        let config_root = TempDir::new().expect("config temp dir");
        let home_root = TempDir::new().expect("home temp dir");
        let _config_guard = EnvVarGuard::set("ANIMUS_CONFIG_DIR", Some(config_root.path().to_string_lossy().as_ref()));
        let _home_guard = EnvVarGuard::set("HOME", Some(home_root.path().to_string_lossy().as_ref()));
        let _legacy_guard = EnvVarGuard::set("AGENT_ORCHESTRATOR_CONFIG_DIR", None);

        let project_root = TempDir::new().expect("project dir");
        let config = orchestrator_core::DaemonProjectConfig {
            auto_run_ready: Some(false),
            interval_secs: Some(11),
            max_tasks_per_tick: Some(7),
            stale_threshold_hours: Some(42),
            ..Default::default()
        };
        orchestrator_core::write_daemon_project_config(project_root.path(), &config).expect("seed daemon config");

        let args = DaemonRunArgs {
            scheduler: DaemonSchedulerArgs {
                pool_size: None,
                interval_secs: None,
                auto_run_ready: None,
                auto_merge: None,
                auto_pr: None,
                auto_commit_before_merge: None,
                auto_prune_worktrees_after_merge: None,
                startup_cleanup: true,
                resume_interrupted: true,
                reconcile_stale: true,
                stale_threshold_hours: None,
                max_tasks_per_tick: None,
                phase_timeout_secs: None,
                idle_timeout_secs: None,
            },
            skip_runner: true,
            runner_scope: None,
            once: true,
            auto_install: false,
            skip_preflight: true,
        };

        apply_scheduler_overrides_to_pm_config(&args, project_root.path().to_string_lossy().as_ref());

        let loaded = orchestrator_core::load_daemon_project_config(project_root.path()).expect("load daemon config");
        assert_eq!(loaded.auto_run_ready, Some(false));
        assert_eq!(loaded.interval_secs, Some(11));
        assert_eq!(loaded.max_tasks_per_tick, Some(7));
        assert_eq!(loaded.stale_threshold_hours, Some(42));
    }
}
