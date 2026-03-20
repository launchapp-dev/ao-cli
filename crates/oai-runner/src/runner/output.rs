use serde_json::json;
use std::io::Write;

use super::journal::{InterruptionKind, TurnPhase};

pub struct OutputFormatter {
    json_mode: bool,
    text_buffer: String,
    total_input_tokens: u64,
    total_output_tokens: u64,
    request_count: u32,
}

impl OutputFormatter {
    pub fn new(json_mode: bool) -> Self {
        Self { json_mode, text_buffer: String::new(), total_input_tokens: 0, total_output_tokens: 0, request_count: 0 }
    }

    pub fn text_chunk(&mut self, text: &str) {
        if self.json_mode {
            self.text_buffer.push_str(text);
        } else {
            print!("{}", text);
            std::io::stdout().flush().ok();
        }
    }

    pub fn flush_result(&mut self) {
        if self.json_mode && !self.text_buffer.is_empty() {
            let event = json!({
                "type": "result",
                "text": self.text_buffer
            });
            println!("{}", event);
            self.text_buffer.clear();
        }
    }

    pub fn tool_call(&self, tool_name: &str, arguments: &serde_json::Value) {
        if self.json_mode {
            let event = json!({
                "type": "tool_call",
                "tool_name": tool_name,
                "arguments": arguments
            });
            println!("{}", event);
        }
    }

    pub fn tool_result(&self, tool_name: &str, result: &str) {
        if self.json_mode {
            let event = json!({
                "type": "tool_result",
                "tool_name": tool_name,
                "output": result
            });
            println!("{}", event);
        } else {
            println!("\n[Tool Result: {}]", tool_name);
            println!("{}", result);
        }
    }

    pub fn tool_error(&self, tool_name: &str, error: &str) {
        if self.json_mode {
            let event = json!({
                "type": "tool_error",
                "tool_name": tool_name,
                "error": error
            });
            println!("{}", event);
        } else {
            eprintln!("\n[Tool Error: {}] {}", tool_name, error);
        }
    }

    pub fn metadata(&mut self, input_tokens: u64, output_tokens: u64) {
        self.total_input_tokens += input_tokens;
        self.total_output_tokens += output_tokens;
        self.request_count += 1;
        if self.json_mode {
            let event = json!({
                "type": "metadata",
                "tokens": {
                    "input": input_tokens,
                    "output": output_tokens
                }
            });
            println!("{}", event);
        }
    }

    pub fn emit_session_summary(&self) {
        let total = self.total_input_tokens + self.total_output_tokens;
        if total == 0 {
            return;
        }
        if self.json_mode {
            let event = json!({
                "type": "session_summary",
                "tokens": {
                    "total_input": self.total_input_tokens,
                    "total_output": self.total_output_tokens,
                    "total": total,
                    "requests": self.request_count
                }
            });
            println!("{}", event);
        } else {
            eprintln!(
                "[oai-runner] Session: {} requests, {} input + {} output = {} total tokens",
                self.request_count, self.total_input_tokens, self.total_output_tokens, total
            );
        }
    }

    pub fn newline(&self) {
        println!();
    }

    pub fn emit_session_resumed(
        &self,
        session_id: &str,
        committed_turns: usize,
        prior_interruption: Option<&InterruptionKind>,
    ) {
        let kind_str = prior_interruption.map(interruption_kind_str);
        if self.json_mode {
            let event = json!({
                "type": "session_resumed",
                "session_id": session_id,
                "committed_turns": committed_turns,
                "prior_interruption": kind_str
            });
            println!("{}", event);
        } else {
            eprintln!(
                "[oai-runner] Resuming session {} ({} committed turns{})",
                session_id,
                committed_turns,
                kind_str.map(|k| format!(", prior interruption: {}", k)).unwrap_or_default()
            );
        }
    }

    pub fn emit_turn_committed(&self, turn_index: usize, phase: &TurnPhase) {
        if self.json_mode {
            let event = json!({
                "type": "turn_committed",
                "turn_index": turn_index,
                "phase": turn_phase_str(phase)
            });
            println!("{}", event);
        }
    }

    pub fn emit_session_interrupted(&self, kind: &InterruptionKind) {
        if self.json_mode {
            let event = json!({
                "type": "session_interrupted",
                "reason": interruption_kind_str(kind)
            });
            println!("{}", event);
        } else {
            eprintln!("[oai-runner] Session interrupted: {}", interruption_kind_str(kind));
        }
    }
}

fn turn_phase_str(phase: &TurnPhase) -> &'static str {
    match phase {
        TurnPhase::AssistantCommitted => "assistant_committed",
        TurnPhase::ToolsPartial => "tools_partial",
        TurnPhase::ToolsCommitted => "tools_committed",
        TurnPhase::Complete => "complete",
    }
}

fn interruption_kind_str(kind: &InterruptionKind) -> &'static str {
    match kind {
        InterruptionKind::Signal => "signal",
        InterruptionKind::MaxTurnsReached => "max_turns_reached",
        InterruptionKind::ApiError => "api_error",
        InterruptionKind::MidToolExecution => "mid_tool_execution",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_formatter_json_mode_initializes_empty_buffer() {
        let formatter = OutputFormatter::new(true);
        assert!(formatter.text_buffer.is_empty());
        assert!(formatter.json_mode);
    }

    #[test]
    fn output_formatter_text_mode_does_not_buffer() {
        let formatter = OutputFormatter::new(false);
        assert!(!formatter.json_mode);
        assert!(formatter.text_buffer.is_empty());
    }

    #[test]
    fn text_chunk_accumulates_in_buffer_for_json_mode() {
        let mut formatter = OutputFormatter::new(true);
        formatter.text_buffer.push_str("hello ");
        formatter.text_buffer.push_str("world");
        assert_eq!(formatter.text_buffer, "hello world");
    }

    #[test]
    fn flush_result_clears_buffer() {
        let mut formatter = OutputFormatter::new(true);
        formatter.text_buffer.push_str("accumulated text");
        assert!(!formatter.text_buffer.is_empty());
        formatter.text_buffer.clear();
        assert!(formatter.text_buffer.is_empty());
    }
}
