//! Generic sink for workflow lifecycle events surfaced by
//! [`crate::workflow_execute::execute_workflow`].
//!
//! The runner emits one [`RuntimeWorkflowEvent`] per phase boundary and one
//! per workflow terminal status. The daemon wires this trait to a
//! `WorkflowEventBroadcaster` so subscribers on the control socket receive
//! the same events; the CLI binary uses [`NoopWorkflowEventEmitter`] when
//! running outside the daemon.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::sync::Arc;

/// Kind discriminator for a [`RuntimeWorkflowEvent`].
///
/// Mirrors the `kind` string values the protocol layer emits on the wire
/// (`workflow_events`). Kept as an enum here so the runner cannot
/// mis-spell a kind; the emitter implementation maps to the wire string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWorkflowEventKind {
    PhaseStarted,
    PhaseCompleted,
    WorkflowCompleted,
    WorkflowFailed,
}

impl RuntimeWorkflowEventKind {
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::PhaseStarted => "phase_started",
            Self::PhaseCompleted => "phase_completed",
            Self::WorkflowCompleted => "workflow_completed",
            Self::WorkflowFailed => "workflow_failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeWorkflowEvent {
    pub workflow_id: String,
    pub kind: RuntimeWorkflowEventKind,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

pub trait WorkflowEventEmitter: Send + Sync {
    fn emit(&self, event: RuntimeWorkflowEvent);
}

pub type SharedWorkflowEventEmitter = Arc<dyn WorkflowEventEmitter>;

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopWorkflowEventEmitter;

impl WorkflowEventEmitter for NoopWorkflowEventEmitter {
    fn emit(&self, _event: RuntimeWorkflowEvent) {}
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct RecordingEmitter {
        events: Mutex<Vec<RuntimeWorkflowEvent>>,
    }

    impl RecordingEmitter {
        pub fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        pub fn snapshot(&self) -> Vec<RuntimeWorkflowEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl WorkflowEventEmitter for RecordingEmitter {
        fn emit(&self, event: RuntimeWorkflowEvent) {
            self.events.lock().unwrap().push(event);
        }
    }
}
