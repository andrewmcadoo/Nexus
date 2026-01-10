use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NexusError {
    #[error("invalid run_id: {0}")]
    InvalidRunId(String),

    #[error("event log is locked by another process")]
    EventLogLocked,

    #[error("event log not found: {}", .0.display())]
    EventLogNotFound(PathBuf),

    #[error("event log corrupted at line {line}: {message}")]
    EventLogCorrupted { line: usize, message: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("permission denied: {action}")]
    PermissionDenied {
        action: String,
        #[source]
        reason: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("patch failed for {path}: {reason}")]
    PatchFailed {
        path: PathBuf,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("configuration error: {message}")]
    ConfigError {
        message: String,
        path: Option<PathBuf>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("failed to load config from {}: {source}", path.display())]
    ConfigLoad {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config at {}: {message}", path.display())]
    ConfigParse { path: PathBuf, message: String },

    #[error("invalid config at {}: {source}", path.display())]
    ConfigValidation {
        path: PathBuf,
        #[source]
        source: SettingsValidationError,
    },

    #[error("API error: {message}")]
    ApiError {
        message: String,
        status_code: Option<u16>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("I/O error: {operation} on {}", path.display())]
    IoError {
        operation: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("validation error: {message}")]
    ValidationError {
        message: String,
        field: Option<String>,
    },

    #[error("JSON error: {context}")]
    JsonError {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("path rejected: {path} - {reason}")]
    PathRejected { path: String, reason: String },

    #[error("OPENAI_API_KEY environment variable not set")]
    MissingApiKey,

    #[error("request timeout after {timeout_secs}s")]
    RequestTimeout { timeout_secs: u64 },

    #[error("rate limited (retry after {retry_after:?}s)")]
    RateLimited { retry_after: Option<u64> },

    #[error("model not available: {model}")]
    ModelNotAvailable { model: String },

    #[error("response parsing failed: {context}")]
    ResponseParseFailed {
        context: String,
        raw_response: Option<String>,
    },

    #[error("stream interrupted: {message}")]
    StreamInterrupted { message: String },
}

#[derive(Error, Debug)]
pub enum SettingsValidationError {
    #[error("invalid schema version: expected '1.0', got '{0}'")]
    InvalidSchemaVersion(String),

    #[error("invalid permission mode: {0}")]
    InvalidPermissionMode(String),

    #[error("invalid path pattern '{path}': {reason}")]
    InvalidPathPattern { path: String, reason: String },

    #[error("max_batch_cu must be >= 1, got {0}")]
    InvalidMaxBatchCu(u32),

    #[error("max_batch_steps must be >= 1, got {0}")]
    InvalidMaxBatchSteps(u32),
}

pub type NexusResult<T> = Result<T, NexusError>;

pub mod exit_codes {
    pub const OK: u8 = 0;
    pub const GENERAL_ERROR: u8 = 1;
    pub const USAGE: u8 = 64;
    pub const DATAERR: u8 = 65;
    pub const NOINPUT: u8 = 66;
    pub const UNAVAILABLE: u8 = 69;
    pub const SOFTWARE: u8 = 70;
    pub const CANTCREAT: u8 = 73;
    pub const IOERR: u8 = 74;
    pub const NOPERM: u8 = 77;
    pub const CONFIG: u8 = 78;
}

impl From<&NexusError> for u8 {
    /// Maps a `NexusError` variant to the appropriate numeric process exit code.
    ///
    /// # Examples
    ///
    /// ```
    /// use nexus::{NexusError, exit_codes};
    ///
    /// let err = NexusError::MissingApiKey;
    /// let code = u8::from(&err);
    /// assert_eq!(code, exit_codes::CONFIG);
    /// ```
    fn from(err: &NexusError) -> u8 {
        match err {
            NexusError::InvalidRunId(_) => exit_codes::USAGE,
            NexusError::EventLogLocked => 75, // EX_TEMPFAIL
            NexusError::EventLogNotFound(_) => exit_codes::NOINPUT,
            NexusError::EventLogCorrupted { .. } => exit_codes::DATAERR,
            NexusError::Serialization(_) => exit_codes::DATAERR,
            NexusError::PermissionDenied { .. } => exit_codes::NOPERM,
            NexusError::PatchFailed { .. } => exit_codes::DATAERR,
            NexusError::ConfigError { .. } => exit_codes::CONFIG,
            NexusError::ConfigLoad { .. } => exit_codes::NOINPUT,
            NexusError::ConfigParse { .. } => exit_codes::CONFIG,
            NexusError::ConfigValidation { .. } => exit_codes::CONFIG,
            NexusError::ApiError { .. } => exit_codes::UNAVAILABLE,
            NexusError::IoError { operation, .. } => {
                if operation.contains("read") {
                    exit_codes::NOINPUT
                } else {
                    exit_codes::IOERR
                }
            }
            NexusError::ValidationError { .. } => exit_codes::DATAERR,
            NexusError::JsonError { .. } => exit_codes::DATAERR,
            NexusError::PathRejected { .. } => exit_codes::NOPERM,
            NexusError::MissingApiKey => exit_codes::CONFIG,
            NexusError::RequestTimeout { .. } => exit_codes::UNAVAILABLE,
            NexusError::RateLimited { .. } => exit_codes::UNAVAILABLE,
            NexusError::ModelNotAvailable { .. } => exit_codes::CONFIG,
            NexusError::ResponseParseFailed { .. } => exit_codes::DATAERR,
            NexusError::StreamInterrupted { .. } => exit_codes::IOERR,
        }
    }
}

/// Derives a process exit code from an `anyhow::Error`.
///
/// # Examples
///
/// ```
/// use nexus::{NexusError, exit_codes, exit_code_from_anyhow};
/// use anyhow::Error;
///
/// // NexusError -> CONFIG (MissingApiKey maps to CONFIG)
/// let ne = NexusError::MissingApiKey;
/// let err = Error::new(ne);
/// assert_eq!(exit_code_from_anyhow(&err), exit_codes::CONFIG);
///
/// // std::io::Error -> IOERR
/// let io_err = std::io::Error::new(std::io::ErrorKind::Other, "io");
/// let err = Error::new(io_err);
/// assert_eq!(exit_code_from_anyhow(&err), exit_codes::IOERR);
///
/// // Unknown error -> GENERAL_ERROR
/// let other = Error::msg("other");
/// assert_eq!(exit_code_from_anyhow(&other), exit_codes::GENERAL_ERROR);
/// ```
pub fn exit_code_from_anyhow(err: &anyhow::Error) -> u8 {
    if let Some(nexus_err) = err.downcast_ref::<NexusError>() {
        return nexus_err.into();
    }
    if err.downcast_ref::<std::io::Error>().is_some() {
        return exit_codes::IOERR;
    }
    exit_codes::GENERAL_ERROR
}
