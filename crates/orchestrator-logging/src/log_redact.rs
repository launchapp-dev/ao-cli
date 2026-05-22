//! Log redaction applied before persistence.
//!
//! Scrubs secret-shaped values from [`LogEntry`] fields before the logger
//! writes a JSON line to disk. Patterns match keys such as `api_key`,
//! `apikey`, `password`, `token`, `secret`, and `authorization` followed by
//! `:` or `=` and a value. The value is replaced with `***REDACTED***`
//! while the key is preserved so operators still see the *shape* of what
//! was logged.
//!
//! Custom patterns can be added via the `ANIMUS_LOG_REDACT_PATTERNS`
//! environment variable — a comma-separated list of regex strings. Each
//! supplied pattern is OR'd into the default redaction set. Invalid
//! patterns are silently skipped (no panic during log write).
//!
//! v0.4.10: the redactor is wired into [`crate::Logger::write_entry`] so
//! every emit site picks up redaction automatically. The previous home
//! under `orchestrator_daemon_runtime::control::log_redact` re-exports
//! these symbols for backwards compatibility.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use crate::LogEntry;

/// Environment variable for additional redaction patterns
/// (comma-separated regex strings).
pub const REDACT_PATTERNS_ENV: &str = "ANIMUS_LOG_REDACT_PATTERNS";

/// Placeholder value substituted for redacted secrets.
pub const REDACTED_PLACEHOLDER: &str = "***REDACTED***";

const DEFAULT_PATTERN: &str = r"(?i)(api[_-]?key|password|token|secret|authorization)\s*[:=]\s*\S+";

fn default_regex() -> &'static Regex {
    static RX: OnceLock<Regex> = OnceLock::new();
    RX.get_or_init(|| Regex::new(DEFAULT_PATTERN).expect("default redaction pattern compiles"))
}

fn custom_regexes() -> &'static Vec<Regex> {
    static RXS: OnceLock<Vec<Regex>> = OnceLock::new();
    RXS.get_or_init(|| {
        std::env::var(REDACT_PATTERNS_ENV)
            .ok()
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .filter_map(|p| Regex::new(p).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
}

/// Redact a single string in place — replaces matched key=value pairs
/// with `<key>=***REDACTED***` (or `<key>: ***REDACTED***`).
pub fn redact_string(input: &str) -> String {
    let mut out = default_regex()
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let key = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!("{}=***REDACTED***", key)
        })
        .into_owned();
    for rx in custom_regexes() {
        out = rx.replace_all(&out, REDACTED_PLACEHOLDER).into_owned();
    }
    out
}

fn redact_json_value(value: &mut Value) {
    match value {
        Value::String(s) => {
            let red = redact_string(s);
            if red != *s {
                *s = red;
            }
        }
        Value::Array(arr) => {
            for v in arr {
                redact_json_value(v);
            }
        }
        Value::Object(map) => {
            for (_k, v) in map.iter_mut() {
                redact_json_value(v);
            }
        }
        _ => {}
    }
}

/// Apply redaction to a [`LogEntry`] in place. Scrubs the `msg` string
/// and any string values inside the `meta` JSON tree. Other typed fields
/// (workflow_id, model, etc.) are identifiers — not secret-bearing — so
/// we leave them alone to keep the hot path cheap.
pub fn redact_log_entry(entry: &mut LogEntry) {
    let new_msg = redact_string(&entry.msg);
    if new_msg != entry.msg {
        entry.msg = new_msg;
    }
    if let Some(meta) = entry.meta.as_mut() {
        redact_json_value(meta);
    }
    if let Some(content) = entry.content.as_mut() {
        let red = redact_string(content);
        if red != *content {
            *content = red;
        }
    }
    if let Some(err) = entry.error.as_mut() {
        let red = redact_string(err);
        if red != *err {
            *err = red;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Level;
    use serde_json::json;

    fn entry_with_msg(msg: &str) -> LogEntry {
        LogEntry {
            ts: "2026-05-22T00:00:00Z".to_string(),
            level: Level::Info,
            cat: "test".to_string(),
            msg: msg.to_string(),
            workflow_id: None,
            task_id: None,
            schedule_id: None,
            phase_id: None,
            model: None,
            tool: None,
            provider: None,
            run_id: None,
            session_id: None,
            turn: None,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            tool_calls: None,
            role: None,
            content: None,
            subject_id: None,
            status: None,
            from_status: None,
            to_status: None,
            branch: None,
            pr_number: None,
            mcp_tool: None,
            mcp_server: None,
            fallback_from: None,
            fallback_to: None,
            cost: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            meta: None,
        }
    }

    #[test]
    fn redacts_default_api_key_pattern() {
        let mut e = entry_with_msg("calling provider with api_key=sk_live_abc123");
        redact_log_entry(&mut e);
        assert!(e.msg.contains("***REDACTED***"));
        assert!(!e.msg.contains("sk_live_abc123"));
    }

    #[test]
    fn redacts_authorization_header() {
        let mut e = entry_with_msg("request authorization: Bearer xyz");
        redact_log_entry(&mut e);
        assert!(e.msg.contains("***REDACTED***"));
        assert!(!e.msg.contains("Bearer"));
    }

    #[test]
    fn redacts_nested_meta_json() {
        let mut e = entry_with_msg("ok");
        e.meta = Some(json!({
            "outer": {
                "headers": "authorization: Bearer secret123",
                "harmless": "value",
            },
            "list": [
                "password=hunter2",
                "fine"
            ]
        }));
        redact_log_entry(&mut e);
        let meta = e.meta.as_ref().unwrap();
        let nested = meta.get("outer").and_then(|v| v.get("headers")).and_then(|v| v.as_str()).unwrap();
        assert!(nested.contains("***REDACTED***"), "nested header value not redacted: {nested}");
        let list_first = meta.get("list").and_then(|v| v.get(0)).and_then(|v| v.as_str()).unwrap();
        assert!(list_first.contains("***REDACTED***"));
        let list_second = meta.get("list").and_then(|v| v.get(1)).and_then(|v| v.as_str()).unwrap();
        assert_eq!(list_second, "fine");
    }

    #[test]
    fn preserves_non_secret_fields() {
        let mut e = entry_with_msg("no secrets here, just a plain message");
        let original = e.msg.clone();
        redact_log_entry(&mut e);
        assert_eq!(e.msg, original);
    }

    #[test]
    fn redacts_token_in_content_and_error() {
        let mut e = entry_with_msg("ok");
        e.content = Some("payload: token=abc.def.ghi".to_string());
        e.error = Some("failed: secret: tophat".to_string());
        redact_log_entry(&mut e);
        assert!(e.content.as_ref().unwrap().contains("***REDACTED***"));
        assert!(e.error.as_ref().unwrap().contains("***REDACTED***"));
    }

    #[test]
    fn write_entry_redacts_msg_in_persisted_line() {
        // Wire-up regression: redact_log_entry must run *before* the JSON
        // line is appended to events.jsonl, not just before display.
        let tmp = tempfile::tempdir().expect("tempdir");
        let logger = crate::Logger::open(tmp.path(), "test.jsonl", Level::Info);
        logger.info("phase", "calling provider with api_key=sk_live_abc123").emit();
        let path = tmp.path().join("test.jsonl");
        let contents = std::fs::read_to_string(&path).expect("read log");
        assert!(contents.contains("***REDACTED***"), "persisted line not redacted: {contents}");
        assert!(!contents.contains("sk_live_abc123"), "secret leaked to disk: {contents}");
    }
}
