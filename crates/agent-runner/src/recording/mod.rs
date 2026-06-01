//! LLM-decision recording + replay (v0.5.1 scoped foundation).
//!
//! Append-only JSONL log of every agent decision-relevant event that flows
//! through the agent-runner. Recording is always-on. Replay is opt-in via
//! `ANIMUS_REPLAY_SESSION=<path>` or `context.replay_session_path`.
//!
//! Recording layout: `~/.animus/<repo-scope>/runs/<run_id>/decisions.jsonl`.
//!
//! v0.6 production-grade gaps (NOT yet solved):
//! 1. Models are not deterministic — replay only works against recorded logs,
//!    not against a fresh model call.
//! 2. Cross-provider shape normalization is NOT solved; logs are provider-specific.
//! 3. No durable_store idempotency-key fence integration.
//! 4. No reattach integration (orphan scan does not consult decision log).
//! 5. Tool side-effects outside the recording boundary are not modeled; replay
//!    yields recorded results without re-executing tools.
//! 6. No chunk-level checksum; truncated tails tolerated, corrupted middle aborts.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionEvent {
    Prompt { timestamp_ms: u64, model_id: String, prompt: String, runtime_contract: Option<Value> },
    ResponseChunk { timestamp_ms: u64, stream: String, text: String },
    ToolCall { timestamp_ms: u64, name: String, args: Value },
    ToolResult { timestamp_ms: u64, name: String, result: Value },
    Metadata { timestamp_ms: u64, payload: Value },
    Finished { timestamp_ms: u64, exit_code: Option<i32> },
}

impl DecisionEvent {
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default()
    }
    pub fn prompt(model_id: impl Into<String>, prompt: impl Into<String>, runtime_contract: Option<Value>) -> Self {
        Self::Prompt { timestamp_ms: Self::now_ms(), model_id: model_id.into(), prompt: prompt.into(), runtime_contract }
    }
    pub fn response_chunk(stream: impl Into<String>, text: impl Into<String>) -> Self {
        Self::ResponseChunk { timestamp_ms: Self::now_ms(), stream: stream.into(), text: text.into() }
    }
    pub fn tool_call(name: impl Into<String>, args: Value) -> Self {
        Self::ToolCall { timestamp_ms: Self::now_ms(), name: name.into(), args }
    }
    pub fn tool_result(name: impl Into<String>, result: Value) -> Self {
        Self::ToolResult { timestamp_ms: Self::now_ms(), name: name.into(), result }
    }
    pub fn metadata(payload: Value) -> Self {
        Self::Metadata { timestamp_ms: Self::now_ms(), payload }
    }
    pub fn finished(exit_code: Option<i32>) -> Self {
        Self::Finished { timestamp_ms: Self::now_ms(), exit_code }
    }
}

pub struct Recorder {
    inner: Mutex<BufWriter<File>>,
    path: PathBuf,
}

impl Recorder {
    pub fn create_at(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create decision-log parent dir {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("open decision log {}", path.display()))?;
        Ok(Self { inner: Mutex::new(BufWriter::new(file)), path })
    }
    pub fn for_run(project_root: &str, run_id: &str) -> Result<Option<Self>> {
        let Some(path) = decision_log_path(project_root, run_id) else { return Ok(None); };
        Ok(Some(Self::create_at(path)?))
    }
    pub fn path(&self) -> &Path { &self.path }
    pub fn record(&self, event: &DecisionEvent) -> Result<()> {
        let line = serde_json::to_string(event).context("serialize decision event")?;
        let mut guard = self.inner.lock().expect("recorder mutex poisoned");
        guard.write_all(line.as_bytes()).context("write decision event")?;
        guard.write_all(b"\n").context("write decision newline")?;
        guard.flush().context("flush decision event")?;
        Ok(())
    }
}

pub fn decision_log_path(project_root: &str, run_id: &str) -> Option<PathBuf> {
    if run_id.trim().is_empty() { return None; }
    if run_id.contains('/') || run_id.contains('\\') || run_id.contains("..") { return None; }
    let home = dirs::home_dir()?;
    Some(home.join(".animus").join(protocol::repository_scope_for_path(Path::new(project_root))).join("runs").join(run_id).join("decisions.jsonl"))
}

pub struct ReplaySource {
    events: std::vec::IntoIter<DecisionEvent>,
    truncated_tail: bool,
}

impl ReplaySource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).with_context(|| format!("open replay session {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        let mut truncated_tail = false;
        let lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>().context("read replay session")?;
        let total = lines.len();
        for (idx, line) in lines.into_iter().enumerate() {
            if line.trim().is_empty() { continue; }
            match serde_json::from_str::<DecisionEvent>(&line) {
                Ok(event) => events.push(event),
                Err(err) => {
                    if idx + 1 == total {
                        truncated_tail = true;
                    } else {
                        anyhow::bail!("corrupted decision-log entry at line {} of {}: {}", idx + 1, path.display(), err);
                    }
                }
            }
        }
        Ok(Self { events: events.into_iter(), truncated_tail })
    }
    pub fn truncated_tail(&self) -> bool { self.truncated_tail }
    pub fn next_event(&mut self) -> Option<DecisionEvent> { self.events.next() }
    pub fn drain(mut self) -> Vec<DecisionEvent> {
        let mut out = Vec::new();
        while let Some(event) = self.next_event() { out.push(event); }
        out
    }
}

