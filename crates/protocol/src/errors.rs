use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error, Diagnostic)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProtocolError {
    #[error("Protocol version mismatch: expected {expected}, got {got}")]
    #[diagnostic(
        code(ao::protocol::version_mismatch),
        help("Ensure the client and server are running compatible versions of ao")
    )]
    VersionMismatch { expected: String, got: String },
    #[error("Invalid message format: {0}")]
    #[diagnostic(code(ao::protocol::invalid_message))]
    InvalidMessage(String),
    #[error("Serialization error: {0}")]
    #[diagnostic(code(ao::protocol::serialization))]
    Serialization(String),
}
