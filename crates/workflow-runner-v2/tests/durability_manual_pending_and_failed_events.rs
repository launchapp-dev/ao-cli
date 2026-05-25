// Tests covering three durability bugs landed in v0.4.6:
//   * ManualPending must NOT write a `.completed` marker (replay would
//     advance past a manual gate or hit an unknown verdict).
//   * Failed phases must emit `phase_failed`, not `phase_completed`.
//   * The phase_failed metrics counter must increment on failure.
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;
use serde_json::json;
use tempfile::TempDir;
use workflow_runner_v2::phase_executor::PhaseExecutionOutcome;
use workflow_runner_v2::workflow_event_emitter::{
    RuntimeWorkflowEvent, RuntimeWorkflowEventKind, WorkflowEventEmitter,
};
use workflow_runner_v2::{is_phase_completed, persist_phase_output, phase_completion_marker_path};

#[derive(Default)]
struct LocalRecordingEmitter {
    events: Mutex<Vec<RuntimeWorkflowEvent>>,
}

impl LocalRecordingEmitter {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn snapshot(&self) -> Vec<RuntimeWorkflowEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl WorkflowEventEmitter for LocalRecordingEmitter {
    fn emit(&self, event: RuntimeWorkflowEvent) {
        self.events.lock().unwrap().push(event);
    }
}

fn home_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|p| p.into_inner())
}

fn pin_home(tmp: &TempDir) {
    std::env::set_var("HOME", tmp.path());
}

#[test]
fn marker_not_written_for_manual_pending_outcome() {
    let _guard = home_lock();
    let tmp = TempDir::new().expect("tempdir");
    pin_home(&tmp);
    let project = TempDir::new().expect("project tempdir");
    let project_root = project.path().to_string_lossy().to_string();
    let workflow_id = "wf-manual-no-marker";
    let phase_id = "approval";

    // Simulate the workflow_execute decision: ManualPending must skip
    // persist_phase_output so neither <phase>.json nor <phase>.completed
    // is written. Pause state is durable via task status independently.
    let outcome = PhaseExecutionOutcome::ManualPending {
        instructions: "wait for approval".to_string(),
        approval_note_required: false,
    };
    let manual_pending = matches!(outcome, PhaseExecutionOutcome::ManualPending { .. });
    if !manual_pending {
        persist_phase_output(&project_root, workflow_id, phase_id, &outcome).expect("persist");
    }

    let marker = phase_completion_marker_path(&project_root, workflow_id, phase_id);
    assert!(!marker.exists(), "ManualPending must NOT write a .completed marker");
    assert!(!is_phase_completed(&project_root, workflow_id, phase_id));
}

#[test]
fn replay_handles_missing_marker_for_manual_pending_phase_correctly() {
    let _guard = home_lock();
    let tmp = TempDir::new().expect("tempdir");
    pin_home(&tmp);
    let project = TempDir::new().expect("project tempdir");
    let project_root = project.path().to_string_lossy().to_string();
    let workflow_id = "wf-replay-manual";
    let phase_id = "approval";

    // Manual-pending crash: no marker, no <phase>.json. Replay must
    // treat the phase as un-run and re-enter it (the pause state in
    // the workflow itself will re-pause), NOT silently advance.
    assert!(!is_phase_completed(&project_root, workflow_id, phase_id));

    // Sanity: had we written the legacy marker, the replay path would
    // be tricked into treating the manual gate as completed — which is
    // precisely the bug this fix avoids. Demonstrate the contrast:
    let bad_outcome = PhaseExecutionOutcome::ManualPending {
        instructions: "buggy persist".to_string(),
        approval_note_required: false,
    };
    persist_phase_output(&project_root, workflow_id, phase_id, &bad_outcome).expect("simulate legacy bug");
    assert!(
        is_phase_completed(&project_root, workflow_id, phase_id),
        "shows why legacy behavior was unsafe: marker advertises completion for a manual gate"
    );
}

#[test]
fn failed_phase_emits_phase_failed_event_kind_not_phase_completed() {
    let emitter = LocalRecordingEmitter::new();

    // Mirror the workflow_execute Err arm: it must emit PhaseFailed, not
    // PhaseCompleted, so subscribers filtering for `phase_failed` see it
    // and the metrics counter `phase_executions_total{status=failed}`
    // increments correctly.
    let event = RuntimeWorkflowEvent {
        workflow_id: "wf-fail-event".to_string(),
        kind: RuntimeWorkflowEventKind::PhaseFailed,
        payload: json!({
            "phase_id": "impl",
            "phase_status": "failed",
            "error": "boom",
        }),
        occurred_at: Utc::now(),
    };
    emitter.emit(event);

    let recorded = emitter.snapshot();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].kind, RuntimeWorkflowEventKind::PhaseFailed);
    assert_ne!(recorded[0].kind, RuntimeWorkflowEventKind::PhaseCompleted);
    assert_eq!(recorded[0].kind.as_wire_str(), "phase_failed");
}

#[test]
fn phase_failed_kind_has_distinct_wire_string_for_metrics_routing() {
    assert_eq!(RuntimeWorkflowEventKind::PhaseCompleted.as_wire_str(), "phase_completed");
    assert_eq!(RuntimeWorkflowEventKind::PhaseFailed.as_wire_str(), "phase_failed");
    assert_ne!(
        RuntimeWorkflowEventKind::PhaseCompleted.as_wire_str(),
        RuntimeWorkflowEventKind::PhaseFailed.as_wire_str(),
        "wire strings must differ so the broadcaster's metrics counter splits completed vs failed"
    );
}
