//! Verifies that a `run_daemon` invocation whose preflight check fails on a
//! project whose daemon was not previously running does NOT leave behind a
//! `DaemonStatus::Running` record. Regression test for the codex round-3 P1
//! finding (run_daemon.rs:115-145): preflight aborts must short-circuit
//! BEFORE the daemon is marked running, so subsequent status commands report
//! "stopped" rather than a phantom "running" daemon.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orchestrator_core::{DaemonStatus, PluginPreflightSpec, RequiredRole};
use orchestrator_daemon_runtime::{
    run_daemon, DaemonRunEvent, DaemonRunHooks, DaemonRuntimeOptions, DispatchWorkflowStartSummary, ProjectTickHooks,
    ProjectTickSnapshot, ProjectTickSummary, ProjectTickSummaryInput,
};
use serde_json::Value;
use tempfile::TempDir;

#[derive(Default, Clone)]
struct LifecycleCounts {
    start_daemon_calls: Arc<Mutex<usize>>,
    stop_daemon_calls: Arc<Mutex<usize>>,
    current_status: Arc<Mutex<DaemonStatus>>,
}

impl LifecycleCounts {
    fn new() -> Self {
        Self {
            start_daemon_calls: Arc::new(Mutex::new(0)),
            stop_daemon_calls: Arc::new(Mutex::new(0)),
            current_status: Arc::new(Mutex::new(DaemonStatus::Stopped)),
        }
    }

    fn start_calls(&self) -> usize {
        *self.start_daemon_calls.lock().unwrap()
    }

    fn stop_calls(&self) -> usize {
        *self.stop_daemon_calls.lock().unwrap()
    }

    fn status(&self) -> DaemonStatus {
        self.current_status.lock().unwrap().clone()
    }
}

struct StubHooks {
    counts: LifecycleCounts,
    spec: PluginPreflightSpec,
}

#[async_trait(?Send)]
impl DaemonRunHooks for StubHooks {
    fn handle_event(&mut self, _event: DaemonRunEvent) -> Result<()> {
        Ok(())
    }

    async fn daemon_status(&mut self, _project_root: &str) -> Result<DaemonStatus> {
        Ok(self.counts.status())
    }

    async fn start_daemon(&mut self, _project_root: &str) -> Result<()> {
        *self.counts.start_daemon_calls.lock().unwrap() += 1;
        *self.counts.current_status.lock().unwrap() = DaemonStatus::Running;
        Ok(())
    }

    async fn stop_daemon(&mut self, _project_root: &str) -> Result<()> {
        *self.counts.stop_daemon_calls.lock().unwrap() += 1;
        *self.counts.current_status.lock().unwrap() = DaemonStatus::Stopped;
        Ok(())
    }

    async fn recover_startup_orphans(&mut self, _project_root: &str) -> Result<usize> {
        Ok(0)
    }

    fn plugin_preflight_spec(&self) -> PluginPreflightSpec {
        self.spec.clone()
    }
}

struct StubDriver;

#[async_trait(?Send)]
impl ProjectTickHooks for StubDriver {
    fn process_due_schedules(&mut self, _root: &str, _now: DateTime<Utc>, _schedule_headroom: Option<usize>) {}

    async fn capture_snapshot(&mut self, _root: &str) -> Result<ProjectTickSnapshot> {
        Ok(ProjectTickSnapshot {
            requirements_before: Vec::new(),
            tasks_before: Vec::new(),
            started_daemon: false,
            daemon_health: None,
        })
    }

    async fn reconcile_completed_processes(&mut self, _root: &str) -> Result<(usize, usize)> {
        Ok((0, 0))
    }

    async fn dispatch_ready_tasks(&mut self, _root: &str, _limit: usize) -> Result<DispatchWorkflowStartSummary> {
        Ok(DispatchWorkflowStartSummary::default())
    }

    async fn collect_health(&mut self, _root: &str) -> Result<Value> {
        Ok(Value::Null)
    }

    async fn build_summary(
        &mut self,
        _args: &DaemonRuntimeOptions,
        input: ProjectTickSummaryInput,
    ) -> Result<ProjectTickSummary> {
        Ok(ProjectTickSummary {
            project_root: input.project_root,
            started_daemon: input.started_daemon,
            health: input.health,
            tasks_total: 0,
            tasks_ready: 0,
            tasks_in_progress: 0,
            tasks_blocked: 0,
            tasks_done: 0,
            stale_in_progress_count: 0,
            stale_in_progress_threshold_hours: 0,
            stale_in_progress_task_ids: Vec::new(),
            workflows_running: 0,
            workflows_completed: 0,
            workflows_failed: 0,
            resumed_workflows: 0,
            cleaned_stale_workflows: 0,
            reconciled_workflows: 0,
            started_ready_workflows: 0,
            executed_workflow_phases: 0,
            failed_workflow_phases: 0,
            task_state_changes: Vec::new(),
            phase_execution_events: Vec::new(),
        })
    }
}

fn pin_test_home() -> TempDir {
    let home = TempDir::new().expect("tempdir home");
    std::env::set_var("HOME", home.path());
    home
}

#[tokio::test]
async fn daemon_start_with_preflight_failure_does_not_leave_running_state() {
    let _home = pin_test_home();
    let project = TempDir::new().expect("tempdir project");
    let project_root = project.path().to_string_lossy().to_string();

    // Force preflight to fail by requiring a subject kind that no installed
    // plugin can satisfy. Auto-install is off (the spec carries no fix repo
    // for the unknown kind) so the runner reports missing and aborts.
    let counts = LifecycleCounts::new();
    let mut hooks = StubHooks {
        counts: counts.clone(),
        spec: PluginPreflightSpec {
            required_roles: vec![RequiredRole::SubjectKind("nonexistent-kind".to_string())],
            auto_install: false,
            auto_install_defaults: Vec::new(),
        },
    };
    let mut driver = StubDriver;
    let mut options = DaemonRuntimeOptions { once: true, ..DaemonRuntimeOptions::default() };

    let result = run_daemon(&project_root, &mut options, &mut driver, &mut hooks, |_| 0).await;
    assert!(result.is_err(), "preflight failure must propagate as an error");
    let message = result.unwrap_err().to_string();
    assert!(message.contains("preflight failed"), "expected preflight failure message, got: {message}");

    assert_eq!(
        counts.start_calls(),
        0,
        "start_daemon must NOT be invoked when preflight aborts (otherwise daemon status leaks `running` after a failed boot)"
    );
    assert_eq!(counts.stop_calls(), 0, "stop_daemon must not be needed when start_daemon was never called");
    assert!(
        matches!(counts.status(), DaemonStatus::Stopped),
        "persisted daemon status must remain stopped after a preflight abort"
    );
}
