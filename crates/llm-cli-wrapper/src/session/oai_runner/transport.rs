use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::cli::{ensure_flag_value, parse_launch_from_runtime_contract, LaunchInvocation};
use crate::error::{Error, Result};
use crate::session::{session_event::SessionEvent, session_request::SessionRequest, session_run::SessionRun};

use super::parser::parse_oai_runner_json_line;

struct SessionControlEntry {
    cancel_tx: oneshot::Sender<()>,
    pid: Option<u32>,
}

pub(crate) async fn start_oai_runner_session(
    request: SessionRequest,
    resume_session_id: Option<String>,
) -> Result<SessionRun> {
    let invocation = oai_runner_invocation_for_request(&request, resume_session_id.as_deref())?;
    let control_session_id = Uuid::new_v4().to_string();
    let control_session_id_for_run = control_session_id.clone();
    let (event_tx, event_rx) = mpsc::channel(128);
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let (pid_tx, pid_rx) = oneshot::channel::<Option<u32>>();
    register_session(control_session_id.clone(), cancel_tx);

    tokio::spawn(async move {
        let backend_label = "oai-runner-native".to_string();
        let session_id_for_event = Some(control_session_id.clone());

        if let Err(error) = run_oai_runner_session(
            request,
            invocation,
            event_tx.clone(),
            cancel_rx,
            pid_tx,
            backend_label,
            session_id_for_event,
        )
        .await
        {
            let _ = event_tx.send(SessionEvent::Error { message: error.to_string(), recoverable: false }).await;
            let _ = event_tx.send(SessionEvent::Finished { exit_code: Some(1) }).await;
        }
        unregister_session(&control_session_id);
    });

    let pid = pid_rx.await.ok().flatten();
    Ok(SessionRun {
        session_id: Some(control_session_id_for_run),
        events: event_rx,
        selected_backend: "oai-runner-native".to_string(),
        fallback_reason: None,
        pid,
    })
}

pub(crate) async fn terminate_oai_runner_session(session_id: &str) -> Result<()> {
    let Some(entry) = take_session(session_id) else {
        return Err(Error::ExecutionFailed(format!("oai-runner backend has no active session for '{}'", session_id)));
    };
    let _ = entry.cancel_tx.send(());
    if let Some(pid) = entry.pid {
        kill_process_group(pid);
    }
    Ok(())
}

pub(crate) fn oai_runner_invocation_for_request(
    request: &SessionRequest,
    resume_session_id: Option<&str>,
) -> Result<LaunchInvocation> {
    if let Some(mut invocation) = parse_launch_from_runtime_contract(request.extras.get("runtime_contract"))? {
        inject_runtime_contract_mcp_config(&mut invocation, request.extras.get("runtime_contract"))?;
        return Ok(invocation);
    }

    let mut args = vec!["run".to_string()];
    if !request.model.trim().is_empty() {
        args.push("-m".to_string());
        args.push(request.model.clone());
    }
    args.push("--format".to_string());
    args.push("json".to_string());
    if let Some(session_id) = resume_session_id.map(str::trim).filter(|value| !value.is_empty()) {
        args.push("--session-id".to_string());
        args.push(session_id.to_string());
    }
    args.push(request.prompt.clone());

    let mut invocation = LaunchInvocation {
        command: "ao-oai-runner".to_string(),
        args,
        env: Default::default(),
        prompt_via_stdin: false,
    };
    inject_runtime_contract_mcp_config(&mut invocation, request.extras.get("runtime_contract"))?;
    ensure_flag_value(&mut invocation.args, "--format", "json", 1);
    Ok(invocation)
}

fn inject_runtime_contract_mcp_config(
    invocation: &mut LaunchInvocation,
    runtime_contract: Option<&Value>,
) -> Result<()> {
    let Some(config_json) = runtime_contract_mcp_config_json(runtime_contract) else {
        return Ok(());
    };

    let insert_at = invocation.args.iter().position(|entry| entry == "run").map(|index| index + 1).unwrap_or(0);
    ensure_flag_value(&mut invocation.args, "--mcp-config", &config_json, insert_at);
    Ok(())
}

