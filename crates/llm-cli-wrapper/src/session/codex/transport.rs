use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::cli::{
    ensure_codex_config_override, ensure_flag, parse_launch_from_runtime_contract, LaunchInvocation,
};
use crate::error::{Error, Result};

use super::parser::parse_codex_stdout_line;
use crate::session::{
    session_event::SessionEvent, session_request::SessionRequest, session_run::SessionRun,
};

pub(crate) async fn start_codex_session(
    request: SessionRequest,
    resume_last_turn: bool,
) -> Result<SessionRun> {
    let invocation = codex_invocation_for_request(&request, resume_last_turn)?;
    let session_id = request
        .extras
        .pointer("/runtime_contract/cli/session/session_id")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let (event_tx, event_rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let _ = event_tx
            .send(SessionEvent::Started {
                backend: "codex-native".to_string(),
                session_id,
            })
            .await;

        if let Err(error) = run_codex_session(request, invocation, event_tx.clone()).await {
            let _ = event_tx
                .send(SessionEvent::Error {
                    message: error.to_string(),
                    recoverable: false,
                })
                .await;
            let _ = event_tx
                .send(SessionEvent::Finished { exit_code: Some(1) })
                .await;
        }
    });

    Ok(SessionRun {
        session_id: None,
        events: event_rx,
        selected_backend: "codex-native".to_string(),
        fallback_reason: None,
    })
}

pub(crate) fn codex_invocation_for_request(
    request: &SessionRequest,
    resume_last_turn: bool,
) -> Result<LaunchInvocation> {
    if let Some(invocation) =
        parse_launch_from_runtime_contract(request.extras.get("runtime_contract"))?
    {
        return Ok(invocation);
    }

    let mut args = vec!["exec".to_string()];
    if resume_last_turn {
        args.push("resume".to_string());
        args.push("--last".to_string());
    }
    args.push("--json".to_string());
    args.push("--full-auto".to_string());
    args.push("--skip-git-repo-check".to_string());

    if let Some(permission_mode) = request
        .permission_mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        ensure_codex_config_override(
            &mut args,
            "approval_policy",
            &format!("\"{permission_mode}\""),
        );
    }

    ensure_codex_config_override(&mut args, "sandbox_workspace_write.network_access", "true");

    if !request.model.trim().is_empty() {
        args.push("--model".to_string());
        args.push(request.model.clone());
    }

    args.push(request.prompt.clone());

    let mut invocation = LaunchInvocation {
        command: "codex".to_string(),
        args,
        prompt_via_stdin: false,
    };
    ensure_flag(&mut invocation.args, "--json", 1);

    Ok(invocation)
}

async fn run_codex_session(
    request: SessionRequest,
    invocation: LaunchInvocation,
    event_tx: mpsc::Sender<SessionEvent>,
) -> Result<()> {
    let mut child = Command::new(&invocation.command)
        .args(&invocation.args)
        .current_dir(&request.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        if invocation.prompt_via_stdin && !request.prompt.is_empty() {
            stdin.write_all(request.prompt.as_bytes()).await?;
        }
        drop(stdin);
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::ExecutionFailed("failed to capture codex stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::ExecutionFailed("failed to capture codex stderr".to_string()))?;

    let stdout_tx = event_tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut last_final_text: Option<String> = None;
        let mut lines = BufReader::new(stdout).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            for event in parse_codex_stdout_line(&line) {
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
            let _ = stderr_tx
                .send(SessionEvent::Error {
                    message: line,
                    recoverable: true,
                })
                .await;
        }
    });

    let exit_code = wait_for_codex_child(&mut child, request.timeout_secs).await?;

    let _ = stdout_task.await;
    let _ = stderr_task.await;

    let _ = event_tx.send(SessionEvent::Finished { exit_code }).await;

    Ok(())
}

async fn wait_for_codex_child(child: &mut Child, timeout_secs: Option<u64>) -> Result<Option<i32>> {
    match timeout_secs {
        Some(secs) => match timeout(Duration::from_secs(secs), child.wait()).await {
            Ok(status) => Ok(status?.code()),
            Err(_) => {
                child.kill().await?;
                Err(Error::ExecutionFailed(format!(
                    "codex session timed out after {} seconds",
                    secs
                )))
            }
        },
        None => Ok(child.wait().await?.code()),
    }
}
