use crate::cli_types::SessionCommand;
use crate::print_value;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionTokenUsage {
    total_input: u64,
    total_output: u64,
    total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionMetadata {
    created_at: u64,
    last_updated: u64,
    turn_count: u32,
    token_usage: SessionTokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionFile {
    metadata: SessionMetadata,
    messages: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
struct SessionListEntry {
    session_id: String,
    created_at: u64,
    last_updated: u64,
    turn_count: u32,
    token_usage: SessionTokenUsage,
    message_count: usize,
}

fn oai_sessions_dir() -> PathBuf {
    let base = std::env::var("AO_CONFIG_DIR")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.ao", h)))
        .unwrap_or_else(|_| ".ao".to_string());
    PathBuf::from(base).join("sessions")
}

pub(crate) async fn handle_session(command: SessionCommand, json: bool) -> Result<()> {
    match command {
        SessionCommand::List => {
            let sessions_dir = oai_sessions_dir();
            let mut entries: Vec<SessionListEntry> = Vec::new();

            if sessions_dir.exists() {
                let dir_entries = std::fs::read_dir(&sessions_dir)?;
                for entry in dir_entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let session_id = match path.file_stem().and_then(|s| s.to_str()) {
                        Some(id) => id.to_string(),
                        None => continue,
                    };
                    let data = match std::fs::read_to_string(&path) {
                        Ok(d) => d,
                        Err(_) => continue,
                    };
                    let mtime = path
                        .metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    // Try new format with metadata
                    if let Ok(sf) = serde_json::from_str::<SessionFile>(&data) {
                        let message_count = sf.messages.len();
                        entries.push(SessionListEntry {
                            session_id,
                            created_at: sf.metadata.created_at,
                            last_updated: sf.metadata.last_updated,
                            turn_count: sf.metadata.turn_count,
                            token_usage: sf.metadata.token_usage,
                            message_count,
                        });
                        continue;
                    }
                    // Fall back to legacy Vec<Value> format
                    if let Ok(msgs) = serde_json::from_str::<Vec<serde_json::Value>>(&data) {
                        entries.push(SessionListEntry {
                            session_id,
                            created_at: mtime,
                            last_updated: mtime,
                            turn_count: 0,
                            token_usage: SessionTokenUsage::default(),
                            message_count: msgs.len(),
                        });
                        continue;
                    }
                    // Corrupted — skip
                }
            }

            entries.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
            let count = entries.len();
            print_value(
                serde_json::json!({
                    "sessions": entries,
                    "count": count,
                    "sessions_dir": sessions_dir.display().to_string(),
                }),
                json,
            )
        }
    }
}
