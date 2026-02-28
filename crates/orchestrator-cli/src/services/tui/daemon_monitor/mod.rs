mod render;
mod state;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use orchestrator_core::services::ServiceHub;
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::DaemonMonitorArgs;
use state::{DaemonSnapshot, ErrorEntry};

pub(crate) async fn handle_daemon_monitor(
    args: DaemonMonitorArgs,
    hub: Arc<dyn ServiceHub>,
    json: bool,
) -> Result<()> {
    if json {
        return Err(anyhow!("`ao daemon-monitor` does not support --json output"));
    }

    let refresh_interval = Duration::from_secs(args.refresh_interval);
    let mut state = DaemonSnapshot::new();

    let project_root = hub.daemon().health().await.ok()
        .and_then(|h| h.project_root.clone())
        .unwrap_or_default();

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let run_result = run_event_loop(
        &mut terminal,
        &mut state,
        &hub,
        &project_root,
        refresh_interval,
    )
    .await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    run_result
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut DaemonSnapshot,
    hub: &Arc<dyn ServiceHub>,
    project_root: &str,
    refresh_interval: Duration,
) -> Result<()> {
    let mut last_refresh = Instant::now();
    refresh_daemon_state(state, hub, project_root).await;

    loop {
        terminal.draw(|frame| render::render(frame, state))?;

        if event::poll(Duration::from_millis(100))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('c')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    break;
                }
                KeyCode::Char('d') => {
                    let daemon = hub.daemon();
                    let is_running = state.is_daemon_running();
                    let result = if is_running {
                        daemon.stop().await
                    } else {
                        daemon.start().await
                    };
                    match result {
                        Ok(()) => {
                            state.status_line = if is_running {
                                "Stopping daemon...".to_string()
                            } else {
                                "Starting daemon...".to_string()
                            };
                        }
                        Err(e) => {
                            state.status_line = format!("Error: {e}");
                        }
                    }
                    refresh_daemon_state(state, hub, project_root).await;
                }
                KeyCode::Char('p') => {
                    let daemon = hub.daemon();
                    let is_running = state.is_daemon_running();
                    if is_running {
                        match daemon.pause().await {
                            Ok(()) => state.status_line = "Daemon paused".to_string(),
                            Err(e) => state.status_line = format!("Pause error: {e}"),
                        }
                    } else if matches!(state.daemon_health.as_ref().map(|h| h.status), Some(orchestrator_core::DaemonStatus::Paused)) {
                        match daemon.resume().await {
                            Ok(()) => state.status_line = "Daemon resumed".to_string(),
                            Err(e) => state.status_line = format!("Resume error: {e}"),
                        }
                    }
                    refresh_daemon_state(state, hub, project_root).await;
                }
                KeyCode::Char('r') => {
                    refresh_daemon_state(state, hub, project_root).await;
                    last_refresh = Instant::now();
                }
                _ => {}
            }
        }

        if last_refresh.elapsed() >= refresh_interval {
            refresh_daemon_state(state, hub, project_root).await;
            last_refresh = Instant::now();
        }
    }

    Ok(())
}

async fn refresh_daemon_state(
    state: &mut DaemonSnapshot,
    hub: &Arc<dyn ServiceHub>,
    project_root: &str,
) {
    let daemon = hub.daemon();
    let tasks = hub.tasks();

    let daemon_result = daemon.health().await;
    let task_stats_result = tasks.statistics().await;

    if let Ok(health) = daemon_result {
        state.daemon_health = Some(health);
        state.status_line = "Connected".to_string();
    } else {
        state.daemon_health = None;
        state.status_line = "Daemon unavailable".to_string();
    }

    if let Ok(stats) = task_stats_result {
        state.task_stats = Some(stats);
    }

    state.recent_errors = load_recent_errors(project_root).await;
    state.last_refresh = Utc::now();
}

async fn load_recent_errors(project_root: &str) -> Vec<ErrorEntry> {
    let errors_path = PathBuf::from(project_root).join(".ao/errors.jsonl");
    
    if !errors_path.exists() {
        return Vec::new();
    }

    let path = errors_path.clone();
    tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines().rev().take(5) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                    let timestamp = parsed
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(Utc::now);
                    
                    let level = parsed
                        .get("level")
                        .and_then(|v| v.as_str())
                        .unwrap_or("info")
                        .to_string();
                    
                    let message = parsed
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    
                    entries.push(ErrorEntry {
                        timestamp,
                        level,
                        message,
                    });
                }
            }
        }
        entries
    })
    .await
    .unwrap_or_default()
}
