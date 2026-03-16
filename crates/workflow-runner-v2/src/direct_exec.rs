use agent_runner::runner::Supervisor;
use protocol::{AgentRunEvent, AgentRunRequest, AgentStatus};
use tokio::sync::mpsc;

pub async fn run_agent_direct(
    req: AgentRunRequest,
    event_tx: mpsc::Sender<AgentRunEvent>,
    cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> AgentStatus {
    let supervisor = Supervisor::new();
    supervisor.spawn_agent(req, event_tx, cancel_rx).await
}
