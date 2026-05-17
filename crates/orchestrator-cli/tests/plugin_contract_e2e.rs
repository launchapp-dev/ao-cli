//! End-to-end contract tests for the AO STDIO plugin surface.
//!
//! Drives the real `animus` binary against the bundled `animus-plugin-smoke`,
//! `animus-provider-claude`, and `animus-provider-codex` plugin binaries via the
//! `ao plugin {list,info,call,ping}` CLI. Locks in:
//!
//! - Discovery via `ANIMUS_PLUGIN_PATH`
//! - Manifest contract (name, version, plugin_kind, capabilities)
//! - Plugin lifecycle (initialize handshake, $/ping)
//! - JSON-RPC dispatch through `ao plugin call`
//!
//! Provider plugin `agent/run` is intentionally NOT exercised here because that
//! would require a real Claude/Codex CLI install. The contract that matters at
//! this layer is that the plugin discovers, initializes, and accepts the
//! request frame; the wrapped session backend is unit-tested in llm-cli-wrapper.

#[path = "support/test_harness.rs"]
pub mod test_harness;

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use test_harness::CliHarness;

fn target_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().and_then(|p| p.parent()).expect("workspace root");
    let workspace_target = workspace_root.join("target").join("debug");
    if workspace_target.exists() {
        return workspace_target;
    }
    manifest_dir.join("target").join("debug")
}

fn ensure_plugin_binary(name: &str) -> Result<PathBuf> {
    let candidate = target_dir().join(name);
    if !candidate.exists() {
        return Err(anyhow!("{name} not built; run `cargo build -p {name}` before running plugin_contract_e2e tests"));
    }
    Ok(candidate)
}

fn plugin_path_env() -> Result<String> {
    let dir = target_dir();
    if !dir.exists() {
        return Err(anyhow!("workspace target/debug not found: {}", dir.display()));
    }
    Ok(dir.to_string_lossy().to_string())
}

fn run_plugin_command(args: &[&str]) -> Result<Value> {
    let plugin_path = plugin_path_env()?;
    let harness = CliHarness::new()?;
    harness.run_json_ok_with_env(args, &[("ANIMUS_PLUGIN_PATH", plugin_path.as_str())])
}

#[test]
fn plugin_list_discovers_smoke_and_provider_plugins() -> Result<()> {
    ensure_plugin_binary("animus-plugin-smoke")?;
    ensure_plugin_binary("animus-provider-claude")?;
    ensure_plugin_binary("animus-provider-codex")?;

    let response = run_plugin_command(&["plugin", "list"])?;
    let plugins = response
        .pointer("/data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("data should be an array: {response}"))?;
    let names: Vec<&str> = plugins.iter().filter_map(|p| p.get("name").and_then(Value::as_str)).collect();

    for required in ["animus-plugin-smoke", "animus-provider-claude", "animus-provider-codex"] {
        assert!(names.contains(&required), "{required} should be discovered; got {names:?}");
    }

    for plugin in plugins {
        for required in ["name", "version", "plugin_kind", "description", "source", "path"] {
            assert!(plugin.get(required).is_some(), "discovered plugin entry should include `{required}`: {plugin}");
        }
    }

    Ok(())
}

#[test]
fn plugin_info_completes_handshake_for_smoke() -> Result<()> {
    ensure_plugin_binary("animus-plugin-smoke")?;
    let response = run_plugin_command(&["plugin", "info", "--name", "animus-plugin-smoke"])?;
    let init = response.pointer("/data/initialize").context("initialize block missing")?;
    assert_eq!(
        init.pointer("/plugin_info/name").and_then(Value::as_str),
        Some("animus-plugin-smoke"),
        "plugin_info.name should match: {init}"
    );
    assert_eq!(
        init.pointer("/plugin_info/plugin_kind").and_then(Value::as_str),
        Some("subject_backend"),
        "smoke plugin should advertise subject_backend kind: {init}"
    );
    assert!(
        init.pointer("/capabilities/subject_kinds")
            .and_then(Value::as_array)
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("smoke"))),
        "smoke plugin should advertise subject_kinds=[smoke]: {init}"
    );
    Ok(())
}

