use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NexusError {
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
    fn from(err: &NexusError) -> u8 {
        match err {
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
        }
    }
}

pub fn exit_code_from_anyhow(err: &anyhow::Error) -> u8 {
    if let Some(nexus_err) = err.downcast_ref::<NexusError>() {
        return nexus_err.into();
    }
    if err.downcast_ref::<std::io::Error>().is_some() {
        return exit_codes::IOERR;
    }
    exit_codes::GENERAL_ERROR
}
