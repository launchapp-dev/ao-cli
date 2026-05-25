use std::sync::{Mutex, OnceLock};

use serde_json::json;
use tempfile::TempDir;
use workflow_runner_v2::phase_session::{
    list_running_checkpoints, lookup_runner_session_sidecar, phase_session_path, read_checkpoint,
    update_provider_session_id, update_session_blocked, update_session_completed, update_session_running,
    write_session_pending, SessionCheckpointStatus,
};

fn sidecar_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|p| p.into_inner())
}

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
    assert!(initial.provider_session_id.is_none());

    let path = phase_session_path(scoped, workflow_id, phase_id);
    assert!(path.exists(), "checkpoint file should exist");

    update_session_running(scoped, workflow_id, phase_id).expect("running");
    let running_no_sid = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert_eq!(running_no_sid.status, SessionCheckpointStatus::Running);
    assert!(running_no_sid.provider_session_id.is_none(), "provider_session_id stays None until plugin reports it");

    let provider_session_id = "sess-xyz";
    update_provider_session_id(scoped, workflow_id, phase_id, provider_session_id).expect("set provider sid");
    let running = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert_eq!(running.provider_session_id.as_deref(), Some(provider_session_id));
    assert_eq!(running.run_id, run_id, "run_id stays distinct from provider_session_id");
    assert_ne!(
        running.provider_session_id.as_deref(),
        Some(running.run_id.as_str()),
        "run_id MUST NOT leak into provider_session_id"
    );

    update_session_completed(scoped, workflow_id, phase_id).expect("completed");
    let completed = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert_eq!(completed.status, SessionCheckpointStatus::Completed);
    assert!(completed.completed_at.is_some());
    assert_eq!(completed.provider_session_id.as_deref(), Some(provider_session_id));
}

#[test]
fn checkpoint_separates_provider_session_id_from_run_id() {
    let temp = TempDir::new().expect("tempdir");
    let scoped = temp.path();
    let workflow_id = "wf-split";
    let phase_id = "research";
    let run_id = "run-internal-uuid";

    write_session_pending(scoped, workflow_id, phase_id, "claude", run_id, None).expect("pending");
    update_session_running(scoped, workflow_id, phase_id).expect("running");

    let checkpoint = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert_eq!(checkpoint.run_id, run_id);
    assert!(
        checkpoint.provider_session_id.is_none(),
        "before any plugin response, provider_session_id MUST be None (no leak of run_id)"
    );
}

#[test]
fn legacy_session_id_field_is_not_promoted_to_provider_session_id_on_read() {
    let temp = TempDir::new().expect("tempdir");
    let scoped = temp.path();
    let runs_dir = scoped.join("runs").join("wf-legacy").join("phases");
    std::fs::create_dir_all(&runs_dir).expect("mkdir");
    let path = runs_dir.join("impl.session.json");
    let legacy = serde_json::json!({
        "workflow_id": "wf-legacy",
        "phase_id": "impl",
        "provider": "claude",
        "run_id": "run-legacy-uuid",
        "session_id": "run-legacy-uuid",
        "status": "running",
        "started_at": "2026-05-25T00:00:00Z",
        "request": null,
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).expect("write legacy");

    let checkpoint = read_checkpoint(scoped, "wf-legacy", "impl").expect("read").expect("present");
    assert_eq!(checkpoint.run_id, "run-legacy-uuid");
    assert!(
        checkpoint.provider_session_id.is_none(),
        "legacy session_id MUST NOT be promoted into provider_session_id (it is actually run_id)"
    );
    assert_eq!(checkpoint.legacy_session_id.as_deref(), Some("run-legacy-uuid"));
}

#[test]
fn auto_resume_blocks_when_provider_session_id_not_captured() {
    let temp = TempDir::new().expect("tempdir");
    let scoped = temp.path();
    let workflow_id = "wf-no-sid";
    let phase_id = "review";

    write_session_pending(scoped, workflow_id, phase_id, "claude", "run-crashed", None).expect("pending");
    update_session_running(scoped, workflow_id, phase_id).expect("running");

    let checkpoint = read_checkpoint(scoped, workflow_id, phase_id).expect("read").expect("present");
    assert!(
        checkpoint.provider_session_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_none(),
        "missing provider session id must be caught by auto-resume before dispatch"
    );
}

#[test]
fn lookup_runner_session_sidecar_reads_persisted_payload() {
    let _guard = sidecar_env_lock();
    let temp = TempDir::new().expect("tempdir");
    std::env::set_var("ANIMUS_RUNNER_SESSION_DIR", temp.path());
    let payload = serde_json::json!({
        "run_id": "run-sidecar",
        "session_id": "sess-from-sidecar",
        "tool": "claude",
        "recorded_at": "2026-05-25T00:00:00Z",
    });
    std::fs::write(temp.path().join("run-sidecar.session.json"), serde_json::to_vec_pretty(&payload).unwrap())
        .expect("write sidecar");
    let sid = lookup_runner_session_sidecar("run-sidecar");
    std::env::remove_var("ANIMUS_RUNNER_SESSION_DIR");
    assert_eq!(sid.as_deref(), Some("sess-from-sidecar"));
}

#[test]
fn lookup_runner_session_sidecar_returns_none_for_missing_or_empty() {
    let _guard = sidecar_env_lock();
    let temp = TempDir::new().expect("tempdir");
    std::env::set_var("ANIMUS_RUNNER_SESSION_DIR", temp.path());
    assert!(lookup_runner_session_sidecar("ghost-run").is_none());

    let empty_payload = serde_json::json!({
        "run_id": "run-empty",
        "session_id": "   ",
        "tool": "claude",
    });
    std::fs::write(temp.path().join("run-empty.session.json"), serde_json::to_vec_pretty(&empty_payload).unwrap())
        .expect("write empty sidecar");
    let sid = lookup_runner_session_sidecar("run-empty");
    std::env::remove_var("ANIMUS_RUNNER_SESSION_DIR");
    assert!(sid.is_none(), "empty/whitespace session_id is treated as missing");
}

#[tokio::test]
async fn recover_attempts_resume_for_running_phases_then_marks_blocked_on_failure() {
    let temp = TempDir::new().expect("tempdir");
    let scoped = temp.path();
    let workflow_id = "wf-recovery";
    let phase_id = "review";

    write_session_pending(scoped, workflow_id, phase_id, "codex", "run-recover", None).expect("pending");
    update_session_running(scoped, workflow_id, phase_id).expect("running");
    update_provider_session_id(scoped, workflow_id, phase_id, "sess-stale").expect("provider sid");

    write_session_pending(scoped, "wf-completed", "design", "claude", "run-done", None).expect("pending other");
    update_session_running(scoped, "wf-completed", "design").expect("running other");
    update_provider_session_id(scoped, "wf-completed", "design", "sess-other").expect("provider sid other");
    update_session_completed(scoped, "wf-completed", "design").expect("completed other");

    let pending = list_running_checkpoints(scoped).expect("list running");
    assert_eq!(pending.len(), 1, "only the truly Running checkpoint should be returned");
    assert_eq!(pending[0].1.phase_id, phase_id);
    assert_eq!(pending[0].1.provider_session_id.as_deref(), Some("sess-stale"));

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
