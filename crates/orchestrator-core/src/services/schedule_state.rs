use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const SCHEDULE_STATE_FILE_NAME: &str = "schedule-state.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduleState {
    #[serde(default)]
    pub schedules: HashMap<String, ScheduleRunState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduleRunState {
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_status: String,
    #[serde(default)]
    pub run_count: u64,
    /// If set, the schedule is paused until this timestamp (e.g., due to a rate-limit window).
    /// The scheduler will skip this schedule until the reset time passes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_until: Option<DateTime<Utc>>,
}

fn schedule_state_path(project_root: &Path) -> PathBuf {
    let scoped_root = protocol::scoped_state_root(project_root).unwrap_or_else(|| project_root.join(".ao"));
    scoped_root.join("state").join(SCHEDULE_STATE_FILE_NAME)
}

pub fn load_schedule_state(project_root: &Path) -> Result<ScheduleState> {
    let path = schedule_state_path(project_root);
    if !path.exists() {
        return Ok(ScheduleState::default());
    }

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read schedule state from {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse schedule state JSON from {}", path.display()))
}

pub fn save_schedule_state(project_root: &Path, state: &ScheduleState) -> Result<()> {
    let path = schedule_state_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create schedule state directory {}", parent.display()))?;
    }
    let payload = serde_json::to_string_pretty(state)?;
    std::fs::write(&path, payload).with_context(|| format!("failed to write schedule state to {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_schedule_state_returns_default() {
        let temp = tempdir().expect("tempdir");
        let loaded = load_schedule_state(temp.path()).expect("load default state");
        assert!(loaded.schedules.is_empty());
    }

    #[test]
    fn save_and_load_schedule_state_round_trip() {
        let temp = tempdir().expect("tempdir");
        let mut original = ScheduleState::default();
        original.schedules.insert(
            "nightly".to_string(),
            ScheduleRunState {
                last_run: Some(Utc::now()),
                last_status: "completed".to_string(),
                run_count: 3,
                paused_until: None,
            },
        );

        save_schedule_state(temp.path(), &original).expect("save state");
        let loaded = load_schedule_state(temp.path()).expect("load state");

        assert_eq!(loaded.schedules.len(), 1);
        let run_state = loaded.schedules.get("nightly").expect("run state should exist");
        assert_eq!(run_state.last_status, "completed");
        assert_eq!(run_state.run_count, 3);
        assert!(run_state.last_run.is_some());
        assert!(run_state.paused_until.is_none());
    }

    #[test]
    fn schedule_state_persists_paused_until() {
        let temp = tempdir().expect("tempdir");
        let mut original = ScheduleState::default();
        let paused_time = Utc::now() + chrono::Duration::hours(2);
        original.schedules.insert(
            "pr-reviewer".to_string(),
            ScheduleRunState {
                last_run: Some(Utc::now()),
                last_status: "failed: rate limit".to_string(),
                run_count: 100,
                paused_until: Some(paused_time),
            },
        );

        save_schedule_state(temp.path(), &original).expect("save state");
        let loaded = load_schedule_state(temp.path()).expect("load state");

        let run_state = loaded.schedules.get("pr-reviewer").expect("run state should exist");
        assert!(run_state.paused_until.is_some());
        assert_eq!(run_state.paused_until.unwrap(), paused_time);
    }

    #[test]
    fn schedule_state_backward_compatible_without_paused_until() {
        // Test that we can load old schedule state files that don't have paused_until
        // The key is that load_schedule_state uses the project root to derive the state path.
        // For temp directories, scoped_state_root may return a path under ~/.ao/
        // We need to write to the actual location where load_schedule_state looks.
        let temp = tempdir().expect("tempdir");

        // First, save a state using the normal flow to discover where it goes
        let original = ScheduleState {
            schedules: std::collections::HashMap::new(),
        };
        save_schedule_state(temp.path(), &original).expect("save initial state");

        // Now read the actual path that was used
        let state_path = {
            let scoped = protocol::scoped_state_root(temp.path());
            let base = scoped.unwrap_or_else(|| temp.path().join(".ao"));
            base.join("state").join(SCHEDULE_STATE_FILE_NAME)
        };

        // Overwrite with old format (no paused_until field)
        let old_json = r#"{"schedules":{"nightly":{"last_run":"2026-03-04T12:00:00Z","last_status":"completed","run_count":5}}}"#;
        std::fs::write(&state_path, old_json).expect("write old state");

        // Now load should successfully parse the old format with paused_until defaulting to None
        let loaded = load_schedule_state(temp.path()).expect("load old state");
        assert_eq!(loaded.schedules.len(), 1);
        let run_state = loaded.schedules.get("nightly").expect("run state should exist");
        assert_eq!(run_state.last_status, "completed");
        assert_eq!(run_state.run_count, 5);
        assert!(run_state.paused_until.is_none()); // Should default to None
    }
}
