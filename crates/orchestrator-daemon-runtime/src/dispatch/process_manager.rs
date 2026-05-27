use std::collections::HashSet;
#[cfg(unix)]
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use protocol::orchestrator::WorkflowStatus;
use protocol::SubjectDispatch;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

#[cfg(unix)]
use crate::control::WorkflowEventBroadcaster;
#[cfg(unix)]
use crate::dispatch::event_pipe::SubprocessEventPipe;
use crate::{build_runner_command, CompletedProcess, RunnerEvent};

struct WorkflowProcess {
    subject_key: String,
    subject_id: String,
    subject_kind: String,
    task_id: Option<String>,
    workflow_ref: String,
    schedule_id: Option<String>,
    started_at: std::time::Instant,
    child: Arc<Mutex<Child>>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
    stderr_reader: Option<JoinHandle<()>>,
    /// Per-spawn workflow_events back-channel. Dropped after the
    /// subprocess is reaped so the socket file is cleaned up.
    #[cfg(unix)]
    #[allow(dead_code)]
    event_pipe: Option<SubprocessEventPipe>,
}

pub struct ProcessManager {
    processes: Vec<WorkflowProcess>,
    process_timeout_secs: Option<u64>,
    pub phase_routing: Option<protocol::PhaseRoutingConfig>,
    pub mcp_config: Option<protocol::McpRuntimeConfig>,
    /// Broadcaster that subprocess back-channel readers forward into.
    /// `None` means subprocess workflow_events fan-out is disabled and the
    /// spawn path falls back to setting no env var (runner uses the noop
    /// emitter). Wired by the daemon at startup via
    /// [`Self::with_event_broadcaster`].
    #[cfg(unix)]
    event_broadcaster: Option<Arc<WorkflowEventBroadcaster>>,
    /// Root directory under which per-spawn event-pipe socket files live.
    #[cfg(unix)]
    pipe_root: Option<PathBuf>,
    /// Cap on the number of concurrently-running runner subprocesses. New
    /// spawn requests beyond this point are rejected; the dispatcher then
    /// leaves the entry in the ready queue for the next tick.
    workflow_concurrency_max: Option<usize>,
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessManager {
    pub fn new() -> Self {
        // The workflow concurrency cap is sourced from `RuntimeQuotas`
        // (which reads `ANIMUS_WORKFLOW_CONCURRENCY_MAX` once at install
        // time, with a documented default of 10). When the env var is
        // explicitly set, the quota struct still honors it — this keeps
        // the operator escape hatch working while ensuring the documented
        // default actually applies even when no env var is present.
        //
        // The subprocess workflow_events broadcaster is NOT looked up
        // here — it is picked up lazily on each spawn via
        // [`crate::daemon::current_workflow_event_broadcaster`] so that
        // a `ProcessManager` constructed before `run_daemon` installs
        // the broadcaster (the normal CLI startup sequence) still
        // attaches per-run pipes once the daemon is live.
        let workflow_concurrency_max = Some(crate::quotas::runtime_quotas().workflow_concurrency_max);
        Self {
            processes: Vec::new(),
            process_timeout_secs: None,
            phase_routing: None,
            mcp_config: None,
            #[cfg(unix)]
            event_broadcaster: None,
            #[cfg(unix)]
            pipe_root: None,
            workflow_concurrency_max,
        }
    }

    pub fn with_timeout(mut self, timeout_secs: Option<u64>) -> Self {
        self.process_timeout_secs = timeout_secs;
        self
    }

    /// Wire the subprocess workflow_events back-channel into this
    /// `ProcessManager`. Every spawn will allocate a per-run Unix
    /// domain socket under `pipe_root` (created if missing), advertise it
    /// to the runner via `ANIMUS_WORKFLOW_EVENT_PIPE`, and start a reader
    /// task that forwards events into `broadcaster`.
    #[cfg(unix)]
    pub fn with_event_broadcaster(mut self, broadcaster: Arc<WorkflowEventBroadcaster>, pipe_root: PathBuf) -> Self {
        self.event_broadcaster = Some(broadcaster);
        self.pipe_root = Some(pipe_root);
        self
    }

    /// Override the cap on the number of concurrently-running runner
    /// subprocesses. `Some(n)` pins the cap at `n`; `None` disables the
    /// cap entirely (unbounded — for tests / specialty deployments that
    /// rely on external scheduling). When the cap is reached,
    /// [`Self::spawn_workflow_runner`] returns a recoverable error and
    /// the caller leaves the entry in the dispatch queue for the next
    /// tick.
    ///
    /// Note: the default cap (from `ProcessManager::new()`) is already
    /// seeded from `RuntimeQuotas::workflow_concurrency_max`, so this
    /// setter is only needed when overriding for tests or specialty
    /// dispatchers.
    pub fn with_workflow_concurrency_max(mut self, max: Option<usize>) -> Self {
        self.workflow_concurrency_max = max;
        self
    }

    pub fn spawn_workflow_runner(&mut self, dispatch: &SubjectDispatch, project_root: &str) -> Result<()> {
        if let Some(cap) = self.workflow_concurrency_max {
            if self.processes.len() >= cap {
                anyhow::bail!(
                    "workflow concurrency cap reached ({} active, max {}); leaving entry queued for next tick",
                    self.processes.len(),
                    cap
                );
            }
        }

        let std_cmd =
            build_runner_command(dispatch, project_root, self.phase_routing.as_ref(), self.mcp_config.as_ref());
        let mut command = Command::from(std_cmd);
        command.stdout(Stdio::null()).stderr(Stdio::piped());

        // Bind the subprocess workflow_events back-channel before fork so the
        // env var we set on the child points to a listener that's already
        // accepting. Best-effort: if bind fails (eg no Unix DS support in a
        // sandbox) we still spawn without the back-channel and the runner
        // falls back to its noop emitter.
        #[cfg(unix)]
        let event_pipe = self.bind_event_pipe_for(dispatch, &mut command);

        let mut child = command.spawn().context("failed to spawn ao-workflow-runner")?;

        let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stderr_reader = if let Some(stderr) = child.stderr.take() {
            let lines = stderr_lines.clone();
            Some(tokio::spawn(async move {
                use tokio::io::{AsyncBufReadExt, BufReader};
                let reader = BufReader::new(stderr);
                let mut line_stream = reader.lines();
                while let Ok(Some(line)) = line_stream.next_line().await {
                    if let Ok(mut buf) = lines.lock() {
                        buf.push(line);
                    }
                }
            }))
        } else {
            None
        };

        let task_id = dispatch.task_id().map(String::from);
        let workflow_ref = dispatch.workflow_ref.clone();
        let schedule_id = dispatch.schedule_id().map(String::from);

        self.processes.push(WorkflowProcess {
            subject_key: dispatch.subject_key(),
            subject_id: dispatch.subject_id().to_string(),
            subject_kind: dispatch.subject_kind().to_string(),
            task_id,
            workflow_ref,
            schedule_id,
            started_at: std::time::Instant::now(),
            child: Arc::new(Mutex::new(child)),
            stderr_lines,
            stderr_reader,
            #[cfg(unix)]
            event_pipe,
        });

        Ok(())
    }

    /// Bind a fresh per-spawn event pipe and attach the
    /// `ANIMUS_WORKFLOW_EVENT_PIPE` env var to `command` so the runner can
    /// connect. Returns `None` when the back-channel isn't configured on
    /// this `ProcessManager` (eg tests, or daemons that opted out) or when
    /// `bind` fails on the host filesystem. Failure is best-effort: we
    /// proceed with the spawn so the workflow still runs; only the
    /// fan-out is silently disabled for that run.
    #[cfg(unix)]
    fn bind_event_pipe_for(&self, dispatch: &SubjectDispatch, command: &mut Command) -> Option<SubprocessEventPipe> {
        // Lazy lookup so the broadcaster is picked up even when the
        // `ProcessManager` was constructed BEFORE [`crate::run_daemon`]
        // installed it (the production daemon spawns the manager first
        // and then starts the daemon loop). Explicit per-instance wiring
        // via [`Self::with_event_broadcaster`] still wins so tests can
        // pin a specific broadcaster.
        let broadcaster = match self.event_broadcaster.as_ref() {
            Some(bus) => bus.clone(),
            None => crate::daemon::current_workflow_event_broadcaster()?,
        };
        let pipe_root = match self.pipe_root.as_ref() {
            Some(root) => root.clone(),
            None => default_event_pipe_root(),
        };
        let subject_label = dispatch.subject_id().to_string();
        // Bind synchronously on the calling thread (just a couple of
        // syscalls) and let `SubprocessEventPipe::bind_sync` spawn the
        // reader task on the current Tokio runtime. This avoids the
        // previous pattern of spawning a bind helper task and blocking
        // on a channel for its result, which could deadlock on a
        // current-thread runtime and stall an executor worker on
        // multi-thread runtimes.
        //
        // Requires a current Tokio runtime (the reader task needs a home);
        // returning `None` when none is present preserves legacy
        // best-effort semantics — the workflow still spawns, only the
        // fan-out is dropped for that run.
        if tokio::runtime::Handle::try_current().is_err() {
            return None;
        }
        let pipe = match SubprocessEventPipe::bind_sync(&pipe_root, &subject_label, broadcaster) {
            Ok(pipe) => Some(pipe),
            Err(error) => {
                tracing::warn!(
                    target: "animus.runtime.event_pipe",
                    %error,
                    "failed to bind workflow event pipe; subprocess events will be dropped"
                );
                None
            }
        };
        if let Some(ref pipe) = pipe {
            command.env(SubprocessEventPipe::env_var(), pipe.socket_path());
        }
        pipe
    }

    pub async fn check_running(&mut self) -> Vec<CompletedProcess> {
        let timeout = self.process_timeout_secs;
        self.check_running_with_timeout(timeout).await
    }

    async fn check_running_with_timeout(&mut self, timeout_secs: Option<u64>) -> Vec<CompletedProcess> {
        let mut completed = Vec::new();
        let mut active = Vec::with_capacity(self.processes.len());

        for mut process in self.processes.drain(..) {
            if let Some(timeout) = timeout_secs {
                if process.started_at.elapsed().as_secs() > timeout {
                    let pid = process.child.lock().ok().and_then(|c| c.id());
                    if let Some(pid) = pid {
                        protocol::graceful_kill_process(pid as i32);
                    }
                    drain_stderr_reader(&mut process.stderr_reader).await;
                    completed.push(CompletedProcess {
                        subject_id: process.subject_key,
                        subject_kind: Some(process.subject_kind),
                        task_id: process.task_id,
                        workflow_id: None,
                        workflow_ref: Some(process.workflow_ref),
                        workflow_status: Some(WorkflowStatus::Failed),
                        schedule_id: process.schedule_id,
                        exit_code: None,
                        success: false,
                        failure_reason: Some(format!("workflow runner exceeded timeout of {} seconds", timeout)),
                        events: parse_runner_events(&process.stderr_lines),
                    });
                    continue;
                }
            }
            let status = {
                let mut maybe_child = match process.child.lock() {
                    Ok(guard) => guard,
                    Err(error) => {
                        completed.push(CompletedProcess {
                            subject_id: process.subject_key,
                            subject_kind: Some(process.subject_kind),
                            task_id: process.task_id,
                            workflow_id: None,
                            workflow_ref: Some(process.workflow_ref),
                            workflow_status: None,
                            schedule_id: process.schedule_id,
                            exit_code: None,
                            success: false,
                            failure_reason: Some(format!("failed to lock workflow process handle: {}", error)),
                            events: Vec::new(),
                        });
                        continue;
                    }
                };

                maybe_child.try_wait()
            };

            match status {
                Ok(Some(status)) => {
                    drain_stderr_reader(&mut process.stderr_reader).await;
                    let exit_code = status.code();
                    let events = parse_runner_events(&process.stderr_lines);
                    let workflow_id = latest_runner_workflow_id(&events);
                    let workflow_status = latest_runner_workflow_status(&events);
                    let (success, failure_reason) = if status.success() {
                        (true, None)
                    } else {
                        (false, Some(format!("workflow runner exited unsuccessfully with status {:?}", exit_code)))
                    };

                    completed.push(CompletedProcess {
                        subject_id: process.subject_key,
                        subject_kind: Some(process.subject_kind),
                        task_id: process.task_id,
                        workflow_id,
                        workflow_ref: Some(process.workflow_ref),
                        workflow_status,
                        schedule_id: process.schedule_id,
                        exit_code,
                        success,
                        failure_reason,
                        events,
                    });
                }
                Ok(None) => active.push(process),
                Err(error) => {
                    completed.push(CompletedProcess {
                        subject_id: process.subject_key,
                        subject_kind: Some(process.subject_kind),
                        task_id: process.task_id,
                        workflow_id: None,
                        workflow_ref: Some(process.workflow_ref),
                        workflow_status: None,
                        schedule_id: process.schedule_id,
                        exit_code: None,
                        success: false,
                        failure_reason: Some(format!("failed to probe workflow process status: {}", error)),
                        events: Vec::new(),
                    });
                }
            }
        }

        self.processes = active;
        completed
    }

    pub fn active_count(&self) -> usize {
        self.processes.len()
    }

    pub fn active_subject_ids(&self) -> HashSet<String> {
        self.processes.iter().flat_map(|process| [process.subject_key.clone(), process.subject_id.clone()]).collect()
    }
}

/// Default per-process directory for per-run event-pipe socket files.
/// Picked under `$TMPDIR/animus-event-pipes/<pid>/` so the path stays well
/// under SUN_LEN on macOS / Linux even when the project root is deep, and
/// so concurrent daemons can't collide on file names.
#[cfg(unix)]
fn default_event_pipe_root() -> std::path::PathBuf {
    std::env::temp_dir().join("animus-event-pipes").join(std::process::id().to_string())
}

async fn drain_stderr_reader(handle: &mut Option<JoinHandle<()>>) {
    if let Some(h) = handle.take() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), h).await;
    }
}

