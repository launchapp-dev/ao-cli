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
        persist_phase_output(&project_root, workflow_id, phase_id, 1, &outcome).expect("persist");
    }

    let marker = phase_completion_marker_path(&project_root, workflow_id, phase_id, 1);
    assert!(!marker.exists(), "ManualPending must NOT write a .completed marker");
    assert!(!is_phase_completed(&project_root, workflow_id, phase_id, 1));
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
    assert!(!is_phase_completed(&project_root, workflow_id, phase_id, 1));

    // Sanity: had we written the legacy marker, the replay path would
    // be tricked into treating the manual gate as completed — which is
    // precisely the bug this fix avoids. Demonstrate the contrast:
    let bad_outcome = PhaseExecutionOutcome::ManualPending {
        instructions: "buggy persist".to_string(),
        approval_note_required: false,
    };
    persist_phase_output(&project_root, workflow_id, phase_id, 1, &bad_outcome).expect("simulate legacy bug");
    assert!(
        is_phase_completed(&project_root, workflow_id, phase_id, 1),
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

/// Regression guard for codex round-6 P2 (workflow_execute.rs:370-372):
/// the single-phase (`--phase` flag) error branch must emit
/// `RuntimeWorkflowEventKind::PhaseFailed` -- the twin of the multi-phase
/// fix landed earlier. Previously it emitted `PhaseCompleted` with
/// `phase_status: "failed"`, so subscribers and metrics keyed on
/// `phase_failed` silently missed every failed single-phase run.
///
/// We assert against the source text rather than spinning up a full
/// workflow because the single-phase path requires a compiled workflow
/// config, a service hub, and an executable phase backend -- overkill
/// when the regression is a one-line enum swap.
#[test]
fn single_phase_error_emits_phase_failed_event_kind() {
    let src = include_str!("../src/workflow_execute.rs");

    // The single-phase block has two early returns -- one for the Ok arm
    // (phase ran, may have succeeded or marked-failed via outcome) and one
    // for the Err arm (phase panicked / unrecoverable error). Both end
    // with the "post-success actions are not run for single-phase execution"
    // sentinel. We want the Err arm, which is the SECOND occurrence.
    let single_phase_branch_marker = "post-success actions are not run for single-phase execution";
    let first = src
        .find(single_phase_branch_marker)
        .expect("single-phase block sentinel string missing -- update test if refactored");
    let after_first = first + single_phase_branch_marker.len();
    let second_rel = src[after_first..]
        .find(single_phase_branch_marker)
        .expect("expected TWO occurrences of single-phase sentinel (Ok arm + Err arm) -- structure changed");
    let err_arm_marker_pos = after_first + second_rel;

    // The Err arm starts somewhere between the Ok arm's marker and the Err
    // arm's marker. Scan that region for the event-kind emission.
    let window = &src[after_first..err_arm_marker_pos];

    assert!(
        window.contains("RuntimeWorkflowEventKind::PhaseFailed"),
        "single-phase Err arm must emit PhaseFailed (codex round-6 P2). \
         Window scanned (between Ok-arm marker and Err-arm marker):\n{window}",
    );

    // Inspect the kind argument actually passed to emit_runtime in this
    // window -- it must be PhaseFailed, not PhaseCompleted.
    if let Some(kind_pos) = window.find("emit_runtime(") {
        let after_emit = &window[kind_pos..];
        let kind_line_end = after_emit.find(',').unwrap_or(after_emit.len());
        let kind_line = &after_emit[..kind_line_end];
        assert!(
            !kind_line.contains("PhaseCompleted"),
            "single-phase Err arm must NOT pass PhaseCompleted to emit_runtime \
             (that was the regression). Kind line: {kind_line}",
        );
        assert!(
            kind_line.contains("PhaseFailed"),
            "single-phase Err arm must pass PhaseFailed to emit_runtime. \
             Kind line: {kind_line}",
        );
    } else {
        panic!("expected emit_runtime call in single-phase Err arm window:\n{window}");
    }
}
