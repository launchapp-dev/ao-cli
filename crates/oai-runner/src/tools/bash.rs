use anyhow::{anyhow, Result};
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

const COMMAND_TIMEOUT_EXIT_MESSAGE: &str = "Command timed out after";
const MAX_OUTPUT_CHARS: usize = 50_000;

#[cfg(unix)]
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

const SAFE_ENV_VARS: &[&str] = &[
    "HOME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LOGNAME",
    "PATH",
    "SHELL",
    "TERM",
    "TMPDIR",
    "USER",
    "XDG_RUNTIME_DIR",
];

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

pub async fn execute_command(working_dir: &Path, command: &str, timeout_secs: Option<u64>) -> Result<String> {
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(300));

    let mut shell_command = Command::new("sh");
    shell_command
        .arg("-c")
        .arg(command)
        .current_dir(working_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    apply_sanitized_environment(&mut shell_command);

    #[cfg(unix)]
    {
        shell_command.process_group(0);
    }

    let mut child = shell_command.spawn().map_err(|e| anyhow!("Failed to spawn command: {}", e))?;
    let pid = child.id();

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

    let stdout_handle = tokio::spawn(async move {
        let mut buffer = Vec::new();
        stdout.read_to_end(&mut buffer).await.map(|_| buffer)
    });
    let stderr_handle = tokio::spawn(async move {
        let mut buffer = Vec::new();
        stderr.read_to_end(&mut buffer).await.map(|_| buffer)
    });

    let exit_status = {
        let wait_for_exit = child.wait();
        tokio::pin!(wait_for_exit);

        tokio::select! {
            result = &mut wait_for_exit => {
                Some(result.map_err(|e| anyhow!("Failed to wait for command: {}", e))?)
            }
            _ = tokio::time::sleep(timeout) => None,
        }
    };

    if exit_status.is_none() {
        terminate_child_process_tree(&mut child, pid).await;
        let _ = child
            .wait()
            .await
            .map_err(|e| anyhow!("Failed to reap timed out command: {}", e))?;
    }

    let stdout = join_reader(stdout_handle, "stdout").await?;
    let stderr = join_reader(stderr_handle, "stderr").await?;

    if exit_status.is_none() {
        return Ok(format!("{} {}s", COMMAND_TIMEOUT_EXIT_MESSAGE, timeout.as_secs()));
    }

    let status = exit_status.expect("status must exist when command did not time out");

    Ok(format_command_result(stdout, stderr, status))
}

pub(crate) fn sanitized_environment() -> Vec<(OsString, OsString)> {
    sanitize_environment_entries(std::env::vars_os())
}

pub(crate) fn sanitize_environment_entries<I>(entries: I) -> Vec<(OsString, OsString)>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let mut env = Vec::new();
    let mut saw_path = false;

    for (key, value) in entries {
        let Some(name) = key.to_str() else {
            continue;
        };
        if !is_safe_env_var(name) {
            continue;
        }
        if name == "PATH" {
            saw_path = true;
        }
        env.push((key, value));
    }

    if !saw_path {
        env.push((OsString::from("PATH"), default_path_value()));
    }

    env
}

fn apply_sanitized_environment(command: &mut Command) {
    command.env_clear();
    for (key, value) in sanitized_environment() {
        command.env(key, value);
    }
}

fn is_safe_env_var(name: &str) -> bool {
    SAFE_ENV_VARS.contains(&name) || name.starts_with("LC_")
}

fn default_path_value() -> OsString {
    #[cfg(unix)]
    {
        OsString::from(DEFAULT_PATH)
    }

    #[cfg(not(unix))]
    {
        std::env::var_os("PATH").unwrap_or_default()
    }
}

async fn join_reader(handle: tokio::task::JoinHandle<Result<Vec<u8>, std::io::Error>>, stream_name: &str) -> Result<String> {
    let bytes = handle
        .await
        .map_err(|e| anyhow!("Failed to join {} reader task: {}", stream_name, e))?
        .map_err(|e| anyhow!("Failed to read {}: {}", stream_name, e))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn format_command_result(stdout: String, stderr: String, status: std::process::ExitStatus) -> String {
    let mut result = String::new();

    if !stdout.is_empty() {
        result.push_str(&stdout);
    }

    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("[stderr]\n");
        result.push_str(&stderr);
    }

    if !status.success() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&exit_status_message(status));
    }

    truncate_result(result)
}

fn exit_status_message(status: std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("[exit code: {}]", code);
    }

    #[cfg(unix)]
    {
        if let Some(signal) = status.signal() {
            return format!("[terminated by signal: {}]", signal);
        }
    }

    "[process terminated unsuccessfully]".to_string()
}

fn truncate_result(result: String) -> String {
    let char_count = result.chars().count();
    if char_count <= MAX_OUTPUT_CHARS {
        return result;
    }

    let truncated: String = result.chars().take(MAX_OUTPUT_CHARS).collect();
    format!("{}...\n[output truncated at {} chars]", truncated, MAX_OUTPUT_CHARS)
}

async fn terminate_child_process_tree(child: &mut Child, pid: Option<u32>) {
    #[cfg(unix)]
    {
        if let Some(pid) = pid {
            if kill_process_group(pid).await.is_ok() {
                return;
            }
        }
    }

    let _ = child.kill().await;
}

#[cfg(unix)]
async fn kill_process_group(pid: u32) -> Result<()> {
    let status = Command::new("kill")
        .arg("-KILL")
        .arg(format!("-{}", pid))
        .status()
        .await
        .map_err(|e| anyhow!("failed to invoke kill for pid {}: {}", pid, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("failed to kill process group for pid {}", pid))
    }
}
