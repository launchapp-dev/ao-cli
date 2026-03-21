use anyhow::Result;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

/// Metadata returned from shell command execution, used for lifecycle reporting.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Formatted output string (stdout + stderr + exit code).
    pub output: String,
    /// Process exit code, or -1 on spawn failure / timeout.
    pub exit_code: i32,
    /// Whether the command was killed due to timeout.
    pub timed_out: bool,
    /// Wall-clock duration of the execution attempt.
    pub duration: Duration,
}

/// Default timeout applied when the caller doesn't specify one.
const DEFAULT_TIMEOUT_SECS: u64 = 300;
/// Maximum characters of output kept before truncation.
const OUTPUT_TRUNCATE_LIMIT: usize = 50_000;

pub async fn execute_command(
    working_dir: &Path,
    command: &str,
    timeout_secs: Option<u64>,
) -> Result<CommandResult> {
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let start = std::time::Instant::now();

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn command: {}", e))?;

    // Take stdout/stderr handles before the child is moved by wait().
    // We spawn reader tasks so they run concurrently with the child process.
    let stdout_handle = child.stdout.take().map(|mut pipe| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            tokio::io::copy(&mut pipe, &mut buf).await.ok();
            buf
        })
    });

    let stderr_handle = child.stderr.take().map(|mut pipe| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            tokio::io::copy(&mut pipe, &mut buf).await.ok();
            buf
        })
    });

    // Wait for the child process with the timeout. Because we use child.wait()
    // (not wait_with_output), the child is only mutably borrowed, so we can
    // still call start_kill() after the timeout fires.
    let wait_result = tokio::time::timeout(timeout, child.wait()).await;

    match wait_result {
        Ok(Ok(status)) => {
            let stdout_bytes = match stdout_handle {
                Some(h) => h.await.unwrap_or_default(),
                None => Vec::new(),
            };
            let stderr_bytes = match stderr_handle {
                Some(h) => h.await.unwrap_or_default(),
                None => Vec::new(),
            };

            let exit_code = status.code().unwrap_or(-1);
            let stdout = String::from_utf8_lossy(&stdout_bytes);
            let stderr = String::from_utf8_lossy(&stderr_bytes);
            let formatted = format_output(&stdout, &stderr, exit_code);

            Ok(CommandResult { output: formatted, exit_code, timed_out: false, duration: start.elapsed() })
        }
        Ok(Err(e)) => {
            // Spawn error (unlikely since we already spawned successfully).
            if let Some(h) = stdout_handle {
                h.abort();
            }
            if let Some(h) = stderr_handle {
                h.abort();
            }
            Ok(CommandResult {
                output: format!("Command failed: {}", e),
                exit_code: -1,
                timed_out: false,
                duration: start.elapsed(),
            })
        }
        Err(_) => {
            // Hard timeout: force-kill the child and reap the zombie process.
            let _ = child.start_kill();
            let _ = child.wait().await;
            // Abort the reader tasks — the child is dead so they'll get EOF
            // but abort ensures we don't wait needlessly.
            if let Some(h) = stdout_handle {
                h.abort();
            }
            if let Some(h) = stderr_handle {
                h.abort();
            }
            Ok(CommandResult {
                output: format!("Command timed out after {}s", timeout.as_secs()),
                exit_code: -1,
                timed_out: true,
                duration: start.elapsed(),
            })
        }
    }
}

/// Combine stdout, stderr, and exit code into the human-readable output string.
fn format_output(stdout: &str, stderr: &str, exit_code: i32) -> String {
    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("[stderr]\n");
        result.push_str(stderr);
    }

    if exit_code != 0 {
        result.push_str(&format!("\n[exit code: {}]", exit_code));
    }

    if result.len() > OUTPUT_TRUNCATE_LIMIT {
        let truncated = &result[..OUTPUT_TRUNCATE_LIMIT];
        format!("{}...\n[output truncated at {} chars]", truncated, OUTPUT_TRUNCATE_LIMIT)
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_command_returns_output() {
        let dir = std::env::temp_dir();
        let result = execute_command(&dir, "echo hello", None).await.unwrap();
        assert!(result.output.contains("hello"));
        assert_eq!(result.exit_code, 0);
        assert!(!result.timed_out);
        assert!(result.duration > Duration::ZERO);
    }

    #[tokio::test]
    async fn execute_command_captures_exit_code() {
        let dir = std::env::temp_dir();
        let result = execute_command(&dir, "exit 42", None).await.unwrap();
        assert_eq!(result.exit_code, 42);
        assert!(result.output.contains("[exit code: 42]"));
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn execute_command_captures_stderr() {
        let dir = std::env::temp_dir();
        let result = execute_command(&dir, "echo oops >&2", None).await.unwrap();
        assert!(result.output.contains("[stderr]"));
        assert!(result.output.contains("oops"));
    }

    #[tokio::test]
    async fn execute_command_respects_explicit_timeout() {
        let dir = std::env::temp_dir();
        // This should succeed quickly since "echo" is fast.
        let result = execute_command(&dir, "echo fast", Some(5)).await.unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.timed_out);
    }

    #[tokio::test]
    async fn execute_command_timeout_kills_and_reaps_child() {
        let dir = std::env::temp_dir();
        // Use a short timeout against a command that would run much longer.
        let result = execute_command(&dir, "sleep 60", Some(1)).await.unwrap();
        assert!(result.timed_out, "Expected timed_out flag to be true");
        assert_eq!(result.exit_code, -1);
        assert!(result.output.contains("timed out"));
        // Total wall-clock should be close to 1s, not 60s.
        assert!(
            result.duration < Duration::from_secs(10),
            "Timeout cleanup took too long: {:?}",
            result.duration
        );
    }

    #[tokio::test]
    async fn execute_command_timeout_is_reliable_under_concurrency() {
        let dir = std::env::temp_dir();
        // Fire several timed-out commands concurrently; none should leak.
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let dir = dir.clone();
                tokio::spawn(async move { execute_command(&dir, "sleep 60", Some(1)).await })
            })
            .collect();

        for h in handles {
            let result = h.await.unwrap().unwrap();
            assert!(result.timed_out);
        }
    }

    #[test]
    fn format_output_stdout_only() {
        let out = format_output("hello world\n", "", 0);
        assert_eq!(out, "hello world\n");
    }

    #[test]
    fn format_output_stderr_only() {
        let out = format_output("", "some error\n", 1);
        assert!(out.contains("[stderr]"));
        assert!(out.contains("some error"));
        assert!(out.contains("[exit code: 1]"));
    }

    #[test]
    fn format_output_both_streams() {
        let out = format_output("ok\n", "warning\n", 0);
        assert!(out.starts_with("ok\n"));
        assert!(out.contains("[stderr]"));
        assert!(out.contains("warning"));
        assert!(!out.contains("[exit code:"));
    }

    #[test]
    fn format_output_truncation() {
        let long = "x".repeat(OUTPUT_TRUNCATE_LIMIT + 1000);
        let out = format_output(&long, "", 0);
        assert!(out.contains("[output truncated"));
        assert!(out.len() < long.len() + 100);
    }
}
