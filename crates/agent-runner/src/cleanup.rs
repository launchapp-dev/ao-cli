use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use tracing::{debug, info, warn};

pub use protocol::{process_exists, kill_process};

#[cfg(windows)]
pub use protocol::{track_job, untrack_job};

pub fn cleanup_orphaned_clis() -> Result<()> {
    let tracker_path = protocol::cli_tracker_path();

    if !tracker_path.exists() {
        debug!(path = %tracker_path.display(), "No orphan tracker file found");
        return Ok(());
    }

    let content = fs::read_to_string(&tracker_path)?;
    let tracked: HashMap<String, u32> = serde_json::from_str(&content).unwrap_or_default();
    info!(
        tracked_count = tracked.len(),
        tracker_path = %tracker_path.display(),
        "Loaded tracked CLI processes for orphan cleanup"
    );

    let mut cleaned = 0;
    for (run_id, pid) in tracked {
        if !process_exists(pid as i32) {
            info!(run_id, pid, "Tracked process is already terminated");
            continue;
        }

        info!(run_id, pid, "Killing orphaned tracked process");
        if kill_process(pid as i32) {
            cleaned += 1;
        } else {
            warn!(run_id, pid, "Failed to kill orphaned process");
        }
    }

    fs::remove_file(&tracker_path)?;
    info!(
        cleaned_count = cleaned,
        tracker_path = %tracker_path.display(),
        "Finished orphaned process cleanup"
    );
    Ok(())
}

pub fn track_process(run_id: &str, pid: u32) -> Result<()> {
    let tracker_path = protocol::cli_tracker_path();
    let mut tracked: HashMap<String, u32> = if tracker_path.exists() {
        let content = fs::read_to_string(&tracker_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };

    tracked.insert(run_id.to_string(), pid);
    fs::write(&tracker_path, serde_json::to_string(&tracked)?)?;
    debug!(
        run_id,
        pid,
        tracked_count = tracked.len(),
        tracker_path = %tracker_path.display(),
        "Tracked CLI process"
    );
    Ok(())
}

pub fn untrack_process(run_id: &str) -> Result<()> {
    let tracker_path = protocol::cli_tracker_path();
    if !tracker_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&tracker_path)?;
    let mut tracked: HashMap<String, u32> = serde_json::from_str(&content).unwrap_or_default();
    let removed = tracked.remove(run_id).is_some();
    fs::write(&tracker_path, serde_json::to_string(&tracked)?)?;
    debug!(
        run_id,
        removed,
        remaining = tracked.len(),
        tracker_path = %tracker_path.display(),
        "Untracked CLI process"
    );
    Ok(())
}
