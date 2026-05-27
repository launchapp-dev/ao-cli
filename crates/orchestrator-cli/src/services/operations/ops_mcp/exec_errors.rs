use super::CliExecutionResult;
use serde_json::{json, Value};

pub(super) fn extract_cli_success_data(stdout_json: Option<Value>) -> Value {
    stdout_json
        .map(|envelope| match envelope {
            Value::Object(mut map) => map.remove("data").unwrap_or(Value::Object(map)),
            other => other,
        })
        .unwrap_or(Value::Null)
}

/// Pull a normalized `error` value off the first available
/// `animus.cli.v1` envelope, preferring stderr over stdout.
///
/// Error envelopes are written to **stderr** per
/// `docs/reference/json-envelope.md`, so a CLI failure with a structured
/// envelope on stderr (and either nothing or a *success* envelope on stdout)
/// is the canonical shape we need to surface to MCP callers. The pre-fix
/// production path only checked `stdout_json`, which meant a properly-emitted
/// stderr envelope was silently discarded and callers saw `error: null` with
/// just the raw `stderr` blob as a fallback.
///
/// Falls back to `data` only when the envelope lacks an `error` field — that
/// matches what scripted callers historically saw when the underlying CLI
/// emitted a success-shaped envelope but exited non-zero (rare, but the
/// fallback exists so we don't lose information).
fn pick_envelope_error(result: &CliExecutionResult) -> Option<Value> {
    let envelope = result.stderr_json.as_ref().or(result.stdout_json.as_ref())?;
    envelope.get("error").cloned().or_else(|| envelope.get("data").cloned())
}

pub(super) fn build_tool_error_payload(tool_name: &str, result: &CliExecutionResult) -> Value {
    let mut payload = json!({ "tool": tool_name, "exit_code": result.exit_code });
    if let Some(error) = pick_envelope_error(result) {
        payload["error"] = error;
    }
    let stderr = result.stderr.trim().to_string();
    if !stderr.is_empty() {
        payload["stderr"] = json!(stderr);
    }
    payload
}

pub(super) fn batch_item_error_from_result(result: &CliExecutionResult) -> Value {
    let mut payload = json!({ "exit_code": result.exit_code });
    if let Some(error) = pick_envelope_error(result) {
        payload["error"] = error;
    }
    let stderr = result.stderr.trim().to_string();
    if !stderr.is_empty() {
        payload["stderr"] = json!(stderr);
    }
    payload
}

/// Test-only alias kept for the existing `ops_mcp::tests` coverage. The
/// previous test helper diverged from production by checking `stderr_json`
/// first — that behavior is now the production contract, so this is just a
/// forwarding shim. Inlined here (rather than deleted) so the existing tests
/// keep passing without churn while a new production-path test below proves
/// `build_tool_error_payload` actually reads stderr.
#[cfg(test)]
pub(super) fn build_cli_error_payload(tool_name: &str, result: &CliExecutionResult) -> Value {
    build_tool_error_payload(tool_name, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::CLI_SCHEMA_ID;

    fn failure_with_envelopes(stdout: Option<Value>, stderr: Option<Value>, stderr_text: &str) -> CliExecutionResult {
        CliExecutionResult {
            command: "animus".to_string(),
            args: vec!["--json".to_string()],
            requested_args: vec!["daemon".to_string(), "start".to_string()],
            project_root: "/tmp/project".to_string(),
            exit_code: 5,
            success: false,
            stdout: String::new(),
            stderr: stderr_text.to_string(),
            stdout_json: stdout,
            stderr_json: stderr,
        }
    }

    /// Production-path regression: `build_tool_error_payload` MUST read the
    /// `error` field off the stderr envelope when present. Before this fix
    /// the function only looked at `stdout_json`, so a properly-emitted error
    /// envelope on stderr was silently dropped and MCP callers saw a null
    /// error with just the raw stderr text. The test_only helper that did
    /// the right thing was named `build_cli_error_payload` and was wired up
    /// only behind `#[cfg(test)]`, so the green test suite hid the regression.
    #[test]
    fn build_tool_error_payload_prefers_stderr_envelope_over_stdout_envelope() {
        let result = failure_with_envelopes(
            Some(json!({"schema": CLI_SCHEMA_ID, "ok": false, "error": {"message": "stdout-error"}})),
            Some(json!({"schema": CLI_SCHEMA_ID, "ok": false, "error": {"message": "stderr-error"}})),
            "stderr body",
        );
        let payload = build_tool_error_payload("animus.daemon.start", &result);
        assert_eq!(
            payload.pointer("/error/message").and_then(Value::as_str),
            Some("stderr-error"),
            "production helper must surface the stderr envelope (canonical error channel per the v1 contract)"
        );
        assert_eq!(payload.get("exit_code").and_then(Value::as_i64), Some(5));
        assert_eq!(payload.get("stderr").and_then(Value::as_str), Some("stderr body"));
        assert_eq!(payload.get("tool").and_then(Value::as_str), Some("animus.daemon.start"));
    }

    /// Production-path fallback: when no stderr envelope exists, the stdout
    /// envelope should still be consulted so we don't lose information from
    /// CLIs that historically emitted a success-shaped envelope but exited
    /// non-zero.
    #[test]
    fn build_tool_error_payload_falls_back_to_stdout_envelope_when_stderr_json_missing() {
        let result = failure_with_envelopes(
            Some(json!({"schema": CLI_SCHEMA_ID, "ok": false, "error": {"message": "stdout-error"}})),
            None,
            "",
        );
        let payload = build_tool_error_payload("animus.task.get", &result);
        assert_eq!(payload.pointer("/error/message").and_then(Value::as_str), Some("stdout-error"));
    }

    /// Batch helper shares the same envelope-picking contract — make sure a
    /// stderr envelope from one of the per-item runs survives the round-trip
    /// into the batch outcome payload (no `tool` field for batch items, but
    /// the `error` and `exit_code` still need to come through).
    #[test]
    fn batch_item_error_from_result_prefers_stderr_envelope() {
        let result = failure_with_envelopes(
            None,
            Some(json!({"schema": CLI_SCHEMA_ID, "ok": false, "error": {"code": "not_found"}})),
            "missing",
        );
        let payload = batch_item_error_from_result(&result);
        assert_eq!(payload.pointer("/error/code").and_then(Value::as_str), Some("not_found"));
        assert_eq!(payload.get("stderr").and_then(Value::as_str), Some("missing"));
        assert_eq!(payload.get("exit_code").and_then(Value::as_i64), Some(5));
    }
}
