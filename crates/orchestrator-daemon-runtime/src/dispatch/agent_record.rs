//! v0.5 P2 Wave C: agent-reattach scaffolding. Full reattach is deferred
//! to v0.6 — today the daemon does NOT attempt to reattach to or interfere
//! with detected orphan agents; operators decide whether to kill them.
//! This module only writes a per-spawn record so a future startup-scan
//! can use it.

use std::path::PathBuf;

use protocol::SubjectDispatch;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpawnRecord {
    pub agent_session_id: String,
    pub pid: u32,
    pub started_at: String,
    pub subject_id: String,
    pub subject_kind: String,
    pub workflow_ref: String,
    pub task_id: Option<String>,
    pub command_line: Vec<String>,
    pub stdio_socket_path: Option<String>,
}

fn agents_dir(project_root: &std::path::Path) -> Option<PathBuf> {
    let scope = protocol::scoped_state_root(project_root)?;
    let dir = scope.join("runs").join("_pending").join("agents");
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Some(dir)
}

pub fn record_path(project_root: &std::path::Path, agent_session_id: &str) -> Option<PathBuf> {
    let dir = agents_dir(project_root)?;
    Some(dir.join(format!("{agent_session_id}.json")))
}

pub fn write_record(project_root: &std::path::Path, record: &AgentSpawnRecord) -> std::io::Result<()> {
    let Some(path) = record_path(project_root, &record.agent_session_id) else {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "scoped runtime root unavailable"));
    };
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, json)?;
    if let Ok(file) = std::fs::File::open(&tmp) {
        let _ = file.sync_all();
    }
    std::fs::rename(&tmp, &path)
}

pub fn delete_record(project_root: &std::path::Path, agent_session_id: &str) {
    if let Some(path) = record_path(project_root, agent_session_id) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn build_record(
    agent_session_id: String,
    pid: u32,
    dispatch: &SubjectDispatch,
    command_line: Vec<String>,
    stdio_socket_path: Option<String>,
) -> AgentSpawnRecord {
    let started_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    AgentSpawnRecord {
        agent_session_id,
        pid,
        started_at,
        subject_id: dispatch.subject_id().to_string(),
        subject_kind: dispatch.subject_kind().to_string(),
        workflow_ref: dispatch.workflow_ref.clone(),
        task_id: dispatch.task_id().map(String::from),
        command_line,
        stdio_socket_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::SubjectDispatchExt;

    fn write_record_into(dir: &std::path::Path, record: &AgentSpawnRecord) -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.json", record.agent_session_id));
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }

    #[test]
    fn record_roundtrips_via_json_in_isolated_dir() {
        let temp = tempfile::tempdir().unwrap();
        let dispatch = protocol::SubjectDispatch::for_task("TASK-1", "standard");
        let record = build_record("agent-xyz".to_string(), 4242, &dispatch, vec!["/bin/echo".into()], None);

        let path = write_record_into(temp.path(), &record).expect("write");
        let raw = std::fs::read(&path).unwrap();
        let back: AgentSpawnRecord = serde_json::from_slice(&raw).unwrap();
        assert_eq!(back.agent_session_id, "agent-xyz");
        assert_eq!(back.pid, 4242);
        assert_eq!(back.subject_id, dispatch.subject_id());
        assert_eq!(back.workflow_ref, "standard");
        assert_eq!(back.command_line, vec!["/bin/echo".to_string()]);

        std::fs::remove_file(&path).unwrap();
        assert!(!path.exists());
    }
}
