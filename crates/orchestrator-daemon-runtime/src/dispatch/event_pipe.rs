//! Per-spawn Unix-domain-socket back-channel for forwarding
//! `workflow_events` from a runner subprocess into the daemon's in-process
//! [`WorkflowEventBroadcaster`].
//!
//! Lifecycle (Unix only — Windows pending):
//!
//! 1. Before [`crate::ProcessManager::spawn_workflow_runner`] forks the
//!    runner, the caller asks for a [`SubprocessEventPipe`] via
//!    [`SubprocessEventPipe::bind`].
//! 2. The pipe binds a freshly-named `UnixListener` under the project's
//!    `~/.animus/<scope>/runs/<workflow-or-subject>/events.pipe` directory
//!    (falls back to `~/.animus/runs/.../events.pipe` when scoped state is
//!    unavailable, e.g. tests with no git context).
//! 3. The path is exposed via the env var
//!    [`workflow_runner_v2::workflow_event_emitter::ANIMUS_WORKFLOW_EVENT_PIPE_ENV`]
//!    on the spawn command; the runner reads it on startup and builds a
//!    [`workflow_runner_v2::SubprocessPipeEmitter`].
//! 4. A reader task per-spawn accepts the (single) inbound connection,
//!    reads newline-delimited JSON frames, deserializes to
//!    [`WireWorkflowEvent`], and forwards each into the broadcaster as a
//!    wire [`animus_control_protocol::types::WorkflowEvent`].
//! 5. On subprocess exit / pipe close the reader task ends and
//!    [`SubprocessEventPipe::cleanup`] removes the socket file.
//!
//! Windows: this module is gated `#[cfg(unix)]`. On Windows the
//! constructor returns `None` and the daemon dispatches without a
//! back-channel — workflow_events from subprocess runs are silently
//! dropped until a Windows transport (named pipes) is wired.

#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::Arc;

#[cfg(unix)]
use tokio::task::JoinHandle;
#[cfg(unix)]
use workflow_runner_v2::workflow_event_emitter::{WireWorkflowEvent, ANIMUS_WORKFLOW_EVENT_PIPE_ENV};

#[cfg(unix)]
use crate::control::WorkflowEventBroadcaster;

/// A bound back-channel for a single workflow runner spawn. Carries the
/// path the subprocess will connect to plus the reader-task handle.
///
/// The pipe must outlive the spawned subprocess; drop it after the
/// subprocess has been reaped (and you've drained its stderr). Dropping
/// aborts the reader task and removes the socket file.
#[cfg(unix)]
pub struct SubprocessEventPipe {
    socket_path: PathBuf,
    reader_task: Option<JoinHandle<()>>,
}

#[cfg(unix)]
#[allow(dead_code)]
impl SubprocessEventPipe {
    /// Bind a fresh per-spawn back-channel under `pipe_root` and start a
    /// reader task that forwards each received event into `broadcaster`.
    ///
    /// `pipe_root` should be the directory we'd like the socket file to
    /// live in (the caller decides scope-vs-project layout); the directory
    /// is created if missing. `subject_label` is a short human-friendly id
    /// included in the file name to make socket files self-identifying when
    /// an operator tails `~/.animus/.../runs/`.
    pub async fn bind(
        pipe_root: &Path,
        subject_label: &str,
        broadcaster: Arc<WorkflowEventBroadcaster>,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(pipe_root)?;
        // Unix domain socket paths are capped (SUN_LEN — ~104 bytes on
        // macOS, 108 on Linux). Pick a socket directory whose path fits
        // even when pipe_root is deep — fall back to $TMPDIR for the
        // socket itself while keeping the per-run dir under pipe_root for
        // operator visibility.
        let socket_name = short_socket_name(subject_label);
        let mut socket_path = pipe_root.join(&socket_name);
        if socket_path.as_os_str().len() > MAX_UNIX_SOCKET_PATH_BYTES {
            let fallback_root = std::env::temp_dir().join("animus-event-pipes");
            std::fs::create_dir_all(&fallback_root)?;
            socket_path = fallback_root.join(&socket_name);
        }
        // Defensive: if a stale socket from a crashed prior run is sitting
        // at the same path, remove it so bind() can succeed.
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }
        let listener = tokio::net::UnixListener::bind(&socket_path)?;

