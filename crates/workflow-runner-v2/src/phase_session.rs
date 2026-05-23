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
    pub session_id: Option<String>,
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
        session_id: None,
        status: SessionCheckpointStatus::Pending,
        started_at: Utc::now().to_rfc3339(),
        completed_at: None,
        blocked_reason: None,
        request,
    };
    write_atomic(&path, &checkpoint)?;
    Ok(checkpoint)
}

pub fn update_session_running(
    scoped_root: &Path,
    workflow_id: &str,
    phase_id: &str,
    session_id: &str,
) -> io::Result<()> {
    mutate(scoped_root, workflow_id, phase_id, |checkpoint| {
        if checkpoint.session_id.as_deref() != Some(session_id) {
            checkpoint.session_id = Some(session_id.to_string());
        }
        checkpoint.status = SessionCheckpointStatus::Running;
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
