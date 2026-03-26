use async_trait::async_trait;

use crate::error::Result;

use super::transport::{start_codex_session, terminate_codex_session};
use crate::session::{
    session_backend::SessionBackend, session_backend_info::SessionBackendInfo,
    session_backend_kind::SessionBackendKind, session_capabilities::SessionCapabilities,
    session_request::SessionRequest, session_run::SessionRun, session_stability::SessionStability,
};

pub struct CodexSessionBackend;

impl CodexSessionBackend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionBackend for CodexSessionBackend {
    fn info(&self) -> SessionBackendInfo {
        SessionBackendInfo {
            kind: SessionBackendKind::CodexSdk,
            provider_tool: "codex".to_string(),
            stability: SessionStability::Experimental,
            display_name: "Codex Native Backend".to_string(),
        }
    }

    fn capabilities(&self) -> SessionCapabilities {
        SessionCapabilities {
            supports_resume: true,
            supports_terminate: true,
            supports_permissions: true,
            supports_mcp: true,
            supports_tool_events: true,
            supports_thinking_events: true,
            supports_artifact_events: true,
            supports_usage_metadata: true,
        }
    }

    async fn start_session(&self, request: SessionRequest) -> Result<SessionRun> {
        start_codex_session(request, false).await
    }

    async fn resume_session(&self, request: SessionRequest, _session_id: &str) -> Result<SessionRun> {
        start_codex_session(request, true).await
    }

    async fn terminate_session(&self, session_id: &str) -> Result<()> {
        terminate_codex_session(session_id).await
    }
}

