use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolError {
    #[error("Protocol version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: String, got: String },
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Structured error variants returned by AI provider integrations.
///
/// Callers use this enum to drive retry and failover decisions without
/// inspecting raw error strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProviderError {
    #[error("rate limit exceeded")]
    RateLimit { retry_after_secs: Option<u64> },
    #[error("authentication failure: {message}")]
    AuthFailure { message: String },
    #[error("context overflow: {message}")]
    ContextOverflow { message: String },
    #[error("model unavailable: {message}")]
    ModelUnavailable { message: String },
    #[error("unknown provider error: {message}")]
    Unknown { message: String },
}

impl ProviderError {
    /// Returns `true` if the error may resolve on a subsequent attempt.
    ///
    /// `RateLimit` is always retryable. `Unknown` covers transient network
    /// and server-side errors that are safe to retry with a limited back-off.
    /// All other variants represent permanent failures that should not be
    /// retried.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ProviderError::RateLimit { .. } | ProviderError::Unknown { .. })
    }
}

/// Parse an OAI-compatible HTTP error response into a [`ProviderError`].
///
/// `status` is the HTTP status code; `body` is the raw response body, which
/// may contain JSON in the form `{"error": {"message": "..."}}`.
pub fn parse_oai_http_error(status: u16, body: &str) -> ProviderError {
    let msg = extract_json_error_message(body).unwrap_or_else(|| body.to_string());
    let msg_lower = msg.to_ascii_lowercase();
    match status {
        429 => ProviderError::RateLimit { retry_after_secs: None },
        401 | 403 => ProviderError::AuthFailure { message: msg },
        400 => {
            if msg_lower.contains("context_length_exceeded")
                || msg_lower.contains("maximum context length")
                || msg_lower.contains("context window")
                || msg_lower.contains("too long")
            {
                ProviderError::ContextOverflow { message: msg }
            } else {
                ProviderError::Unknown { message: format!("HTTP {status}: {msg}") }
            }
        }
        404 => {
            if msg_lower.contains("model") {
                ProviderError::ModelUnavailable { message: msg }
            } else {
                ProviderError::Unknown { message: format!("HTTP {status}: {msg}") }
            }
        }
        _ => ProviderError::Unknown { message: format!("HTTP {status}: {msg}") },
    }
}

/// Parse a Claude CLI error message string into a [`ProviderError`] variant.
///
/// The Claude CLI emits plain-text error descriptions in result events.  This
/// function inspects the message for well-known patterns and maps them to the
/// appropriate variant.  Unrecognised messages map to [`ProviderError::Unknown`]
/// without panicking (AC5).
pub fn parse_claude_error_message(message: &str) -> ProviderError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("rate limit") || lower.contains("rate-limit") || lower.contains("overloaded") {
        ProviderError::RateLimit { retry_after_secs: None }
    } else if lower.contains("context window")
        || lower.contains("context_length")
        || lower.contains("prompt is too long")
        || lower.contains("too many tokens")
    {
        ProviderError::ContextOverflow { message: message.to_string() }
    } else if lower.contains("authentication")
        || lower.contains("invalid api key")
        || lower.contains("unauthorized")
        || lower.contains("invalid x-api-key")
    {
        ProviderError::AuthFailure { message: message.to_string() }
    } else if lower.contains("model")
        && (lower.contains("not found")
            || lower.contains("unavailable")
            || lower.contains("deprecated")
            || lower.contains("does not exist"))
    {
        ProviderError::ModelUnavailable { message: message.to_string() }
    } else {
        ProviderError::Unknown { message: message.to_string() }
    }
}

fn extract_json_error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if let Some(msg) = value.pointer("/error/message").and_then(|v| v.as_str()) {
        return Some(msg.to_string());
    }
    value.get("message").and_then(|v| v.as_str()).map(|s| s.to_string())
}

#[cfg(test)]
mod provider_error_tests {
    use super::*;

    #[test]
    fn parse_oai_http_error_rate_limit() {
        let err = parse_oai_http_error(429, r#"{"error":{"message":"Too many requests"}}"#);
        assert!(matches!(err, ProviderError::RateLimit { .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn parse_oai_http_error_auth_failure_401() {
        let err = parse_oai_http_error(401, r#"{"error":{"message":"Invalid API key"}}"#);
        assert!(matches!(err, ProviderError::AuthFailure { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn parse_oai_http_error_auth_failure_403() {
        let err = parse_oai_http_error(403, "forbidden");
        assert!(matches!(err, ProviderError::AuthFailure { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn parse_oai_http_error_context_overflow() {
        let body = r#"{"error":{"message":"This model's maximum context length is 128000 tokens"}}"#;
        let err = parse_oai_http_error(400, body);
        assert!(matches!(err, ProviderError::ContextOverflow { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn parse_oai_http_error_model_unavailable() {
        let body = r#"{"error":{"message":"The model `gpt-5` does not exist"}}"#;
        let err = parse_oai_http_error(404, body);
        assert!(matches!(err, ProviderError::ModelUnavailable { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn parse_oai_http_error_unknown_for_server_error() {
        let err = parse_oai_http_error(503, "service unavailable");
        assert!(matches!(err, ProviderError::Unknown { .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn parse_oai_http_error_unknown_for_unrecognised_400() {
        let err = parse_oai_http_error(400, r#"{"error":{"message":"invalid request body"}}"#);
        assert!(matches!(err, ProviderError::Unknown { .. }));
    }

    #[test]
    fn parse_claude_error_message_rate_limit() {
        let err = parse_claude_error_message("Claude is rate limited, please try again later");
        assert!(matches!(err, ProviderError::RateLimit { .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn parse_claude_error_message_overloaded() {
        let err = parse_claude_error_message("Claude is currently overloaded");
        assert!(matches!(err, ProviderError::RateLimit { .. }));
    }

    #[test]
    fn parse_claude_error_message_context_overflow() {
        let err = parse_claude_error_message("prompt is too long: 250000 tokens exceed context window");
        assert!(matches!(err, ProviderError::ContextOverflow { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn parse_claude_error_message_auth_failure() {
        let err = parse_claude_error_message("Authentication error: invalid x-api-key");
        assert!(matches!(err, ProviderError::AuthFailure { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn parse_claude_error_message_model_unavailable() {
        let err = parse_claude_error_message("model claude-3-haiku-99 does not exist");
        assert!(matches!(err, ProviderError::ModelUnavailable { .. }));
        assert!(!err.is_retryable());
    }

    #[test]
    fn parse_claude_error_message_unknown_fallthrough() {
        let err = parse_claude_error_message("something completely unexpected happened");
        assert!(matches!(err, ProviderError::Unknown { .. }));
    }

    #[test]
    fn parse_oai_http_error_extracts_nested_json_message() {
        let body = r#"{"error":{"type":"invalid_request_error","message":"context_length_exceeded"}}"#;
        let err = parse_oai_http_error(400, body);
        assert!(matches!(err, ProviderError::ContextOverflow { .. }));
    }

    #[test]
    fn parse_oai_http_error_falls_back_to_raw_body() {
        let err = parse_oai_http_error(429, "plain text rate limit response");
        assert!(matches!(err, ProviderError::RateLimit { .. }));
    }
}
