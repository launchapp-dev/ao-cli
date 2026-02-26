use protocol::*;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let previous = std::env::var(key).ok();
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(self.key, previous);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

#[test]
fn test_agent_run_request_roundtrip() {
    let req = AgentRunRequest {
        protocol_version: PROTOCOL_VERSION.to_string(),
        run_id: RunId("run-123".into()),
        model: ModelId("claude-sonnet-4".into()),
        context: serde_json::json!({ "tool": "claude", "prompt": "Hello", "cwd": "/tmp" }),
        timeout_secs: Some(300),
    };

    let json = serde_json::to_string(&req).unwrap();
    let parsed: AgentRunRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.run_id.0, "run-123");
}

#[test]
fn test_agent_run_event_serialization() {
    let evt = AgentRunEvent::OutputChunk {
        run_id: RunId("run-123".into()),
        stream_type: OutputStreamType::Stdout,
        text: "chunk".into(),
    };
    let json = serde_json::to_string(&evt).unwrap();
    assert!(json.contains("output_chunk"));
    assert!(json.contains("stdout"));
}

#[test]
fn test_agent_control_request() {
    let req = AgentControlRequest {
        run_id: RunId("run-456".into()),
        action: AgentControlAction::Pause,
    };

    let json = serde_json::to_string(&req).unwrap();
    let parsed: AgentControlRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.action, AgentControlAction::Pause);
}

#[test]
fn test_agent_status_response() {
    let resp = AgentStatusResponse {
        run_id: RunId("run-789".into()),
        status: AgentStatus::Running,
        elapsed_ms: 45000,
        started_at: Timestamp::now(),
        completed_at: None,
    };

    let json = serde_json::to_string(&resp).unwrap();
    let parsed: AgentStatusResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.status, AgentStatus::Running);
    assert_eq!(parsed.elapsed_ms, 45000);
}

#[test]
fn test_model_availability_enum() {
    let status = ModelStatus {
        model: ModelId("claude-sonnet-4".into()),
        availability: ModelAvailability::Available,
        details: None,
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("available"));
}

#[test]
fn test_project_model_config() {
    let config = ProjectModelConfig {
        project_id: ProjectId("proj-123".into()),
        allowed_models: vec![
            ModelId("claude-sonnet-4".into()),
            ModelId("gpt-4-turbo".into()),
        ],
        phase_defaults: WorkflowPhaseModelDefaults {
            design: Some(ModelId("gemini-3-pro".into())),
            development: Some(ModelId("claude-sonnet-4".into())),
            quality_assurance: Some(ModelId("claude-sonnet-4".into())),
            review: Some(ModelId("gpt-4-turbo".into())),
            deploy: None,
        },
        fallback_model: Some(ModelId("claude-sonnet-4".into())),
    };

    let json = serde_json::to_string(&config).unwrap();
    let parsed: ProjectModelConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.allowed_models.len(), 2);
    assert!(parsed.phase_defaults.design.is_some());
}

#[test]
fn test_runner_status_request_rejects_unexpected_fields() {
    let parsed = serde_json::from_str::<RunnerStatusRequest>(
        r#"{"run_id":"run-cli-control","action":"terminate"}"#,
    );
    assert!(
        parsed.is_err(),
        "runner status request must reject control-shaped payloads"
    );
}

#[test]
fn test_runner_status_response_roundtrip_includes_protocol_metadata() {
    let response = RunnerStatusResponse {
        active_agents: 2,
        protocol_version: PROTOCOL_VERSION.to_string(),
        build_id: Some("1700000000.123-987654".to_string()),
    };

    let json = serde_json::to_string(&response).expect("serialize runner status");
    let parsed: RunnerStatusResponse =
        serde_json::from_str(&json).expect("deserialize runner status");

    assert_eq!(parsed.active_agents, 2);
    assert_eq!(parsed.protocol_version, PROTOCOL_VERSION);
    assert_eq!(parsed.build_id.as_deref(), Some("1700000000.123-987654"));
}

