use chrono::{DateTime, Utc};
use orchestrator_core::{DaemonHealth, DaemonStatus, TaskStatistics};

#[derive(Debug, Clone)]
pub(crate) struct DaemonSnapshot {
    pub(crate) daemon_health: Option<DaemonHealth>,
    pub(crate) task_stats: Option<TaskStatistics>,
    pub(crate) recent_errors: Vec<ErrorEntry>,
    pub(crate) last_refresh: DateTime<Utc>,
    pub(crate) status_line: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ErrorEntry {
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) level: String,
    pub(crate) message: String,
}

impl DaemonSnapshot {
    pub(crate) fn new() -> Self {
        Self {
            daemon_health: None,
            task_stats: None,
            recent_errors: Vec::new(),
            last_refresh: Utc::now(),
            status_line: "Initializing...".to_string(),
        }
    }

    pub(crate) fn daemon_status(&self) -> &'static str {
        self.daemon_health
            .as_ref()
            .map(|h| match h.status {
                DaemonStatus::Starting => "Starting",
                DaemonStatus::Running => "Running",
                DaemonStatus::Paused => "Paused",
                DaemonStatus::Stopping => "Stopping",
                DaemonStatus::Stopped => "Stopped",
                DaemonStatus::Crashed => "Crashed",
            })
            .unwrap_or("Unknown")
    }

    pub(crate) fn is_daemon_running(&self) -> bool {
        matches!(
            self.daemon_health.as_ref().map(|h| h.status),
            Some(DaemonStatus::Running) | Some(DaemonStatus::Paused)
        )
    }

    pub(crate) fn is_runner_connected(&self) -> bool {
        self.daemon_health
            .as_ref()
            .map(|h| h.runner_connected)
            .unwrap_or(false)
    }

    pub(crate) fn active_agents(&self) -> usize {
        self.daemon_health
            .as_ref()
            .map(|h| h.active_agents)
            .unwrap_or(0)
    }

    pub(crate) fn max_agents(&self) -> Option<usize> {
        self.daemon_health.as_ref().and_then(|h| h.max_agents)
    }

    pub(crate) fn daemon_pid(&self) -> Option<u32> {
        self.daemon_health.as_ref().and_then(|h| h.daemon_pid)
    }

    pub(crate) fn runner_pid(&self) -> Option<u32> {
        self.daemon_health.as_ref().and_then(|h| h.runner_pid)
    }

    pub(crate) fn task_ready(&self) -> usize {
        self.task_stats
            .as_ref()
            .map(|s| s.by_status.get("ready").copied().unwrap_or(0))
            .unwrap_or(0)
    }

    pub(crate) fn task_in_progress(&self) -> usize {
        self.task_stats.as_ref().map(|s| s.in_progress).unwrap_or(0)
    }

    pub(crate) fn task_blocked(&self) -> usize {
        self.task_stats.as_ref().map(|s| s.blocked).unwrap_or(0)
    }

    pub(crate) fn task_on_hold(&self) -> usize {
        self.task_stats
            .as_ref()
            .map(|s| s.by_status.get("on_hold").copied().unwrap_or(0))
            .unwrap_or(0)
    }

    pub(crate) fn task_total(&self) -> usize {
        self.task_stats.as_ref().map(|s| s.total).unwrap_or(0)
    }
}

impl Default for DaemonSnapshot {
    fn default() -> Self {
        Self::new()
    }
}
