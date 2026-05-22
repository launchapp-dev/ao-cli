//! Error types for CLI wrapper

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("CLI not found: {0}")]
    CliNotFound(String),

    #[error("CLI execution failed: {0}")]
    ExecutionFailed(String),

    /// The plugin's `initialize` handshake did not advertise the capability
    /// the caller is trying to invoke (e.g. `agent/cancel` against a plugin
    /// whose `capabilities.cancellation` is `false`). Pattern-match on this
    /// variant instead of grepping `ExecutionFailed` strings when you want
    /// to short-circuit gracefully.
    #[error("plugin '{plugin}' does not advertise capability '{capability}'")]
    CapabilityNotSupported {
        /// Plugin name from the manifest / handshake.
        plugin: String,
        /// Capability that was missing (e.g. `"cancellation"`).
        capability: String,
    },

    #[error("CLI authentication required: {0}")]
    AuthenticationRequired(String),

    #[error("CLI output parsing failed: {0}")]
    ParsingFailed(String),

    #[error("CLI validation failed: {0}")]
    ValidationFailed(String),

    #[error("CLI test failed: {0}")]
    TestFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Toml error: {0}")]
    TomlError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::SerializationError(e.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::TomlError(e.to_string())
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self {
        Error::TomlError(e.to_string())
    }
}