#[test]
fn test_runner_status_response_deserializes_legacy_shape() {
    let parsed: RunnerStatusResponse =
        serde_json::from_str(r#"{"active_agents":3}"#).expect("deserialize legacy status");

    assert_eq!(parsed.active_agents, 3);
    assert_eq!(parsed.protocol_version, PROTOCOL_VERSION);
    assert!(parsed.build_id.is_none());
}

#[test]
fn test_ipc_auth_request_roundtrip() {
    let request = IpcAuthRequest::new("secret-token");
    let json = serde_json::to_string(&request).expect("serialize auth request");
    assert_eq!(json, r#"{"kind":"ipc_auth","token":"secret-token"}"#);

    let parsed: IpcAuthRequest = serde_json::from_str(&json).expect("deserialize auth request");
    assert_eq!(parsed.kind, IpcAuthRequestKind::IpcAuth);
    assert_eq!(parsed.token, "secret-token");
}

#[test]
fn test_ipc_auth_request_rejects_unknown_fields() {
    let parsed = serde_json::from_str::<IpcAuthRequest>(
        r#"{"kind":"ipc_auth","token":"secret","extra":"value"}"#,
    );
    assert!(
        parsed.is_err(),
        "auth request must reject unknown fields to keep handshake strict"
    );
}

#[test]
fn test_ipc_auth_result_failure_roundtrip() {
    let result = IpcAuthResult::rejected(IpcAuthFailureCode::InvalidToken, "unauthorized");
    let json = serde_json::to_string(&result).expect("serialize auth failure");
    assert_eq!(
        json,
        r#"{"kind":"ipc_auth_result","ok":false,"code":"invalid_token","message":"unauthorized"}"#
    );

    let parsed: IpcAuthResult = serde_json::from_str(&json).expect("deserialize auth failure");
    assert!(!parsed.ok);
    assert_eq!(parsed.code, Some(IpcAuthFailureCode::InvalidToken));
    assert_eq!(parsed.message.as_deref(), Some("unauthorized"));
}

#[test]
fn test_config_get_token_uses_env_over_config() {
    let _lock = env_lock().lock().expect("env lock");
    let _env = EnvVarGuard::set("AGENT_RUNNER_TOKEN", Some("env-token"));
    let config = Config {
        agent_runner_token: Some("config-token".to_string()),
    };

    let token = config.get_token().expect("env token should resolve");
    assert_eq!(token, "env-token");
}

#[test]
fn test_config_get_token_rejects_blank_env_value() {
    let _lock = env_lock().lock().expect("env lock");
    let _env = EnvVarGuard::set("AGENT_RUNNER_TOKEN", Some("   "));
    let config = Config {
        agent_runner_token: Some("config-token".to_string()),
    };

    let error = config
        .get_token()
        .expect_err("blank env token should fail closed");
    assert!(
        error.to_string().contains("AGENT_RUNNER_TOKEN"),
        "error should mention env token source"
    );
}

#[test]
fn test_config_get_token_rejects_blank_config_value() {
    let _lock = env_lock().lock().expect("env lock");
    let _env = EnvVarGuard::set("AGENT_RUNNER_TOKEN", None);
    let config = Config {
        agent_runner_token: Some("   ".to_string()),
    };

    let error = config
        .get_token()
        .expect_err("blank config token should fail closed");
    assert!(
        error.to_string().contains("agent_runner_token"),
        "error should mention config token source"
    );
}

#[test]
fn test_config_get_token_rejects_missing_token() {
    let _lock = env_lock().lock().expect("env lock");
    let _env = EnvVarGuard::set("AGENT_RUNNER_TOKEN", None);
    let config = Config {
        agent_runner_token: None,
    };

    let error = config
        .get_token()
        .expect_err("missing token should fail closed");
    assert!(
        error.to_string().contains("agent_runner_token"),
        "error should mention missing config token"
    );
}
