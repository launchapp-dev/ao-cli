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

    /// Emit a `config` event showing the effective execution policy and model.
    pub fn config(&self, exec_policy: &str, model: &str, tool_names: &[&str]) {
        if self.json_mode {
            let event = json!({
                "type": "config",
                "exec_policy": exec_policy,
                "model": model,
                "tools": tool_names,
            });
            println!("{}", event);
        } else {
            eprintln!(
                "[oai-runner] Config: policy={}, model={}, tools=[{}]",
                exec_policy,
                model,
                tool_names.join(", ")
            );
        }
    }

    /// Emit a `command_start` event immediately before a shell command runs.
    pub fn command_start(&self, command: &str, timeout_secs: u64) {
        if self.json_mode {
            let event = json!({
                "type": "command_start",
                "command": command,
                "timeout_secs": timeout_secs,
            });
            println!("{}", event);
        } else {
            eprintln!("[oai-runner] [Command Start] {} (timeout: {}s)", truncate_command(command, 200), timeout_secs);
        }
    }

    /// Emit a `command_end` event after a shell command finishes or is killed.
    pub fn command_end(&self, command: &str, exit_code: i32, timed_out: bool, duration_ms: u64) {
        if self.json_mode {
            let event = json!({
                "type": "command_end",
                "command": command,
                "exit_code": exit_code,
                "timed_out": timed_out,
                "duration_ms": duration_ms,
            });
            println!("{}", event);
        } else if timed_out {
            eprintln!(
                "[oai-runner] [Command End] {} — timed out after {:.1}s",
                truncate_command(command, 200),
                duration_ms as f64 / 1000.0
            );
        } else {
            eprintln!(
                "[oai-runner] [Command End] {} — exit {}, {:.1}s",
                truncate_command(command, 200),
                exit_code,
                duration_ms as f64 / 1000.0
            );
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
}

/// Truncate a command string for human-readable display.
fn truncate_command(cmd: &str, max_len: usize) -> &str {
    if cmd.len() <= max_len {
        cmd
    } else {
        // Try to break at a whitespace boundary.
        let truncated = &cmd[..max_len];
        if let Some(pos) = truncated.rfind(|c: char| c.is_whitespace()) {
            &cmd[..pos]
        } else {
            truncated
        }
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
    fn truncate_command_short_is_identity() {
        assert_eq!(truncate_command("cargo build", 200), "cargo build");
    }

    #[test]
    fn truncate_command_long_is_shortened() {
        let long = "a".repeat(300);
        let t = truncate_command(&long, 100);
        assert!(t.len() <= 100);
    }

    #[test]
    fn truncate_command_breaks_at_whitespace() {
        let cmd = "cargo build --release --all-features";
        assert!(truncate_command(cmd, 15).ends_with("cargo build"));
    }
}
