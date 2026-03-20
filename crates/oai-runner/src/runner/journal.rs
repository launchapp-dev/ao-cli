use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::api::types::ChatMessage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    AssistantCommitted,
    ToolsPartial,
    ToolsCommitted,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InterruptionKind {
    Signal,
    MaxTurnsReached,
    ApiError,
    MidToolExecution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub turn_index: usize,
    pub assistant_message: ChatMessage,
    pub tool_results: Vec<ChatMessage>,
    pub phase: TurnPhase,
    pub interruption: Option<InterruptionKind>,
}

impl TurnRecord {
    pub fn is_recoverable(&self) -> bool {
        self.interruption.is_none()
            && matches!(self.phase, TurnPhase::ToolsCommitted | TurnPhase::Complete)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionJournal {
    pub session_id: String,
    pub model: String,
    pub preamble: Vec<ChatMessage>,
    pub turns: Vec<TurnRecord>,
    pub interrupted: Option<InterruptionKind>,
}

impl SessionJournal {
    pub fn new(session_id: String, model: String) -> Self {
        Self { session_id, model, preamble: Vec::new(), turns: Vec::new(), interrupted: None }
    }

    pub fn committed_turn_count(&self) -> usize {
        self.turns.iter().filter(|t| t.is_recoverable()).count()
    }

    pub fn to_messages(&self) -> Vec<ChatMessage> {
        let mut msgs = self.preamble.clone();
        for turn in &self.turns {
            if turn.is_recoverable() {
                msgs.push(turn.assistant_message.clone());
                msgs.extend(turn.tool_results.clone());
            }
        }
        msgs
    }

    pub fn begin_turn(&mut self, turn_index: usize, assistant_message: ChatMessage) {
        self.turns.push(TurnRecord {
            turn_index,
            assistant_message,
            tool_results: Vec::new(),
            phase: TurnPhase::AssistantCommitted,
            interruption: None,
        });
    }

    pub fn commit_tools(&mut self, tool_results: Vec<ChatMessage>) {
        if let Some(last) = self.turns.last_mut() {
            last.tool_results = tool_results;
            last.phase = TurnPhase::ToolsCommitted;
        }
    }

    pub fn commit_tools_partial(&mut self, tool_results: Vec<ChatMessage>) {
        if let Some(last) = self.turns.last_mut() {
            last.tool_results = tool_results;
            last.phase = TurnPhase::ToolsPartial;
            last.interruption = Some(InterruptionKind::MidToolExecution);
        }
    }

    pub fn complete_turn(&mut self) {
        if let Some(last) = self.turns.last_mut() {
            last.phase = TurnPhase::Complete;
        }
    }

    pub fn set_interrupted(&mut self, kind: InterruptionKind) {
        if let Some(last) = self.turns.last_mut() {
            if last.interruption.is_none() {
                last.interruption = Some(kind.clone());
            }
        }
        self.interrupted = Some(kind);
    }
}

pub fn session_base_dir() -> PathBuf {
    let dir = std::env::var("AO_CONFIG_DIR")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{}/.ao", h)))
        .unwrap_or_else(|_| ".ao".to_string());
    PathBuf::from(dir)
}

fn session_file_path_in(base: &Path, session_id: &str) -> PathBuf {
    base.join("sessions").join(format!("{}.json", session_id))
}

pub fn load_journal(base: &Path, session_id: &str) -> Option<SessionJournal> {
    let path = session_file_path_in(base, session_id);
    if !path.exists() {
        return None;
    }
    let data = std::fs::read_to_string(&path).ok()?;
    let trimmed = data.trim();
    if trimmed.starts_with('[') {
        let messages: Vec<ChatMessage> = serde_json::from_str(trimmed).ok()?;
        if messages.is_empty() {
            return None;
        }
        let mut journal = SessionJournal::new(session_id.to_string(), String::new());
        journal.preamble = messages;
        return Some(journal);
    }
    serde_json::from_str(trimmed).ok()
}

pub fn save_journal(base: &Path, journal: &SessionJournal) -> Result<()> {
    let path = session_file_path_in(base, &journal.session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(journal)?;
    std::fs::write(&path, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.to_string(), content: Some(content.to_string()), tool_calls: None, tool_call_id: None }
    }

    #[test]
    fn new_journal_is_empty() {
        let j = SessionJournal::new("s1".to_string(), "gpt-4o".to_string());
        assert_eq!(j.committed_turn_count(), 0);
        assert!(j.to_messages().is_empty());
    }

    #[test]
    fn complete_turn_is_recoverable() {
        let mut j = SessionJournal::new("s1".to_string(), "gpt-4o".to_string());
        j.preamble = vec![msg("system", "You help."), msg("user", "task")];
        j.begin_turn(0, msg("assistant", "Hello"));
        j.complete_turn();
        assert_eq!(j.committed_turn_count(), 1);
        let msgs = j.to_messages();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn assistant_committed_turn_is_not_recoverable() {
        let mut j = SessionJournal::new("s1".to_string(), "gpt-4o".to_string());
        j.begin_turn(0, msg("assistant", "Working..."));
        assert_eq!(j.committed_turn_count(), 0);
        assert!(j.to_messages().is_empty());
    }

    #[test]
    fn tools_committed_turn_is_recoverable() {
        let mut j = SessionJournal::new("s1".to_string(), "gpt-4o".to_string());
        j.preamble = vec![msg("user", "Do task")];
        j.begin_turn(0, msg("assistant", "I'll read the file"));
        j.commit_tools(vec![msg("tool", "file contents here")]);
        assert_eq!(j.committed_turn_count(), 1);
        let msgs = j.to_messages();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn interrupted_turn_excluded_from_to_messages() {
        let mut j = SessionJournal::new("s1".to_string(), "gpt-4o".to_string());
        j.preamble = vec![msg("user", "task")];
        j.begin_turn(0, msg("assistant", "doing..."));
        j.commit_tools_partial(vec![]);
        assert_eq!(j.committed_turn_count(), 0);
        let msgs = j.to_messages();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn multiple_turns_only_committed_included() {
        let mut j = SessionJournal::new("s1".to_string(), "m".to_string());
        j.preamble = vec![msg("user", "go")];
        j.begin_turn(0, msg("assistant", "reading..."));
        j.commit_tools(vec![msg("tool", "data")]);
        j.begin_turn(1, msg("assistant", "done"));
        j.complete_turn();
        j.begin_turn(2, msg("assistant", "partial..."));
        assert_eq!(j.committed_turn_count(), 2);
        let msgs = j.to_messages();
        assert_eq!(msgs.len(), 4);
    }

    #[test]
    fn set_interrupted_marks_last_turn_and_journal() {
        let mut j = SessionJournal::new("s1".to_string(), "m".to_string());
        j.begin_turn(0, msg("assistant", "..."));
        j.set_interrupted(InterruptionKind::Signal);
        assert!(matches!(j.interrupted, Some(InterruptionKind::Signal)));
        assert!(matches!(j.turns[0].interruption, Some(InterruptionKind::Signal)));
    }

    #[test]
    fn set_interrupted_without_turns_sets_journal_only() {
        let mut j = SessionJournal::new("s1".to_string(), "m".to_string());
        j.set_interrupted(InterruptionKind::MaxTurnsReached);
        assert!(matches!(j.interrupted, Some(InterruptionKind::MaxTurnsReached)));
        assert!(j.turns.is_empty());
    }

    #[test]
    fn migrate_flat_format_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sid = "flat-session";
        let path = base.join("sessions").join(format!("{}.json", sid));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let flat = serde_json::json!([
            {"role": "system", "content": "you help"},
            {"role": "user", "content": "hi"},
            {"role": "assistant", "content": "hello"}
        ]);
        std::fs::write(&path, flat.to_string()).unwrap();
        let journal = load_journal(base, sid).unwrap();
        assert_eq!(journal.preamble.len(), 3);
        assert_eq!(journal.turns.len(), 0);
    }

    #[test]
    fn journal_round_trips_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sid = "test-journal";
        let mut j = SessionJournal::new(sid.to_string(), "gpt-4o".to_string());
        j.preamble = vec![msg("system", "helpful"), msg("user", "task")];
        j.begin_turn(0, msg("assistant", "I'll help"));
        j.complete_turn();
        save_journal(base, &j).unwrap();
        let loaded = load_journal(base, sid).unwrap();
        assert_eq!(loaded.committed_turn_count(), 1);
        assert_eq!(loaded.preamble.len(), 2);
    }

    #[test]
    fn load_nonexistent_journal_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_journal(dir.path(), "no-such-session").is_none());
    }

    #[test]
    fn interrupted_journal_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        let sid = "interrupted";
        let mut j = SessionJournal::new(sid.to_string(), "m".to_string());
        j.preamble = vec![msg("user", "task")];
        j.begin_turn(0, msg("assistant", "reading..."));
        j.set_interrupted(InterruptionKind::Signal);
        save_journal(base, &j).unwrap();
        let loaded = load_journal(base, sid).unwrap();
        assert!(matches!(loaded.interrupted, Some(InterruptionKind::Signal)));
        assert_eq!(loaded.committed_turn_count(), 0);
    }
}
