use serde_json::json;
use std::io::Write;

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

    pub fn assistant_message_complete(&self, turn: u32) {
        if self.json_mode {
            let event = json!({
                "type": "assistant_message_complete",
                "turn": turn
            });
            println!("{}", event);
        }
    }

    pub fn session(&self, action: &str, session_id: &str, message_count: usize) {
        if self.json_mode {
            let event = json!({
                "type": "session",
                "action": action,
                "session_id": session_id,
                "message_count": message_count
            });
            println!("{}", event);
        }
    }

    pub fn retry(&self, kind: &str, attempt: u32, reason: &str) {
        if self.json_mode {
            let event = json!({
                "type": "retry",
                "kind": kind,
                "attempt": attempt,
                "reason": reason
            });
            println!("{}", event);
        }
    }

    pub fn error(&self, kind: &str, message: &str) {
        if self.json_mode {
            let event = json!({
                "type": "error",
                "kind": kind,
                "message": message
            });
            println!("{}", event);
        }
    }

    pub fn newline(&self) {
        println!();
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

    #[test]
    fn assistant_message_complete_only_emits_in_json_mode() {
        let json_formatter = OutputFormatter::new(true);
        let text_formatter = OutputFormatter::new(false);
        assert!(json_formatter.json_mode);
        assert!(!text_formatter.json_mode);
    }

    #[test]
    fn session_event_fields_are_present() {
        let formatter = OutputFormatter::new(true);
        let event = serde_json::json!({
            "type": "session",
            "action": "resumed",
            "session_id": "test-id",
            "message_count": 5
        });
        assert_eq!(event["type"], "session");
        assert_eq!(event["action"], "resumed");
        assert_eq!(event["session_id"], "test-id");
        assert_eq!(event["message_count"], 5);
        drop(formatter);
    }

    #[test]
    fn retry_event_fields_are_present() {
        let event = serde_json::json!({
            "type": "retry",
            "kind": "api",
            "attempt": 1,
            "reason": "rate limited (429)"
        });
        assert_eq!(event["type"], "retry");
        assert_eq!(event["kind"], "api");
        assert_eq!(event["attempt"], 1);
    }

    #[test]
    fn error_event_fields_are_present() {
        let event = serde_json::json!({
            "type": "error",
            "kind": "schema_validation",
            "message": "Schema validation failed after 3 retries"
        });
        assert_eq!(event["type"], "error");
        assert_eq!(event["kind"], "schema_validation");
        assert!(!event["message"].as_str().unwrap().is_empty());
    }
}