#[test]
fn plugin_info_completes_handshake_for_claude_provider() -> Result<()> {
    ensure_plugin_binary("animus-provider-claude")?;
    let response = run_plugin_command(&["plugin", "info", "--name", "animus-provider-claude"])?;
    let init = response.pointer("/data/initialize").context("initialize block missing")?;
    assert_eq!(
        init.pointer("/plugin_info/plugin_kind").and_then(Value::as_str),
        Some("provider"),
        "claude provider should advertise provider kind"
    );
    let methods = init
        .pointer("/capabilities/methods")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("capabilities.methods missing"))?;
    let method_names: Vec<&str> = methods.iter().filter_map(Value::as_str).collect();
    for required in ["agent/run", "agent/cancel", "agent/resume"] {
        assert!(method_names.contains(&required), "claude provider should advertise {required}");
    }
    Ok(())
}

#[test]
fn plugin_info_completes_handshake_for_codex_provider() -> Result<()> {
    ensure_plugin_binary("animus-provider-codex")?;
    let response = run_plugin_command(&["plugin", "info", "--name", "animus-provider-codex"])?;
    let init = response.pointer("/data/initialize").context("initialize block missing")?;
    assert_eq!(
        init.pointer("/plugin_info/plugin_kind").and_then(Value::as_str),
        Some("provider"),
        "codex provider should advertise provider kind"
    );
    Ok(())
}

#[test]
fn plugin_ping_round_trips() -> Result<()> {
    ensure_plugin_binary("animus-plugin-smoke")?;
    let response = run_plugin_command(&["plugin", "ping", "--name", "animus-plugin-smoke"])?;
    assert_eq!(response.pointer("/data/ok").and_then(Value::as_bool), Some(true));
    Ok(())
}

#[test]
fn plugin_call_dispatches_smoke_subject_get() -> Result<()> {
    ensure_plugin_binary("animus-plugin-smoke")?;
    let response = run_plugin_command(&[
        "plugin",
        "call",
        "--name",
        "animus-plugin-smoke",
        "--method",
        "smoke/get",
        "--params",
        r#"{"id":"SMOKE-CONTRACT-1"}"#,
    ])?;
    let result = response.pointer("/data/result").context("result missing")?;
    assert_eq!(
        result.pointer("/id").and_then(Value::as_str),
        Some("SMOKE-CONTRACT-1"),
        "smoke plugin should echo the id back: {result}"
    );
    assert_eq!(
        result.pointer("/attributes/kind").and_then(Value::as_str),
        Some("smoke"),
        "smoke plugin should tag attributes.kind=smoke"
    );
    Ok(())
}

#[test]
fn plugin_info_for_unknown_plugin_returns_not_found() -> Result<()> {
    let plugin_path = plugin_path_env()?;
    let harness = CliHarness::new()?;
    let (payload, status) = harness.run_json_err_with_exit_and_env(
        &["plugin", "info", "--name", "animus-plugin-does-not-exist"],
        &[("ANIMUS_PLUGIN_PATH", plugin_path.as_str())],
    )?;
    assert_eq!(status, 3, "missing plugin should map to not_found exit code");
    assert_eq!(payload.pointer("/error/code").and_then(Value::as_str), Some("not_found"));
    Ok(())
}

#[test]
fn plugin_install_url_without_sha256_is_rejected() -> Result<()> {
    let harness = CliHarness::new()?;
    let (payload, status) = harness.run_json_err_with_exit(&[
        "plugin",
        "install",
        "--url",
        "https://example.invalid/animus-plugin-malicious",
    ])?;
    assert_eq!(status, 2, "missing --sha256 with --url should map to invalid_input exit code");
    assert_eq!(payload.pointer("/error/code").and_then(Value::as_str), Some("invalid_input"));
    let message = payload.pointer("/error/message").and_then(Value::as_str).unwrap_or_default();
    assert!(
        message.contains("--sha256") && message.contains("URL"),
        "error message should mention --sha256 and URL: {message}"
    );
    Ok(())
}
