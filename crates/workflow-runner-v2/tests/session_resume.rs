use serde_json::json;
use tempfile::TempDir;
use workflow_runner_v2::phase_session::{
    list_running_checkpoints, phase_session_path, read_checkpoint, update_session_blocked, update_session_completed,
    update_session_running, write_session_pending, SessionCheckpointStatus,
};

#[test]
fn session_checkpoint_writes_pending_running_completed_in_order() {
    let temp = TempDir::new().expect("tempdir");
    let scoped = temp.path();
    let workflow_id = "wf-checkpoint";
    let phase_id = "impl";
    let provider = "claude";
    let run_id = "run-abc";

    let initial = write_session_pending(
        scoped,
        workflow_id,
        phase_id,
        provider,
        run_id,
        Some(json!({"model": "claude-sonnet-4-6"})),
    )
    .expect("pending");
    assert_eq!(initial.status, SessionCheckpointStatus::Pending);
    assert_eq!(initial.provider, provider);
    assert_eq!(initial.run_id, run_id);
    assert!(initial.completed_at.is_none());

    let path = phase_session_path(scoped, workflow_id, phase_id);
    assert!(path.exists(), "checkpoint file should exist");

    let session_id = "sess-xyz";
    update_session_running(scoped, workflow_id, phase_id, session_id).expect("running");
    let running = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert_eq!(running.status, SessionCheckpointStatus::Running);
    assert_eq!(running.session_id.as_deref(), Some(session_id));
    assert!(running.completed_at.is_none());

    update_session_completed(scoped, workflow_id, phase_id).expect("completed");
    let completed = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert_eq!(completed.status, SessionCheckpointStatus::Completed);
    assert!(completed.completed_at.is_some());
    assert_eq!(completed.session_id.as_deref(), Some(session_id));
}

#[tokio::test]
async fn recover_attempts_resume_for_running_phases_then_marks_blocked_on_failure() {
    let temp = TempDir::new().expect("tempdir");
    let scoped = temp.path();
    let workflow_id = "wf-recovery";
    let phase_id = "review";

    write_session_pending(scoped, workflow_id, phase_id, "codex", "run-recover", None).expect("pending");
    update_session_running(scoped, workflow_id, phase_id, "sess-stale").expect("running");

    write_session_pending(scoped, "wf-completed", "design", "claude", "run-done", None).expect("pending other");
    update_session_running(scoped, "wf-completed", "design", "sess-other").expect("running other");
    update_session_completed(scoped, "wf-completed", "design").expect("completed other");

    let pending = list_running_checkpoints(scoped).expect("list running");
    assert_eq!(pending.len(), 1, "only the truly Running checkpoint should be returned");
    assert_eq!(pending[0].1.phase_id, phase_id);
    assert_eq!(pending[0].1.session_id.as_deref(), Some("sess-stale"));

    let mut recovered = 0usize;
    for (_path, checkpoint) in &pending {
        let resume_result: Result<(), String> = Err("provider session expired".to_string());
        match resume_result {
            Ok(()) => recovered += 1,
            Err(err) => {
                update_session_blocked(
                    scoped,
                    &checkpoint.workflow_id,
                    &checkpoint.phase_id,
                    &format!("session resume failed: {err}"),
                )
                .expect("block");
            }
        }
    }
    assert_eq!(recovered, 0, "mock provider always fails resume");

    let after = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert_eq!(after.status, SessionCheckpointStatus::Blocked);
    assert!(after.blocked_reason.as_deref().unwrap_or("").contains("session resume failed"));

    let still_running = list_running_checkpoints(scoped).expect("list after");
    assert!(still_running.is_empty(), "blocked checkpoints should no longer be Running");
}