pub fn env_replay_source() -> Result<Option<ReplaySource>> {
    match std::env::var("ANIMUS_REPLAY_SESSION") {
        Ok(path) if !path.trim().is_empty() => Ok(Some(ReplaySource::open(path)?)),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tempfile::TempDir;

    fn tmp_log() -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("decisions.jsonl");
        (dir, path)
    }

    #[test]
    fn round_trip_preserves_event_sequence() {
        let (_dir, path) = tmp_log();
        let recorder = Recorder::create_at(&path).expect("recorder");
        recorder.record(&DecisionEvent::prompt("claude-sonnet", "hello", None)).unwrap();
        recorder.record(&DecisionEvent::response_chunk("stdout", "hi ")).unwrap();
        recorder.record(&DecisionEvent::response_chunk("stdout", "there")).unwrap();
        recorder.record(&DecisionEvent::finished(Some(0))).unwrap();
        drop(recorder);
        let events = ReplaySource::open(&path).expect("replay").drain();
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], DecisionEvent::Prompt { .. }));
        match &events[1] {
            DecisionEvent::ResponseChunk { text, .. } => assert_eq!(text, "hi "),
            other => panic!("unexpected: {:?}", other),
        }
        match &events[2] {
            DecisionEvent::ResponseChunk { text, .. } => assert_eq!(text, "there"),
            other => panic!("unexpected: {:?}", other),
        }
        assert!(matches!(events[3], DecisionEvent::Finished { exit_code: Some(0), .. }));
    }

    #[test]
    fn streaming_chunk_order_is_preserved_under_concurrent_recording() {
        let (_dir, path) = tmp_log();
        let recorder = Recorder::create_at(&path).expect("recorder");
        for i in 0..50 {
            recorder.record(&DecisionEvent::response_chunk("stdout", format!("chunk-{i}"))).unwrap();
        }
        drop(recorder);
        let events = ReplaySource::open(&path).expect("replay").drain();
        let texts: Vec<String> = events.into_iter().filter_map(|e| match e {
            DecisionEvent::ResponseChunk { text, .. } => Some(text),
            _ => None,
        }).collect();
        let expected: Vec<String> = (0..50).map(|i| format!("chunk-{i}")).collect();
        assert_eq!(texts, expected);
    }

    #[test]
    fn tool_result_replay_does_not_re_execute_tool() {
        let (_dir, path) = tmp_log();
        let recorder = Recorder::create_at(&path).expect("recorder");
        static TOOL_EXEC_COUNT: AtomicU32 = AtomicU32::new(0);
        TOOL_EXEC_COUNT.store(0, Ordering::SeqCst);
        let tool_args = serde_json::json!({"path": "/tmp/foo"});
        recorder.record(&DecisionEvent::tool_call("read_file", tool_args.clone())).unwrap();
        let tool_result = {
            TOOL_EXEC_COUNT.fetch_add(1, Ordering::SeqCst);
            serde_json::json!({"content": "side-effect-1"})
        };
        recorder.record(&DecisionEvent::tool_result("read_file", tool_result.clone())).unwrap();
        recorder.record(&DecisionEvent::finished(Some(0))).unwrap();
        drop(recorder);
        assert_eq!(TOOL_EXEC_COUNT.load(Ordering::SeqCst), 1);
        let events = ReplaySource::open(&path).expect("replay").drain();
        let mut saw_call = false;
        let mut saw_result = false;
        for event in events {
            match event {
                DecisionEvent::ToolCall { name, args, .. } => {
                    assert_eq!(name, "read_file");
                    assert_eq!(args, tool_args);
                    saw_call = true;
                }
                DecisionEvent::ToolResult { name, result, .. } => {
                    assert_eq!(name, "read_file");
                    assert_eq!(result, tool_result);
                    saw_result = true;
                }
                _ => {}
            }
        }
        assert!(saw_call && saw_result);
        assert_eq!(TOOL_EXEC_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn truncated_final_line_is_tolerated() {
        let (_dir, path) = tmp_log();
        let recorder = Recorder::create_at(&path).expect("recorder");
        recorder.record(&DecisionEvent::prompt("m", "p", None)).unwrap();
        recorder.record(&DecisionEvent::response_chunk("stdout", "ok")).unwrap();
        drop(recorder);
        let mut file = OpenOptions::new().append(true).open(&path).expect("reopen");
        file.write_all(br#"{"kind":"response_chunk","timestamp_ms":1,"stream":"stdout","tex"#).unwrap();
        drop(file);
        let source = ReplaySource::open(&path).expect("replay tolerates truncation");
        assert!(source.truncated_tail());
        let events = source.drain();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn corrupted_middle_line_aborts_replay() {
        let (_dir, path) = tmp_log();
        {
            let mut file = OpenOptions::new().create(true).append(true).open(&path).unwrap();
            writeln!(file, r#"{{"kind":"prompt","timestamp_ms":1,"model_id":"m","prompt":"p","runtime_contract":null}}"#).unwrap();
            writeln!(file, "not-json-garbage").unwrap();
            writeln!(file, r#"{{"kind":"finished","timestamp_ms":2,"exit_code":0}}"#).unwrap();
        }
        match ReplaySource::open(&path) {
            Ok(_) => panic!("middle corruption must abort"),
            Err(err) => assert!(err.to_string().contains("corrupted"), "unexpected error: {err}"),
        }
    }

    #[test]
    fn unsafe_run_id_is_rejected() {
        assert!(decision_log_path("/tmp", "../escape").is_none());
        assert!(decision_log_path("/tmp", "ok/nope").is_none());
        assert!(decision_log_path("/tmp", "").is_none());
    }

    #[test]
    fn env_replay_source_returns_none_when_unset() {
        if std::env::var("ANIMUS_REPLAY_SESSION").is_err() {
            assert!(env_replay_source().expect("no error").is_none());
        }
    }
}