fn runtime_contract_mcp_config_json(runtime_contract: Option<&Value>) -> Option<String> {
    let contract = runtime_contract?;
    let mut servers = Vec::new();

    if let Some(url) =
        contract.pointer("/mcp/endpoint").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty())
    {
        servers.push(serde_json::json!({
            "url": url,
            "transport": "http"
        }));
    } else if let Some(command) =
        contract.pointer("/mcp/stdio/command").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty())
    {
        let args = contract
            .pointer("/mcp/stdio/args")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let env = contract
            .pointer("/mcp/stdio/env")
            .and_then(Value::as_object)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        servers.push(serde_json::json!({
            "command": command,
            "args": args,
            "env": env,
        }));
    }

    if let Some(additional) = contract.pointer("/mcp/additional_servers").and_then(Value::as_object) {
        for entry in additional.values() {
            if let Some(url) = entry.get("url").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty())
            {
                servers.push(serde_json::json!({
                    "url": url,
                    "transport": "http"
                }));
                continue;
            }

            let Some(command) =
                entry.get("command").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty())
            else {
                continue;
            };

            let args = entry
                .get("args")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let env = entry
                .get("env")
                .and_then(Value::as_object)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string())))
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            servers.push(serde_json::json!({
                "command": command,
                "args": args,
                "env": env,
            }));
        }
    }

    if servers.is_empty() {
        None
    } else {
        serde_json::to_string(&servers).ok()
    }
}

async fn run_oai_runner_session(
    request: SessionRequest,
    invocation: LaunchInvocation,
    event_tx: mpsc::Sender<SessionEvent>,
    mut cancel_rx: oneshot::Receiver<()>,
    pid_tx: oneshot::Sender<Option<u32>>,
    backend: String,
    session_id: Option<String>,
) -> Result<()> {
    let mut command = Command::new(&invocation.command);
    command
        .args(&invocation.args)
        .current_dir(&request.cwd)
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env_remove("CLAUDE_CODE_SESSION_ACCESS_TOKEN")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .envs(request.env_vars.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn()?;
    let pid = child.id();
    let _ = pid_tx.send(pid);

    if let (Some(pid), Some(control_session_id)) = (pid, session_id.as_deref()) {
        if !record_session_pid(control_session_id, pid) {
            crate::session::kill_and_reap_child(&mut child).await;
            return Err(Error::ExecutionFailed("oai-runner session cancelled".to_string()));
        }
    }

    let _ = event_tx.send(SessionEvent::Started { backend, session_id, pid }).await;

    if let Some(mut stdin) = child.stdin.take() {
        if invocation.prompt_via_stdin && !request.prompt.is_empty() {
            stdin.write_all(request.prompt.as_bytes()).await?;
        }
        drop(stdin);
    }

    let stdout =
        child.stdout.take().ok_or_else(|| Error::ExecutionFailed("failed to capture oai-runner stdout".to_string()))?;
    let stderr =
        child.stderr.take().ok_or_else(|| Error::ExecutionFailed("failed to capture oai-runner stderr".to_string()))?;

    let stdout_tx = event_tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut last_final_text: Option<String> = None;
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            for event in parse_oai_runner_json_line(&line) {
                if let SessionEvent::FinalText { text } = &event {
                    if last_final_text.as_deref() == Some(text.as_str()) {
                        continue;
                    }
                    last_final_text = Some(text.clone());
                }
                let _ = stdout_tx.send(event).await;
            }
        }
    });

    let stderr_tx = event_tx.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = stderr_tx.send(SessionEvent::Error { message: line, recoverable: true }).await;
        }
    });

    let exit_code = wait_for_child(&mut child, request.timeout_secs, &mut cancel_rx, "oai-runner").await?;
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    let _ = event_tx.send(SessionEvent::Finished { exit_code }).await;
    Ok(())
}

async fn wait_for_child(
    child: &mut Child,
    timeout_secs: Option<u64>,
    cancel_rx: &mut oneshot::Receiver<()>,
    label: &str,
) -> Result<Option<i32>> {
    match timeout_secs {
        Some(secs) => {
            let timeout_sleep = tokio::time::sleep(Duration::from_secs(secs));
            tokio::pin!(timeout_sleep);
            tokio::select! {
                status = child.wait() => Ok(status?.code()),
                _ = &mut timeout_sleep => {
                    crate::session::kill_and_reap_child(child).await;
                    Err(Error::ExecutionFailed(format!("{label} session timed out after {secs} seconds")))
                }
                _ = cancel_rx => {
                    crate::session::kill_and_reap_child(child).await;
                    Err(Error::ExecutionFailed(format!("{label} session cancelled")))
                }
            }
        }
        None => {
            tokio::select! {
                status = child.wait() => Ok(status?.code()),
                _ = cancel_rx => {
                    crate::session::kill_and_reap_child(child).await;
                    Err(Error::ExecutionFailed(format!("{label} session cancelled")))
                }
            }
        }
    }
}

