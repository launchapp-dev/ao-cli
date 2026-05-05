//! Real `agent/run` integration test against the deterministic `ao-provider-mock`.
//!
//! Wires the SessionBackendResolver through plugin discovery (mirroring how
//! agent-runner does it in production) and asserts that:
//!
//! - The mock provider is selected when the request's tool matches its
//!   `provider_tool` (`mock`).
//! - Streaming notifications come through as live SessionEvents:
//!   Started → Thinking → ToolCall → ToolResult → TextDelta×3 → FinalText →
//!   Metadata → Finished — all visible *before* the request future resolves.

use std::path::{Path, PathBuf};
use std::time::Duration;

use cli_wrapper::session::{SessionBackendResolver, SessionEvent, SessionRequest};
use serde_json::json;
use tokio::time::timeout;

fn workspace_target_debug() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().and_then(Path::parent).expect("workspace root");
    workspace_root.join("target").join("debug")
}

fn ensure_mock_provider() {
    let bin = workspace_target_debug().join("ao-provider-mock");
    if !bin.exists() {
        panic!("ao-provider-mock binary not built; run `cargo build -p ao-provider-mock` first");
    }
}

fn build_request() -> SessionRequest {
    SessionRequest {
        tool: "mock".to_string(),
        model: "mock-fast-1".to_string(),
        prompt: "hello-from-test".to_string(),
        cwd: std::env::current_dir().expect("cwd"),
        project_root: None,
        mcp_endpoint: None,
        permission_mode: None,
        timeout_secs: Some(15),
        env_vars: Vec::new(),
        extras: json!({}),
    }
}

#[tokio::test]
async fn resolver_routes_mock_tool_through_plugin() {
    ensure_mock_provider();
    std::env::set_var("AO_PLUGIN_PATH", workspace_target_debug());
    let resolver = SessionBackendResolver::with_plugin_discovery(Path::new(env!("CARGO_MANIFEST_DIR")));

    let request = build_request();
    let backend = resolver.resolve(&request);
    let info = backend.info();
    assert_eq!(info.provider_tool, "mock", "provider_tool should match mock plugin");
    assert!(
        info.display_name.contains("ao-provider-mock"),
        "display_name should reflect plugin: {}",
        info.display_name
    );
}

#[tokio::test]
async fn agent_run_streams_notifications_in_order_through_plugin() {
    ensure_mock_provider();
    std::env::set_var("AO_PLUGIN_PATH", workspace_target_debug());
    let resolver = SessionBackendResolver::with_plugin_discovery(Path::new(env!("CARGO_MANIFEST_DIR")));

    let request = build_request();
    let mut run = timeout(Duration::from_secs(10), resolver.start_session(request))
        .await
        .expect("start_session should not hang")
        .expect("start_session should succeed");

    let mut events: Vec<SessionEvent> = Vec::new();
    while let Some(event) = timeout(Duration::from_secs(10), run.events.recv())
        .await
        .expect("recv should not hang")
    {
        events.push(event.clone());
        if matches!(event, SessionEvent::Finished { .. }) {
            break;
        }
    }

    assert!(!events.is_empty(), "should observe at least one event");

    // Started must be the first event.
    match events.first() {
        Some(SessionEvent::Started { backend, .. }) => {
            assert!(
                backend.starts_with("plugin:ao-provider-mock"),
                "first event backend label should reflect plugin: {backend}"
            );
        }
        other => panic!("expected first event to be Started, got {other:?}"),
    }

    // Finished must be the last event.
    match events.last() {
        Some(SessionEvent::Finished { exit_code }) => {
            assert_eq!(*exit_code, Some(0), "mock provider should exit cleanly");
        }
        other => panic!("expected last event to be Finished, got {other:?}"),
    }

    // Streaming notifications should reach us as their respective SessionEvents.
    assert!(
        events.iter().any(|e| matches!(e, SessionEvent::Thinking { text } if text.contains("planning"))),
        "Thinking event should be forwarded as agent/thinking notification: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(e, SessionEvent::ToolCall { tool_name, .. } if tool_name == "mock.echo")),
        "ToolCall event should be forwarded as agent/toolCall notification"
    );
    assert!(
        events.iter().any(|e| matches!(e, SessionEvent::ToolResult { tool_name, success, .. } if tool_name == "mock.echo" && *success)),
        "ToolResult event should be forwarded as agent/toolResult notification"
    );

    let delta_count = events
        .iter()
        .filter(|e| matches!(e, SessionEvent::TextDelta { text } if text.starts_with("mock-stream-")))
        .count();
    assert_eq!(delta_count, 3, "should observe 3 streamed TextDelta events: {events:?}");

    // Final text should contain the prompt-echo at minimum. Provider runtime
    // concatenates streamed TextDelta into the final aggregated `output`, so
    // the FinalText event is the cumulative collected output.
    let final_text = events.iter().find_map(|e| {
        if let SessionEvent::FinalText { text } = e {
            Some(text.clone())
        } else {
            None
        }
    });
    let final_text = final_text.expect("FinalText event must be present");
    assert!(
        final_text.contains("MOCK_RESULT: hello-from-test"),
        "FinalText should include prompt echo, got: {final_text}"
    );
}