        let socket_path_clone = socket_path.clone();
        let reader_task = tokio::spawn(async move {
            reader_loop(listener, broadcaster, socket_path_clone).await;
        });

        Ok(Self { socket_path, reader_task: Some(reader_task) })
    }

    /// Path the runner subprocess should connect to. Pass this via the
    /// [`ANIMUS_WORKFLOW_EVENT_PIPE_ENV`] env var on the spawn command.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Env var name the runner subprocess reads. Exposed for the spawn
    /// site so it can `cmd.env(SubprocessEventPipe::env_var(), pipe.socket_path())`.
    pub fn env_var() -> &'static str {
        ANIMUS_WORKFLOW_EVENT_PIPE_ENV
    }

    /// Explicitly tear down the reader task and remove the socket file.
    /// Idempotent. Equivalent to dropping the pipe except this allows
    /// the caller to await reader-task completion deterministically.
    pub async fn shutdown(mut self) {
        if let Some(task) = self.reader_task.take() {
            task.abort();
            let _ = task.await;
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(unix)]
impl Drop for SubprocessEventPipe {
    fn drop(&mut self) {
        if let Some(task) = self.reader_task.take() {
            task.abort();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(unix)]
async fn reader_loop(
    listener: tokio::net::UnixListener,
    broadcaster: Arc<WorkflowEventBroadcaster>,
    socket_path: PathBuf,
) {
    // We only ever expect one client per spawn (the runner subprocess we
    // just forked). Loop accepts so that if the runner reconnects after a
    // transient write failure we pick the new connection up.
    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(error) => {
                tracing::debug!(
                    target: "animus.runtime.event_pipe",
                    socket = %socket_path.display(),
                    %error,
                    "event pipe listener errored; exiting reader loop"
                );
                return;
            }
        };

        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    forward_line(&broadcaster, &line);
                }
                Ok(None) => break, // EOF; subprocess closed.
                Err(error) => {
                    tracing::debug!(
                        target: "animus.runtime.event_pipe",
                        %error,
                        "event pipe stream errored mid-stream; rotating to accept loop"
                    );
                    break;
                }
            }
        }
    }
}

#[cfg(unix)]
fn forward_line(broadcaster: &WorkflowEventBroadcaster, line: &str) {
    let wire: WireWorkflowEvent = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                target: "animus.runtime.event_pipe",
                %error,
                "discarding malformed workflow event frame"
            );
            return;
        }
    };
    let event = animus_control_protocol::types::WorkflowEvent {
        workflow_id: wire.workflow_id,
        kind: wire.kind,
        payload: wire.payload,
        occurred_at: wire.occurred_at,
    };
    broadcaster.emit(event);
}

/// Conservative cap on the Unix domain socket path length. POSIX guarantees
/// only ~104 bytes for SUN_LEN on macOS / 108 on Linux. Use 100 to give
/// headroom for trailing nulls and platform quirks.
#[cfg(unix)]
const MAX_UNIX_SOCKET_PATH_BYTES: usize = 100;

/// Build a short, collision-resistant socket file name. We never embed the
/// full subject_label because most subject ids are too long to leave room
/// for a meaningful unique suffix inside `MAX_UNIX_SOCKET_PATH_BYTES`.
#[cfg(unix)]
fn short_socket_name(subject_label: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    // 12 chars of sanitized subject label is enough to make the file
    // self-identifying without blowing the path budget.
    let short_label: String =
        subject_label.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(12).collect();
    let short_label = if short_label.is_empty() { "wf".to_string() } else { short_label };
    format!("{}-{:x}.sock", short_label, nanos & 0xffff_ffff)
}

#[cfg(not(unix))]
pub struct SubprocessEventPipe;