fn session_registry() -> &'static Mutex<HashMap<String, SessionControlEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, SessionControlEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_session(session_id: String, cancel_tx: oneshot::Sender<()>) {
    if let Ok(mut registry) = session_registry().lock() {
        registry.insert(session_id, SessionControlEntry { cancel_tx, pid: None });
    }
}

fn unregister_session(session_id: &str) {
    if let Ok(mut registry) = session_registry().lock() {
        registry.remove(session_id);
    }
}

fn record_session_pid(session_id: &str, pid: u32) -> bool {
    if let Ok(mut registry) = session_registry().lock() {
        if let Some(entry) = registry.get_mut(session_id) {
            entry.pid = Some(pid);
            return true;
        }
    }
    false
}

fn take_session(session_id: &str) -> Option<SessionControlEntry> {
    session_registry().lock().ok().and_then(|mut registry| registry.remove(session_id))
}

fn kill_process_group(pid: u32) {
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("kill").args(["-9", &format!("-{}", pid)]).output();
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    use tokio::process::Command;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn registry_tracks_pid_and_cancel_token() {
        let session_id = "session-123".to_string();
        let (cancel_tx, cancel_rx) = oneshot::channel();

        register_session(session_id.clone(), cancel_tx);
        assert!(record_session_pid(&session_id, 4242));

        let entry = take_session(&session_id).expect("expected session control entry");
        assert_eq!(entry.pid, Some(4242));

        let _ = entry.cancel_tx.send(());
        assert!(cancel_rx.await.is_ok());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn kill_process_group_terminates_child_process_group() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 60"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        command.process_group(0);

        let mut child = command.spawn().expect("failed to spawn test child");
        let pid = child.id().expect("spawned child should have pid");

        kill_process_group(pid);

        let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("child should exit after process-group kill")
            .expect("child wait should succeed");
        assert!(!status.success());
    }

    #[test]
    fn invocation_derives_mcp_config_from_runtime_contract() {
        let request = SessionRequest {
            tool: "oai-runner".to_string(),
            model: "openrouter/minimax/minimax-m2.7".to_string(),
            prompt: "hello".to_string(),
            cwd: ".".into(),
            project_root: None,
            mcp_endpoints: vec![],
            permission_mode: None,
            timeout_secs: None,
            env_vars: vec![],
            extras: json!({
                "runtime_contract": {
                    "mcp": {
                        "endpoint": "https://primary.example/mcp",
                        "additional_servers": {
                            "docs": {
                                "url": "https://docs.example/mcp"
                            },
                            "project-stdio": {
                                "command": "project-mcp",
                                "args": ["serve"],
                                "env": {
                                    "PROJECT_TOKEN": "secret"
                                }
                            }
                        }
                    }
                }
            }),
        };

        let invocation = oai_runner_invocation_for_request(&request, None).expect("invocation should build");
        let mcp_idx =
            invocation.args.iter().position(|arg| arg == "--mcp-config").expect("mcp config flag should be present");
        let config_json = invocation.args.get(mcp_idx + 1).expect("mcp config payload should exist");
        let entries: Vec<Value> = serde_json::from_str(config_json).expect("mcp config should parse");
        assert_eq!(entries.len(), 3);
        assert!(entries
            .iter()
            .any(|entry| entry.get("url").and_then(Value::as_str) == Some("https://primary.example/mcp")));
        assert!(entries
            .iter()
            .any(|entry| entry.get("url").and_then(Value::as_str) == Some("https://docs.example/mcp")));
        assert!(entries.iter().any(|entry| {
            entry.get("command").and_then(Value::as_str) == Some("project-mcp")
                && entry.pointer("/env/PROJECT_TOKEN").and_then(Value::as_str) == Some("secret")
        }));
    }
}
