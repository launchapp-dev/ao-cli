use anyhow::{Context, Result};
use protocol::{
    AgentControlRequest, AgentRunEvent, AgentRunRequest, AgentStatusRequest, ModelStatusRequest,
    RunnerStatusRequest,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use super::{auth, handlers};
use crate::runner::Runner;

#[cfg(test)]
const AUTH_PAYLOAD_TIMEOUT: Duration = Duration::from_millis(200);
#[cfg(not(test))]
const AUTH_PAYLOAD_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn truncate_for_log(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{}…", truncated)
}

fn event_kind(event: &AgentRunEvent) -> &'static str {
    match event {
        AgentRunEvent::Started { .. } => "started",
        AgentRunEvent::OutputChunk { .. } => "output_chunk",
        AgentRunEvent::Metadata { .. } => "metadata",
        AgentRunEvent::Error { .. } => "error",
        AgentRunEvent::Finished { .. } => "finished",
        AgentRunEvent::ToolCall { .. } => "tool_call",
        AgentRunEvent::ToolResult { .. } => "tool_result",
        AgentRunEvent::Artifact { .. } => "artifact",
        AgentRunEvent::Thinking { .. } => "thinking",
    }
}

pub(super) async fn write_json_line<W: AsyncWrite + Unpin, T: serde::Serialize>(
    writer: &mut W,
    payload: &T,
) -> Result<()> {
    let json = serde_json::to_string(payload)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

pub(super) async fn handle_connection<S>(
    stream: S,
    runner: Arc<Mutex<Runner>>,
    connection_id: u64,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half).lines();
    let mut authenticated = false;

    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<AgentRunEvent>(100);
    debug!(connection_id, "Connection event channel initialized");

    loop {
        if !authenticated {
            let line = match tokio::time::timeout(AUTH_PAYLOAD_TIMEOUT, reader.next_line()).await {
                Ok(line) => line.context("Failed to read IPC auth payload")?,
                Err(_) => {
                    warn!(
                        connection_id,
                        timeout_ms = AUTH_PAYLOAD_TIMEOUT.as_millis(),
                        "Closing IPC connection after auth timeout"
                    );
                    break;
                }
            };
            let Some(text) = line else {
                info!(connection_id, "Client closed IPC stream");
                break;
            };
            let text = text.trim();
            if text.is_empty() {
                continue;
            }

            match auth::authenticate_first_payload(text, &mut write_half, connection_id).await? {
                auth::AuthResult::Accepted => {
                    authenticated = true;
                    info!(connection_id, "IPC connection authenticated");
                }
                auth::AuthResult::Rejected => {
                    info!(
                        connection_id,
                        "Closing unauthenticated IPC connection after auth failure"
                    );
                    break;
                }
            }
            continue;
        }

        tokio::select! {
            line = reader.next_line() => {
                let Some(text) = line.context("Failed to read IPC message")? else {
                    info!(connection_id, "Client closed IPC stream");
                    break;
                };
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                debug!(
                    connection_id,
                    payload_bytes = text.len(),
                    payload_preview = %truncate_for_log(text, 240),
                    "Received IPC payload"
                );

                if let Ok(req) = serde_json::from_str::<AgentRunRequest>(text) {
                    handlers::run::handle_run_request(
                        req,
                        &runner,
                        &event_tx,
                        &mut write_half,
                        connection_id,
                    )
                    .await?;
                } else if let Ok(req) = serde_json::from_str::<ModelStatusRequest>(text) {
                    handlers::status::handle_model_status_request(
                        req,
                        &runner,
                        &mut write_half,
                        connection_id,
                    )
                    .await?;
                } else if let Ok(req) = serde_json::from_str::<AgentControlRequest>(text) {
                    handlers::control::handle_control_request(
                        req,
                        &runner,
                        &mut write_half,
                        connection_id,
                    )
                    .await?;
                } else if let Ok(req) = serde_json::from_str::<AgentStatusRequest>(text) {
                    handlers::status::handle_agent_status_request(
                        req,
                        &runner,
                        &mut write_half,
                        connection_id,
                    )
                    .await?;
                } else if let Ok(req) = serde_json::from_str::<RunnerStatusRequest>(text) {
                    handlers::status::handle_runner_status_request(
                        req,
                        &runner,
                        &mut write_half,
                        connection_id,
                    )
                    .await?;
                } else {
                    warn!(
                        connection_id,
                        payload_preview = %truncate_for_log(text, 600),
                        "Unrecognized IPC payload"
                    );
                }
            }
            Some(evt) = event_rx.recv() => {
                debug!(
                    connection_id,
                    event_kind = event_kind(&evt),
                    "Forwarding run event to client"
                );
                write_json_line(&mut write_half, &evt).await?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{IpcAuthFailureCode, IpcAuthResult};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    fn runner_for_test() -> Arc<Mutex<Runner>> {
        let (cleanup_tx, _cleanup_rx) = tokio::sync::mpsc::channel(1);
        Arc::new(Mutex::new(Runner::new(cleanup_tx)))
    }

    #[tokio::test]
    async fn rejects_non_auth_payload_as_first_message() {
        let (mut client, server) = tokio::io::duplex(1024);
        let server_task = tokio::spawn(handle_connection(server, runner_for_test(), 1001));

        write_json_line(&mut client, &RunnerStatusRequest::default())
            .await
            .expect("write runner status request");

        let mut reader = BufReader::new(client);
        let mut line = String::new();
        let read_len = tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut line))
            .await
            .expect("auth response timeout")
            .expect("read auth response");
        assert!(read_len > 0, "expected auth rejection response");

        let response: IpcAuthResult =
            serde_json::from_str(line.trim()).expect("parse auth rejection payload");
        assert!(!response.ok, "non-auth first payload must be rejected");
        assert_eq!(
            response.code,
            Some(IpcAuthFailureCode::MalformedAuthPayload)
        );

        line.clear();
        let eof_len = tokio::time::timeout(Duration::from_secs(1), reader.read_line(&mut line))
            .await
            .expect("socket close timeout")
            .expect("read socket close");
        assert_eq!(eof_len, 0, "server should close connection after rejection");

        server_task
            .await
            .expect("join server task")
            .expect("handle connection");
    }

    #[tokio::test]
    async fn closes_connection_when_auth_payload_times_out() {
        let (mut client, server) = tokio::io::duplex(1024);
        let server_task = tokio::spawn(handle_connection(server, runner_for_test(), 1002));

        let mut buf = [0_u8; 1];
        let read_len = tokio::time::timeout(
            AUTH_PAYLOAD_TIMEOUT + Duration::from_secs(1),
            client.read(&mut buf),
        )
        .await
        .expect("auth-timeout close window exceeded")
        .expect("read after timeout close");
        assert_eq!(
            read_len, 0,
            "server should close idle unauthenticated connection"
        );

        server_task
            .await
            .expect("join server task")
            .expect("handle connection");
    }
}
