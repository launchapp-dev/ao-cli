use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const ROTATION_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimestampedNotification {
    pub seq: u64,
    pub ts: String,
    pub phase: String,
    pub notification: Value,
}

pub struct NotificationLog {
    path: PathBuf,
    seq: AtomicU64,
    file: Mutex<BufWriter<File>>,
}

impl NotificationLog {
    pub fn open(scoped_root: &Path, workflow_id: &str) -> io::Result<Self> {
        let dir = workflow_run_dir(scoped_root, workflow_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("notifications.jsonl");
        let (starting_seq, valid_bytes) = scan_max_seq_and_valid_bytes(&path)?;
        if let Ok(metadata) = std::fs::metadata(&path) {
            if metadata.len() > valid_bytes {
                let truncate = OpenOptions::new().write(true).open(&path)?;
                truncate.set_len(valid_bytes)?;
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self { path, seq: AtomicU64::new(starting_seq), file: Mutex::new(BufWriter::new(file)) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, phase: &str, notification: &Value) -> io::Result<()> {
        let next = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let record = TimestampedNotification {
            seq: next,
            ts: Utc::now().to_rfc3339(),
            phase: phase.to_string(),
            notification: notification.clone(),
        };
        let line = serde_json::to_string(&record).map_err(io::Error::other)?;
        let mut guard = self.file.lock().map_err(|_| io::Error::other("notification log mutex poisoned"))?;
        guard.write_all(line.as_bytes())?;
        guard.write_all(b"\n")?;
        guard.flush()?;
        Ok(())
    }

    pub fn rotate_if_needed(&self) -> io::Result<()> {
        let metadata = match std::fs::metadata(&self.path) {
            Ok(m) => m,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };
        if metadata.len() < ROTATION_THRESHOLD_BYTES {
            return Ok(());
        }
        let mut guard = self.file.lock().map_err(|_| io::Error::other("notification log mutex poisoned"))?;
        guard.flush()?;
        drop(guard);

        let parent = self.path.parent().ok_or_else(|| io::Error::other("notification log path has no parent"))?;
        let next_rotation = next_rotation_index(parent)?;
        let rotated = parent.join(format!("notifications.{next_rotation}.jsonl"));
        std::fs::rename(&self.path, &rotated)?;
        let file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        let mut guard = self.file.lock().map_err(|_| io::Error::other("notification log mutex poisoned"))?;
        *guard = BufWriter::new(file);
        Ok(())
    }

    pub fn tail(scoped_root: &Path, workflow_id: &str, from_seq: u64) -> io::Result<Vec<TimestampedNotification>> {
        let path = workflow_run_dir(scoped_root, workflow_id).join("notifications.jsonl");
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let reader = BufReader::new(file);
        let mut out = Vec::new();
        let mut buf = String::new();
        let mut reader = reader;
        loop {
            buf.clear();
            let read = reader.read_line(&mut buf)?;
            if read == 0 {
                break;
            }
            if !buf.ends_with('\n') {
                // Drop incomplete trailing line: crash mid-write left a partial record on disk,
                // and silently skipping it is the only way to keep replay idempotent without
                // surfacing a corruption alarm to every reader.
                break;
            }
            let trimmed = buf.trim_end_matches('\n').trim_end_matches('\r');
            if trimmed.is_empty() {
                continue;
            }
            let record: TimestampedNotification = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if record.seq > from_seq {
                out.push(record);
            }
        }
        Ok(out)
    }
}

pub fn workflow_run_dir(scoped_root: &Path, workflow_id: &str) -> PathBuf {
    scoped_root.join("runs").join(sanitize_id(workflow_id))
}

fn sanitize_id(value: &str) -> String {
    value.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect()
}

fn next_rotation_index(parent: &Path) -> io::Result<u64> {
    let mut max_idx = 0u64;
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(rest) = name.strip_prefix("notifications.") {
            if let Some(idx_str) = rest.strip_suffix(".jsonl") {
                if let Ok(idx) = idx_str.parse::<u64>() {
                    if idx > max_idx {
                        max_idx = idx;
                    }
                }
            }
        }
    }
    Ok(max_idx + 1)
}

fn scan_max_seq_and_valid_bytes(path: &Path) -> io::Result<(u64, u64)> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(err) => return Err(err),
    };
    let mut reader = BufReader::new(file);
    let mut max_seq = 0u64;
    let mut valid_bytes: u64 = 0;
    let mut buf = String::new();
    loop {
        buf.clear();
        let read = reader.read_line(&mut buf)?;
        if read == 0 {
            break;
        }
        if !buf.ends_with('\n') {
            break;
        }
        let trimmed = buf.trim_end_matches('\n').trim_end_matches('\r');
        valid_bytes = valid_bytes.saturating_add(read as u64);
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(record) = serde_json::from_str::<TimestampedNotification>(trimmed) {
            if record.seq > max_seq {
                max_seq = record.seq;
            }
        }
    }
    Ok((max_seq, valid_bytes))
}
