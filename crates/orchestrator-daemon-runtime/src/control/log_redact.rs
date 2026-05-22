//! Log redaction layer applied before persistence.
//!
//! v0.4.10: the module moved into `orchestrator_logging` so the redactor
//! can be invoked directly from `Logger::write_entry` (every emit site
//! picks up redaction automatically). This file remains as a re-export
//! for backwards compatibility with the v0.4.9 module path
//! (`orchestrator_daemon_runtime::control::log_redact`).
//!
//! New code should import from `orchestrator_logging::log_redact` instead.

pub use orchestrator_logging::log_redact::{
    redact_log_entry, redact_string, REDACTED_PLACEHOLDER, REDACT_PATTERNS_ENV,
};
