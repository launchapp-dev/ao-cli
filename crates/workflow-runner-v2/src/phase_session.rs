use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionCheckpointStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCheckpoint {
    pub workflow_id: String,
    pub phase_id: String,
    pub provider: String,
    pub run_id: String,
    // `provider_session_id` is the provider plugin's external session id
    // (what `resume_agent` accepts). It is None until the plugin has
    // reported one back. Pre-v0.4.6 checkpoints incorrectly stored
    // `run_id` in a `session_id` slot; that legacy field is consumed by
    // `legacy_session_id` below and deliberately NOT promoted to
    // `provider_session_id` (the bytes are not a real provider id), so
    // auto-resume safely skips legacy checkpoints rather than dispatching
    // an unknown id to the plugin.
    #[serde(default)]
    pub provider_session_id: Option<String>,
    // Captured purely to drain stale on-disk values written before the
    // v0.4.6 fix split run_id from provider_session_id. Never read by the
    // runtime; never serialized back out.
    #[serde(default, rename = "session_id", skip_serializing)]
    pub legacy_session_id: Option<String>,
    pub status: SessionCheckpointStatus,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<Value>,
}

pub fn phase_session_path(scoped_root: &Path, workflow_id: &str, phase_id: &str) -> PathBuf {
    scoped_root
        .join("runs")
        .join(sanitize(workflow_id))
        .join("phases")
        .join(format!("{}.session.json", sanitize(phase_id)))
}

pub fn write_session_pending(
    scoped_root: &Path,
    workflow_id: &str,
    phase_id: &str,
    provider: &str,
    run_id: &str,
    request: Option<Value>,
) -> io::Result<SessionCheckpoint> {
    let path = phase_session_path(scoped_root, workflow_id, phase_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let checkpoint = SessionCheckpoint {
        workflow_id: workflow_id.to_string(),
        phase_id: phase_id.to_string(),
        provider: provider.to_string(),
        run_id: run_id.to_string(),
        provider_session_id: None,
        legacy_session_id: None,
        status: SessionCheckpointStatus::Pending,
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
        blocked_reason: None,
        request,
    };
    write_atomic(&path, &checkpoint)?;
    Ok(checkpoint)
}

// Marks a checkpoint Running WITHOUT setting provider_session_id. The
// provider's external id arrives asynchronously (e.g. via a sidecar the
// runner persists after the plugin's first response); callers should
// invoke `update_provider_session_id` separately once it is known.
pub fn update_session_running(scoped_root: &Path, workflow_id: &str, phase_id: &str) -> io::Result<()> {
    mutate(scoped_root, workflow_id, phase_id, |checkpoint| {
        checkpoint.status = SessionCheckpointStatus::Running;
    })
}

// Records the provider plugin's external session id (the one resume_agent
// will accept). Called after the plugin reports its session id back to the
// runner — never with the internal run_id.
pub fn update_provider_session_id(
    scoped_root: &Path,
    workflow_id: &str,
    phase_id: &str,
    provider_session_id: &str,
) -> io::Result<()> {
    mutate(scoped_root, workflow_id, phase_id, |checkpoint| {
        if checkpoint.provider_session_id.as_deref() != Some(provider_session_id) {
            checkpoint.provider_session_id = Some(provider_session_id.to_string());
        }
    })
}

pub fn update_session_running_after_resume(
    scoped_root: &Path,
    workflow_id: &str,
    phase_id: &str,
    new_provider_session_id: Option<&str>,
) -> io::Result<()> {
    mutate(scoped_root, workflow_id, phase_id, |checkpoint| {
        if let Some(sid) = new_provider_session_id {
            checkpoint.provider_session_id = Some(sid.to_string());
        }
        checkpoint.status = SessionCheckpointStatus::Running;
        checkpoint.blocked_reason = None;
        checkpoint.started_at = Utc::now().to_rfc3339();
    })
}

pub fn update_session_completed(scoped_root: &Path, workflow_id: &str, phase_id: &str) -> io::Result<()> {
    mutate(scoped_root, workflow_id, phase_id, |checkpoint| {
        checkpoint.status = SessionCheckpointStatus::Completed;
        checkpoint.completed_at = Some(Utc::now().to_rfc3339());
    })
}

pub fn update_session_blocked(scoped_root: &Path, workflow_id: &str, phase_id: &str, reason: &str) -> io::Result<()> {
    mutate(scoped_root, workflow_id, phase_id, |checkpoint| {
        checkpoint.status = SessionCheckpointStatus::Blocked;
        checkpoint.blocked_reason = Some(reason.to_string());
    })
}

// Best-effort lookup of the provider plugin's external session id from the
// runner-sessions sidecar the agent-runner writes when a native session
// backend produces a `Started { session_id }` event. Returns None when the
// sidecar is missing, malformed, or the plugin never reported an id (e.g.
// CLI-only providers without a resumable session).
pub fn lookup_runner_session_sidecar(run_id: &str) -> Option<String> {
    let runs_root = std::env::var_os("ANIMUS_RUNNER_SESSION_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".animus").join("runner-sessions")))?;
    let path = runs_root.join(format!("{}.session.json", sanitize(run_id)));
    let contents = fs::read_to_string(&path).ok()?;
    let parsed: Value = serde_json::from_str(&contents).ok()?;
    let sid = parsed.get("session_id").and_then(Value::as_str)?.trim();
    if sid.is_empty() {
        None
    } else {
        Some(sid.to_string())
    }
}

pub fn read_checkpoint(scoped_root: &Path, workflow_id: &str, phase_id: &str) -> io::Result<Option<SessionCheckpoint>> {
    let path = phase_session_path(scoped_root, workflow_id, phase_id);
    read_path(&path)
}

pub fn read_path(path: &Path) -> io::Result<Option<SessionCheckpoint>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            let checkpoint: SessionCheckpoint = serde_json::from_str(trimmed).map_err(io::Error::other)?;
            Ok(Some(checkpoint))
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn list_running_checkpoints(scoped_root: &Path) -> io::Result<Vec<(PathBuf, SessionCheckpoint)>> {
    let runs_dir = scoped_root.join("runs");
    let mut out = Vec::new();
    let entries = match fs::read_dir(&runs_dir) {
        Ok(e) => e,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    for run_entry in entries {
        let run_entry = run_entry?;
        let phases_dir = run_entry.path().join("phases");
        let phase_entries = match fs::read_dir(&phases_dir) {
            Ok(e) => e,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err),
        };
        for phase_entry in phase_entries {
            let phase_entry = phase_entry?;
            let path = phase_entry.path();
            if !path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".session.json")) {
                continue;
            }
            if let Some(checkpoint) = read_path(&path)? {
                if matches!(checkpoint.status, SessionCheckpointStatus::Running) {
                    out.push((path, checkpoint));
                }
            }
        }
    }
    Ok(out)
}

fn mutate(
    scoped_root: &Path,
    workflow_id: &str,
    phase_id: &str,
    f: impl FnOnce(&mut SessionCheckpoint),
) -> io::Result<()> {
    let path = phase_session_path(scoped_root, workflow_id, phase_id);
    let mut checkpoint = read_path(&path)?.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, format!("no session checkpoint at {}", path.display()))
    })?;
    f(&mut checkpoint);
    write_atomic(&path, &checkpoint)
}

fn write_atomic(path: &Path, checkpoint: &SessionCheckpoint) -> io::Result<()> {
    let payload = serde_json::to_vec_pretty(checkpoint).map_err(io::Error::other)?;
    let tmp = path.with_extension("session.json.tmp");
    fs::write(&tmp, payload)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn sanitize(value: &str) -> String {
    value.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect()
}
