//! Daemon-side reattach client for v0.5.1 P2 #6.2 round-3.
//!
//! Pairs with [`workflow_runner_v2::reattach::ReattachListenerEmitter`]: the
//! runner binds a `UnixListener` at a deterministic path advertised via the
//! spawn record's `stdio_socket_path`. On daemon startup, after the orphan
//! scan reports each live orphan, this module opens a `UnixStream` to that
//! path, spawns a reader task that translates newline-JSON
//! [`workflow_runner_v2::WireWorkflowEvent`] frames into wire
//! [`animus_control_protocol::types::WorkflowEvent`] and forwards them into
//! the daemon's [`crate::control::WorkflowEventBroadcaster`].
//!
//! Scope honesty for this fold-in:
//! - Only NEW events emitted by the runner AFTER reattach are forwarded.
//!   Events the runner emitted during the daemon-gap are buffered nowhere
//!   on the runner side — they are visible only via `decisions.jsonl`. Gap
//!   reconstruction from `decisions.jsonl` is a v0.6 follow-up.
//! - The reader task lives until the runner closes the socket OR the
//!   daemon shuts down. There is no built-in cancellation hook today; if a
//!   reattached orphan completes mid-stream, the JoinHandle exits cleanly
//!   on EOF.
//! - No throttling, no retry. A failed connect emits
//!   [`crate::DaemonRunEvent::OrphanAgentReattachFailed`] and the orphan
//!   record stays on disk for the operator to inspect or for the next
//!   daemon start to retry.
//! - Windows: gated `#[cfg(unix)]`. Windows daemons skip reattach entirely
//!   and the orphan record stays on disk.

#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::sync::Arc;

#[cfg(unix)]
use tokio::task::JoinHandle;

#[cfg(unix)]
use crate::control::WorkflowEventBroadcaster;

#[cfg(unix)]
#[allow(dead_code)]
pub struct ReattachConnection {
    socket_path: std::path::PathBuf,
    reader_task: JoinHandle<()>,
}

#[cfg(unix)]
impl ReattachConnection {
    #[allow(dead_code)]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

/// Try to connect to a runner's reattach listener and start forwarding its
/// events into `broadcaster`. Returns `Ok(connection)` on a successful
/// connect; the caller is responsible for retaining the
/// [`ReattachConnection`] so the reader task isn't dropped (dropping the
/// JoinHandle merely detaches the task on tokio, but we keep the handle
/// for testability and future graceful-shutdown hooks).
#[cfg(unix)]
pub fn try_reattach(
    socket_path: &Path,
    broadcaster: Arc<WorkflowEventBroadcaster>,
) -> std::io::Result<ReattachConnection> {
    use tokio::net::UnixStream;
    // Connect synchronously using std to get a clean, blocking error if the
    // runner isn't listening yet (eg the daemon restarted faster than the
    // runner bound its listener). Promote to a tokio stream for the reader
    // task so we don't block a runtime worker.
    let std_stream = std::os::unix::net::UnixStream::connect(socket_path)?;
    std_stream.set_nonblocking(true)?;
    let stream = UnixStream::from_std(std_stream)?;
    let socket_owned = socket_path.to_path_buf();
    let reader_socket = socket_owned.clone();
    let reader_task = tokio::spawn(reader_loop(stream, broadcaster, reader_socket));
    Ok(ReattachConnection { socket_path: socket_owned, reader_task })
}

#[cfg(unix)]
async fn reader_loop(
    stream: tokio::net::UnixStream,
    broadcaster: Arc<WorkflowEventBroadcaster>,
    socket: std::path::PathBuf,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => forward_line(&broadcaster, &line, &socket),
            Ok(None) => {
                tracing::debug!(
                    target: "animus.runtime.reattach",
                    socket = %socket.display(),
                    "reattach reader EOF (runner closed)"
                );
                return;
            }
            Err(error) => {
                tracing::debug!(
                    target: "animus.runtime.reattach",
                    socket = %socket.display(),
                    %error,
                    "reattach reader stream error; exiting"
                );
                return;
            }
        }
    }
}

#[cfg(unix)]
fn forward_line(broadcaster: &WorkflowEventBroadcaster, line: &str, socket: &Path) {
    let wire: workflow_runner_v2::workflow_event_emitter::WireWorkflowEvent = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                target: "animus.runtime.reattach",
                socket = %socket.display(),
                %error,
                "discarding malformed reattach event frame"
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

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::control::WorkflowEventBroadcaster;
    use std::io::Write;
    use std::os::unix::net::UnixListener as StdListener;
    use std::path::PathBuf;
    use std::thread;
    use tempfile::TempDir;

    fn pair() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("reattach.sock");
        (dir, path)
    }

    #[tokio::test]
    async fn try_reattach_fails_on_missing_socket() {
        let (_dir, path) = pair();
        let broadcaster = WorkflowEventBroadcaster::new();
        let err = try_reattach(&path, broadcaster).err().expect("connect must fail when socket absent");
        assert!(err.kind() == std::io::ErrorKind::NotFound || err.kind() == std::io::ErrorKind::ConnectionRefused);
    }

    #[tokio::test]
    async fn forwarded_event_reaches_broadcaster_subscriber() {
        use crate::control::WorkflowEventFilter;
        let (_dir, path) = pair();
        let listener = StdListener::bind(&path).expect("bind listener");
        let broadcaster = WorkflowEventBroadcaster::new();
        let (_id, mut rx) = broadcaster.subscribe(WorkflowEventFilter::default());

        // Accept the daemon's reattach connect on a background thread so
        // try_reattach can complete.
        let writer_handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            // Send one wire event frame.
            let frame = serde_json::json!({
                "workflow_id": "wf-reattach-1",
                "kind": "phase_started",
                "payload": {"phase": "implementation"},
                "occurred_at": chrono::Utc::now()
            });
            let mut line = serde_json::to_string(&frame).unwrap();
            line.push('\n');
            stream.write_all(line.as_bytes()).unwrap();
            stream.flush().unwrap();
            // Keep the stream alive briefly so the reader can drain.
            std::thread::sleep(std::time::Duration::from_millis(100));
        });

        let _conn = try_reattach(&path, broadcaster.clone()).expect("reattach connect");

        let item = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("subscriber timeout")
            .expect("subscriber channel closed");
        match item {
            crate::control::SubscriberItem::Event(event) => {
                assert_eq!(event.workflow_id, "wf-reattach-1");
                assert_eq!(event.kind, "phase_started");
            }
            crate::control::SubscriberItem::Closed { reason } => {
                panic!("unexpected close item: {reason}");
            }
        }

        writer_handle.join().unwrap();
    }
}