#[cfg(not(unix))]
impl SubprocessEventPipe {
    pub fn env_var() -> &'static str {
        "ANIMUS_WORKFLOW_EVENT_PIPE"
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::control::{SubscriberItem, WorkflowEventBroadcaster, WorkflowEventFilter};
    use chrono::Utc;
    use serde_json::json;
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use tempfile::tempdir;
    use workflow_runner_v2::workflow_event_emitter::WireWorkflowEvent;

    fn unwrap_event(item: SubscriberItem) -> animus_control_protocol::types::WorkflowEvent {
        match item {
            SubscriberItem::Event(e) => e,
            SubscriberItem::Closed { reason } => panic!("expected event, got Closed({reason})"),
        }
    }

    #[tokio::test]
    async fn subprocess_workflow_emits_events_via_pipe_to_daemon_broadcaster() {
        let temp = tempdir().expect("tempdir");
        let bus = WorkflowEventBroadcaster::new();
        let (_id, mut rx) = bus.subscribe(WorkflowEventFilter::default());

        let pipe = SubprocessEventPipe::bind(temp.path(), "wf-pipe-test", bus.clone()).await.expect("bind pipe");
        let socket_path = pipe.socket_path().to_path_buf();

        // Simulate the subprocess writing two newline-delimited frames into the
        // socket the daemon prebound. This is exactly what
        // SubprocessPipeEmitter does inside the runner process.
        let writer_path = socket_path.clone();
        let writer = tokio::task::spawn_blocking(move || {
            let mut stream = UnixStream::connect(&writer_path).expect("subprocess connect");
            let evt1 = WireWorkflowEvent {
                workflow_id: "wf-subprocess-1".to_string(),
                kind: "phase_started".to_string(),
                payload: json!({"phase_id": "impl"}),
                occurred_at: Utc::now(),
            };
            let evt2 = WireWorkflowEvent {
                workflow_id: "wf-subprocess-1".to_string(),
                kind: "phase_completed".to_string(),
                payload: json!({"phase_id": "impl", "phase_status": "completed"}),
                occurred_at: Utc::now(),
            };
            for evt in [&evt1, &evt2] {
                let mut line = serde_json::to_string(evt).expect("serialize");
                line.push('\n');
                stream.write_all(line.as_bytes()).expect("write line");
            }
            // Drop stream to signal EOF.
        });
        writer.await.expect("writer task");

        let first = unwrap_event(rx.recv().await.expect("event 1 must arrive"));
        let second = unwrap_event(rx.recv().await.expect("event 2 must arrive"));
        assert_eq!(first.workflow_id, "wf-subprocess-1");
        assert_eq!(first.kind, "phase_started");
        assert_eq!(second.kind, "phase_completed");

        pipe.shutdown().await;
        assert!(!socket_path.exists(), "socket file should be removed on shutdown");
    }

    #[tokio::test]
    async fn malformed_line_is_discarded_without_killing_reader() {
        let temp = tempdir().expect("tempdir");
        let bus = WorkflowEventBroadcaster::new();
        let (_id, mut rx) = bus.subscribe(WorkflowEventFilter::default());

        let pipe = SubprocessEventPipe::bind(temp.path(), "wf-malformed", bus.clone()).await.expect("bind pipe");
        let socket_path = pipe.socket_path().to_path_buf();

        let writer = tokio::task::spawn_blocking(move || {
            let mut stream = UnixStream::connect(&socket_path).expect("connect");
            stream.write_all(b"this is not json\n").expect("write garbage");
            let valid = WireWorkflowEvent {
                workflow_id: "wf-after-garbage".to_string(),
                kind: "phase_started".to_string(),
                payload: json!({}),
                occurred_at: Utc::now(),
            };
            let mut line = serde_json::to_string(&valid).expect("serialize");
            line.push('\n');
            stream.write_all(line.as_bytes()).expect("write valid");
        });
        writer.await.expect("writer task");

        let valid_event = unwrap_event(rx.recv().await.expect("post-garbage event must still arrive"));
        assert_eq!(valid_event.workflow_id, "wf-after-garbage");
        pipe.shutdown().await;
    }
}