impl Default for CodexSessionBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::super::{parser::parse_codex_stdout_line, transport::codex_invocation_for_request};
    use super::CodexSessionBackend;
    use crate::session::{SessionBackend, SessionEvent, SessionRequest};

    #[test]
    fn codex_invocation_defaults_to_json_and_full_auto() {
        let request = SessionRequest {
            tool: "codex".to_string(),
            model: "gpt-5".to_string(),
            prompt: "hello".to_string(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({}),
        };

        let invocation = codex_invocation_for_request(&request, false).expect("launch should build");
        assert_eq!(invocation.command, "codex");
        assert!(invocation.args.contains(&"exec".to_string()));
        assert!(invocation.args.contains(&"--json".to_string()));
        assert!(invocation.args.contains(&"--full-auto".to_string()));
    }

    #[test]
    fn codex_parser_emits_thinking_usage_and_final_text() {
        let reasoning = r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"thinking"}}"#;
        let message = r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"done"}}"#;
        let completed = r#"{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":2}}"#;

        assert_eq!(parse_codex_stdout_line(reasoning), vec![SessionEvent::Thinking { text: "thinking".to_string() }]);
        assert_eq!(parse_codex_stdout_line(message), vec![SessionEvent::FinalText { text: "done".to_string() }]);
        match parse_codex_stdout_line(completed).as_slice() {
            [SessionEvent::Metadata { metadata }] => {
                assert_eq!(metadata.get("type").and_then(|value| value.as_str()), Some("codex_usage"));
                assert_eq!(metadata.pointer("/usage/input_tokens").and_then(|value| value.as_u64()), Some(1));
                assert_eq!(metadata.pointer("/usage/output_tokens").and_then(|value| value.as_u64()), Some(2));
                assert!(metadata.get("raw").is_some(), "expected raw Codex usage metadata to be preserved");
            }
            other => panic!("expected metadata event, got: {:?}", other),
        }
    }

    #[test]
    fn codex_parser_emits_tool_call_from_function_call_item() {
        let line = r#"{"type":"item.completed","item":{"id":"item_2","type":"function_call","call_id":"call_abc","name":"read_file","arguments":"{\"path\":\"/tmp/foo\"}"}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], SessionEvent::ToolCall { tool_name, .. } if tool_name == "read_file"));
        if let SessionEvent::ToolCall { arguments, .. } = &events[0] {
            assert_eq!(arguments.get("path").and_then(|v| v.as_str()), Some("/tmp/foo"));
        }
    }

    #[test]
    fn codex_parser_emits_tool_result_from_function_call_output_item() {
        let line = r#"{"type":"item.completed","item":{"id":"item_3","type":"function_call_output","call_id":"call_abc","output":"file contents"}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], SessionEvent::ToolResult { success: true, .. }));
        if let SessionEvent::ToolResult { tool_name, output, .. } = &events[0] {
            assert_eq!(tool_name, "call_abc");
            assert_eq!(output.as_str(), Some("file contents"));
        }
    }

    #[test]
    fn codex_parser_emits_tool_call_from_shell_call_item() {
        let line = r#"{"type":"item.completed","item":{"id":"item_4","type":"shell_call","call_id":"call_sh1","action":{"type":"exec","command":"ls -la","env":{}}}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], SessionEvent::ToolCall { tool_name, .. } if tool_name == "shell"));
        if let SessionEvent::ToolCall { arguments, .. } = &events[0] {
            assert_eq!(arguments.get("command").and_then(|v| v.as_str()), Some("ls -la"));
        }
    }

    #[test]
    fn codex_parser_emits_tool_result_from_shell_call_output_item() {
        let line = r#"{"type":"item.completed","item":{"id":"item_5","type":"shell_call_output","call_id":"call_sh1","output":"file1\nfile2\n","metadata":{"exit_code":0}}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], SessionEvent::ToolResult { tool_name, success: true, .. } if tool_name == "shell")
        );
    }

    #[test]
    fn codex_parser_shell_call_output_marks_failure_on_nonzero_exit() {
        let line = r#"{"type":"item.completed","item":{"id":"item_6","type":"shell_call_output","call_id":"call_sh2","output":"error","metadata":{"exit_code":1}}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], SessionEvent::ToolResult { success: false, .. }));
    }

    #[test]
    fn codex_parser_emits_error_from_top_level_error_event() {
        let line = r#"{"type":"error","error":{"message":"rate limit exceeded","code":"rate_limit"}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        assert!(
            matches!(&events[0], SessionEvent::Error { message, recoverable: false } if message == "rate limit exceeded")
        );
    }

    #[test]
    fn codex_parser_emits_error_from_error_item() {
        let line =
            r#"{"type":"item.completed","item":{"id":"item_7","type":"error","message":"tool execution failed"}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], SessionEvent::Error { message, .. } if message == "tool execution failed"));
    }

    #[test]
    fn codex_parser_emits_artifact_from_artifact_item() {
        let line = r#"{"type":"item.completed","item":{"id":"artifact_1","type":"artifact","file_path":"out.txt","mime_type":"text/plain","size_bytes":12}}"#;
        let events = parse_codex_stdout_line(line);
        assert_eq!(events.len(), 1);
        match &events[0] {
            SessionEvent::Artifact { artifact_id, metadata } => {
                assert_eq!(artifact_id, "artifact_1");
                assert_eq!(metadata.get("file_path").and_then(|value| value.as_str()), Some("out.txt"));
                assert_eq!(metadata.get("mime_type").and_then(|value| value.as_str()), Some("text/plain"));
                assert_eq!(metadata.get("size_bytes").and_then(|value| value.as_u64()), Some(12));
            }
            other => panic!("expected artifact event, got: {:?}", other),
        }
    }

    #[test]
    fn codex_backend_reports_tool_events_capability() {
        let backend = CodexSessionBackend::new();
        assert!(backend.capabilities().supports_tool_events);
        assert!(backend.capabilities().supports_artifact_events);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn codex_backend_uses_codex_native_label() {
        let backend = CodexSessionBackend::new();
        let request = SessionRequest {
            tool: "sh".to_string(),
            model: String::new(),
            prompt: String::new(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({
                "runtime_contract": {
                    "cli": {
                        "launch": {
                            "command": "sh",
                            "args": ["-c", "printf 'codex-native\\n'"],
                            "prompt_via_stdin": false
                        }
                    }
                }
            }),
        };

        let mut run = backend.start_session(request).await.expect("session should start");

        assert_eq!(run.selected_backend, "codex-native");

        let started = run.events.recv().await.expect("started event");
        assert!(matches!(
            started,
            SessionEvent::Started { backend, .. } if backend == "codex-native"
        ));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn codex_backend_emits_thinking_and_final_text_from_fixture() {
        let backend = CodexSessionBackend::new();
        let fixture = "/Users/samishukri/ao-cli/crates/llm-cli-wrapper/tests/fixtures/codex_real.jsonl";
        let request = SessionRequest {
            tool: "sh".to_string(),
            model: String::new(),
            prompt: String::new(),
            cwd: PathBuf::from("."),
            project_root: None,
            mcp_endpoint: None,
            permission_mode: None,
            timeout_secs: None,
            env_vars: Vec::new(),
            extras: json!({
                "runtime_contract": {
                    "cli": {
                        "launch": {
                            "command": "sh",
                            "args": ["-c", format!("cat {fixture}")],
                            "prompt_via_stdin": false
                        }
                    }
                }
            }),
        };

        let mut run = backend.start_session(request).await.expect("session should start");

        let mut saw_thinking = false;
        let mut saw_final_text = false;
        let mut saw_metadata = false;

        while let Some(event) = run.events.recv().await {
            match event {
                SessionEvent::Thinking { .. } => saw_thinking = true,
                SessionEvent::FinalText { text } if text == "PINEAPPLE_42" => {
                    saw_final_text = true;
                }
                SessionEvent::Metadata { .. } => saw_metadata = true,
                SessionEvent::Finished { .. } => break,
                _ => {}
            }
        }

        assert!(saw_thinking, "expected codex thinking event");
        assert!(saw_metadata, "expected codex usage metadata");
        assert!(saw_final_text, "expected final text from codex fixture");
    }
}