fn parse_runner_events(stderr_lines: &Arc<Mutex<Vec<String>>>) -> Vec<RunnerEvent> {
    let lines = match stderr_lines.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => return Vec::new(),
    };
    lines.iter().filter_map(|line| serde_json::from_str::<RunnerEvent>(line).ok()).collect()
}

fn latest_runner_workflow_id(events: &[RunnerEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| event.workflow_id.clone())
}

fn latest_runner_workflow_status(events: &[RunnerEvent]) -> Option<WorkflowStatus> {
    events.iter().rev().find_map(|event| event.workflow_status)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::await_holding_lock)]

    use super::*;
    use std::env;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let original = env::var(key).ok();
            match value {
                Some(value) => env::set_var(key, value),
                None => env::remove_var(key),
            }
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.original.as_deref() {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn new_process_manager_starts_empty() {
        let manager = ProcessManager::new();
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn new_process_manager_seeds_concurrency_cap_from_runtime_quotas() {
        // `ProcessManager::new()` must always seed `workflow_concurrency_max`
        // from `RuntimeQuotas` — never leave it `None`. Previously the
        // field was `None` unless `ANIMUS_WORKFLOW_CONCURRENCY_MAX` was
        // explicitly set, leaving the spawn site unbounded for typical
        // operators and contradicting the documented "default 10" cap
        // in the v0.4.13 CHANGELOG.
        //
        // We don't mutate the env here (would race other tests sharing
        // the process); we only assert the wiring: whatever the
        // process-wide quota currently is, `ProcessManager::new()`
        // mirrors it as `Some(quota)`.
        let manager = ProcessManager::new();
        let cap = manager.workflow_concurrency_max.expect("default cap must be wired from RuntimeQuotas");
        let expected = crate::quotas::runtime_quotas().workflow_concurrency_max;
        assert_eq!(cap, expected, "ProcessManager cap must match the live RuntimeQuotas value");
        assert!(cap > 0, "default workflow concurrency must be > 0; got {cap}");
    }

    #[tokio::test]
    async fn spawn_workflow_runner_tracks_active_processes() {
        let _lock = test_env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let temp_dir = TempDir::new().expect("temp directory should be created");
        let runner_path = {
            #[cfg(unix)]
            let path = temp_dir.path().join("ao-workflow-runner");
            #[cfg(not(unix))]
            let path = temp_dir.path().join("ao-workflow-runner.exe");
            path
        };

        #[cfg(unix)]
        let runner_payload = "#!/bin/sh\nexit 0\n";
        #[cfg(not(unix))]
        let runner_payload = "@echo off\r\nexit /B 0\r\n";

        fs::write(&runner_path, runner_payload).expect("mock runner should be written");
        #[cfg(unix)]
        {
            let mut permissions =
                fs::metadata(&runner_path).expect("mock runner metadata should be available").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner_path, permissions).expect("mock runner should be executable");
        }

        let runner_override = runner_path.to_string_lossy();
        let _runner_guard = EnvVarGuard::set("ANIMUS_WORKFLOW_RUNNER_BIN", Some(runner_override.as_ref()));

        let mut manager = ProcessManager::new();
        let dispatch = SubjectDispatch::for_task("task-123", "standard");
        manager
            .spawn_workflow_runner(&dispatch, temp_dir.path().to_string_lossy().as_ref())
            .expect("mock runner should be spawned via explicit workflow runner override");
        assert_eq!(manager.active_count(), 1);
        let _ = manager.check_running().await;
    }

    #[tokio::test]
    async fn workflow_concurrency_queues_when_at_cap() {
        // v0.4.13: ProcessManager with `with_workflow_concurrency_max(2)`
        // accepts the first two spawns and refuses the third with a
        // recoverable error so the dispatcher leaves the third entry in
        // the ready queue for the next tick.
        let _lock = test_env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let temp_dir = TempDir::new().expect("temp directory should be created");
        let runner_path = temp_dir.path().join("ao-workflow-runner");
        // A runner that sleeps long enough that the first two stay active
        // while we attempt the third spawn.
        #[cfg(unix)]
        let runner_payload = "#!/bin/sh\nsleep 5\nexit 0\n";
        #[cfg(not(unix))]
        let runner_payload = "@echo off\r\ntimeout 5\r\nexit /B 0\r\n";
        fs::write(&runner_path, runner_payload).expect("mock runner should be written");
        #[cfg(unix)]
        {
            let mut permissions =
                fs::metadata(&runner_path).expect("mock runner metadata should be available").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner_path, permissions).expect("mock runner should be executable");
        }
        let runner_override = runner_path.to_string_lossy();
        let _runner_guard = EnvVarGuard::set("ANIMUS_WORKFLOW_RUNNER_BIN", Some(runner_override.as_ref()));

        let mut manager = ProcessManager::new().with_workflow_concurrency_max(Some(2));
        let project_root = temp_dir.path().to_string_lossy().to_string();

        let d1 = SubjectDispatch::for_task("task-1", "standard");
        let d2 = SubjectDispatch::for_task("task-2", "standard");
        let d3 = SubjectDispatch::for_task("task-3", "standard");

        manager.spawn_workflow_runner(&d1, &project_root).expect("spawn 1 should succeed (under cap)");
        manager.spawn_workflow_runner(&d2, &project_root).expect("spawn 2 should succeed (at cap)");
        assert_eq!(manager.active_count(), 2);

        let third = manager.spawn_workflow_runner(&d3, &project_root);
        assert!(third.is_err(), "spawn 3 must be refused when at concurrency cap");
        let err = third.unwrap_err().to_string();
        assert!(err.contains("workflow concurrency cap reached"), "error must explain the cap; got: {err}");
        // The dispatcher's contract: refused entries stay in the queue.
        // We assert active count is still 2 (the third was not silently
        // accepted then dropped).
        assert_eq!(manager.active_count(), 2);

        // Drain so the test exits cleanly.
        for _ in 0..200 {
            if manager.check_running().await.len() == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    #[test]
    fn subject_id_returns_correct_value_for_each_variant() {
        let task = SubjectDispatch::for_task("TASK-1", "standard");
        assert_eq!(task.subject_id(), "TASK-1");
        assert!(task.schedule_id().is_none());

        let requirement = SubjectDispatch::for_requirement("REQ-1", "standard", "manual");
        assert_eq!(requirement.subject_id(), "REQ-1");
        assert!(requirement.schedule_id().is_none());

        let custom = SubjectDispatch::for_custom(
            "schedule:nightly",
            "nightly run",
            "standard",
            Some(serde_json::json!({"key":"value"})),
            "schedule",
        );
        assert_eq!(custom.subject_id(), "schedule:nightly");
        assert_eq!(custom.schedule_id(), Some("nightly"));
    }

    #[tokio::test]
    async fn custom_subject_tracks_schedule_id_and_parses_events() {
        let _lock = test_env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let temp_dir = TempDir::new().expect("temp directory should be created");
        let runner_path = temp_dir.path().join("ao-workflow-runner");
        let runner_payload = "#!/bin/sh\nprintf '%s\\n' '{\"event\":\"runner_start\",\"workflow_ref\":\"standard\"}' >&2\nprintf '%s\\n' '{\"event\":\"runner_complete\",\"workflow_ref\":\"standard\",\"exit_code\":0}' >&2\nexit 0\n";
        fs::write(&runner_path, runner_payload).expect("mock runner should be written");
        #[cfg(unix)]
        {
            let mut permissions =
                fs::metadata(&runner_path).expect("mock runner metadata should be available").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner_path, permissions).expect("mock runner should be executable");
        }

        let runner_override = runner_path.to_string_lossy();
        let _runner_guard = EnvVarGuard::set("ANIMUS_WORKFLOW_RUNNER_BIN", Some(runner_override.as_ref()));

        let mut manager = ProcessManager::new();
        let dispatch = SubjectDispatch::for_custom("schedule:nightly", "nightly run", "standard", None, "schedule");
        manager
            .spawn_workflow_runner(&dispatch, temp_dir.path().to_string_lossy().as_ref())
            .expect("mock runner should spawn");

        let mut completed = Vec::new();
        for _ in 0..100 {
            completed = manager.check_running().await;
            if !completed.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        assert_eq!(completed.len(), 1);
        let completed = &completed[0];
        assert_eq!(completed.subject_id, "schedule:nightly");
        assert_eq!(completed.schedule_id.as_deref(), Some("nightly"));
        assert!(completed.success);
        assert_eq!(completed.events.len(), 2);
        assert!(completed.workflow_id.is_none());
        assert!(completed.workflow_status.is_none());
        assert_eq!(completed.events[0].workflow_ref.as_deref(), Some("standard"));
        assert_eq!(completed.events[1].workflow_ref.as_deref(), Some("standard"));
    }

    #[tokio::test]
    async fn generic_subjects_keep_kind_qualified_completion_identity() {
        let _lock = test_env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        let temp_dir = TempDir::new().expect("temp directory should be created");
        let runner_path = temp_dir.path().join("ao-workflow-runner");
        fs::write(&runner_path, "#!/bin/sh\nexit 0\n").expect("mock runner should be written");
        #[cfg(unix)]
        {
            let mut permissions =
                fs::metadata(&runner_path).expect("mock runner metadata should be available").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner_path, permissions).expect("mock runner should be executable");
        }

        let runner_override = runner_path.to_string_lossy();
        let _runner_guard = EnvVarGuard::set("ANIMUS_WORKFLOW_RUNNER_BIN", Some(runner_override.as_ref()));

        let dispatch = SubjectDispatch::for_subject_with_metadata(
            protocol::SubjectRef::new("pack.review", "REV-7"),
            "review",
            "manual",
            chrono::Utc::now(),
        );

        let mut manager = ProcessManager::new();
        manager
            .spawn_workflow_runner(&dispatch, temp_dir.path().to_string_lossy().as_ref())
            .expect("mock runner should spawn");

        let active_subject_ids = manager.active_subject_ids();
        assert!(active_subject_ids.contains("REV-7"));
        assert!(active_subject_ids.contains("pack.review::REV-7"));

        let mut completed = Vec::new();
        for _ in 0..100 {
            completed = manager.check_running().await;
            if !completed.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].subject_id, "pack.review::REV-7");
        assert_eq!(completed[0].subject_kind.as_deref(), Some("pack.review"));
    }
}
