//! Daemon liveness and control-socket reachability checks.

use std::path::Path;
use std::time::SystemTime;

use orchestrator_daemon_runtime::control::{control_socket_path, ControlClient};
use orchestrator_daemon_runtime::DaemonRuntimeState;

use super::check_kit::{CheckContext, CheckFix, CheckStatus, DiagnosticCheck};

const CATEGORY: &str = "daemon";

pub(crate) fn run(ctx: &CheckContext) -> Vec<DiagnosticCheck> {
    let mut out = Vec::new();

    let project_root_str = ctx.project_root.to_string_lossy().to_string();
    let pid = DaemonRuntimeState::read_daemon_pid_file(&project_root_str);

    let pid_check = match pid {
        Some(pid) => {
            let alive = pid_alive(pid);
            if alive {
                DiagnosticCheck::new("daemon_pid", CATEGORY, CheckStatus::Pass, "Daemon process alive")
                    .details(format!("daemon pid {pid} is alive"))
            } else {
                DiagnosticCheck::new("daemon_pid", CATEGORY, CheckStatus::Fail, "Daemon process alive")
                    .current(format!("pid {pid} recorded but not running"))
                    .expected("daemon process running with recorded pid")
                    .fix(CheckFix::auto(
                        "remove_stale_pid",
                        "Remove the stale pid file so the next `daemon start` can claim it.",
                        "animus daemon start --auto-install",
                    ))
            }
        }
        None => DiagnosticCheck::new("daemon_pid", CATEGORY, CheckStatus::Warn, "Daemon process alive")
            .current("no pid file recorded".to_string())
            .expected("daemon running with pid file present".to_string())
            .fix(CheckFix::command(
                "start_daemon",
                "Start the daemon (with auto-install of any missing plugins).",
                "animus daemon start --auto-install",
            )),
    };
    out.push(pid_check);

    let socket_path = control_socket_path(&ctx.project_root);
    if socket_path.exists() {
        out.push(
            DiagnosticCheck::new("daemon_control_socket_present", CATEGORY, CheckStatus::Pass, "Daemon control socket")
                .details(format!("control socket at {}", socket_path.display())),
        );
    } else {
        out.push(
            DiagnosticCheck::new("daemon_control_socket_present", CATEGORY, CheckStatus::Warn, "Daemon control socket")
                .current(format!("no socket at {}", socket_path.display()))
                .expected("control.sock created by running daemon".to_string())
                .fix(CheckFix::command(
                    "start_daemon",
                    "Start the daemon to create the control socket.",
                    "animus daemon start --auto-install",
                )),
        );
    }

    // Attempt a real RPC roundtrip — only when the socket is present so we
    // don't double-report a stopped daemon. Runs on the current tokio handle
    // if available; otherwise spins a fresh single-thread runtime.
    if socket_path.exists() && !ctx.skip_subprocess {
        let probe = probe_daemon_health(&ctx.project_root);
        match probe {
            Ok(status) => out.push(
                DiagnosticCheck::new("daemon_health_rpc", CATEGORY, CheckStatus::Pass, "Daemon health RPC")
                    .details(format!("daemon reported {status}")),
            ),
            Err(reason) => out.push(
                DiagnosticCheck::new("daemon_health_rpc", CATEGORY, CheckStatus::Fail, "Daemon health RPC")
                    .current(reason)
                    .expected("daemon.health returns Healthy".to_string())
                    .fix(CheckFix::command(
                        "restart_daemon",
                        "Restart the daemon to recover from a wedged control loop.",
                        "animus daemon stop && animus daemon start --auto-install",
                    )),
            ),
        }
    }

    out
}

fn probe_daemon_health(project_root: &Path) -> Result<String, String> {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return Err(format!("failed to build tokio runtime: {e}")),
    };
    rt.block_on(async move {
        let started = SystemTime::now();
        let client = match ControlClient::try_connect(project_root).await {
            Ok(Some(c)) => c,
            Ok(None) => return Err("control socket disappeared between checks".to_string()),
            Err(e) => return Err(format!("failed to connect: {e}")),
        };
        match client.daemon_health().await {
            Ok(response) => {
                let elapsed = started.elapsed().unwrap_or_default();
                Ok(format!("{:?} (RPC took {}ms)", response.status, elapsed.as_millis()))
            }
            Err(e) => Err(format!("daemon.health RPC failed: {e}")),
        }
    })
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // Shell out to `kill -0 <pid>` instead of calling `libc::kill` directly:
    // workspace lints forbid `unsafe`, and the spawn cost (one fork+exec) is
    // negligible compared to the doctor's other checks.
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn pid_alive(_pid: u32) -> bool {
    // Platforms without unix kill(): fall back to "trust the pid file".
    true
}
