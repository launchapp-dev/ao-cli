use orchestrator_core::{DaemonHealth, DaemonStatus, TaskStatistics};

#[derive(Debug, Clone, Default)]
pub(crate) struct DaemonSnapshot {
    pub(crate) status: String,
    pub(crate) healthy: bool,
    pub(crate) active_agents: usize,
    pub(crate) max_agents: Option<usize>,
    pub(crate) runner_connected: bool,
    pub(crate) tasks_ready: usize,
    pub(crate) tasks_in_progress: usize,
    pub(crate) tasks_blocked: usize,
    pub(crate) tasks_total: usize,
    pub(crate) error: Option<String>,
}

impl DaemonSnapshot {
    pub(crate) fn from_health_and_stats(health: DaemonHealth, stats: TaskStatistics) -> Self {
        let status = match health.status {
            DaemonStatus::Starting => "starting",
            DaemonStatus::Running => "running",
            DaemonStatus::Paused => "paused",
            DaemonStatus::Stopping => "stopping",
            DaemonStatus::Stopped => "stopped",
            DaemonStatus::Crashed => "crashed",
        };
        Self {
            status: status.to_string(),
            healthy: health.healthy,
            active_agents: health.active_agents,
            max_agents: health.max_agents,
            runner_connected: health.runner_connected,
            tasks_ready: stats.by_status.get("ready").copied().unwrap_or(0),
            tasks_in_progress: stats.in_progress,
            tasks_blocked: stats.blocked,
            tasks_total: stats.total,
            error: None,
        }
    }

    pub(crate) fn from_error(msg: String) -> Self {
        Self {
            status: "unknown".to_string(),
            error: Some(msg),
            ..Default::default()
        }
    }

    pub(crate) fn is_running_or_paused(&self) -> bool {
        matches!(self.status.as_str(), "running" | "paused" | "starting" | "stopping")
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.status == "paused"
    }

    pub(crate) fn daemon_lines(&self) -> Vec<String> {
        if let Some(ref err) = self.error {
            return vec![format!("error: {err}")];
        }

        let health_marker = if self.healthy { "OK" } else { "!" };
        let runner_label = if self.runner_connected { "connected" } else { "disconnected" };
        let agents_label = match self.max_agents {
            Some(max) => format!("{}/{}", self.active_agents, max),
            None => self.active_agents.to_string(),
        };

        vec![
            format!("Status:  {} [{}]", self.status, health_marker),
            format!("Runner:  {}", runner_label),
            format!("Agents:  {}", agents_label),
            String::new(),
            "Queue:".to_string(),
            format!("  ready       {}", self.tasks_ready),
            format!("  in-progress {}", self.tasks_in_progress),
            format!("  blocked     {}", self.tasks_blocked),
            format!("  total       {}", self.tasks_total),
        ]
    }
}
