# Phase 1: Foundation Implementation Plan

**Version:** 1.0
**Date:** 2026-01-08
**Status:** Ready for Implementation
**Goal:** Types compile, CLI parses args, settings load

---

## 1. Overview

Phase 1 establishes the Rust CLI foundation for Nexus. Upon completion:

- `cargo build` compiles successfully with all dependencies
- `nexus --help` displays CLI help with proper argument parsing
- `nexus "task description"` accepts a refactoring task (stub execution)
- Settings load from `.nexus/settings.json` with sensible defaults
- Type definitions match JSON schemas exactly
- Error handling follows thiserror/anyhow patterns

**Not in Phase 1:** API calls, diff application, permission prompts, event logging.

---

## 2. Dependencies (Final Cargo.toml)

```toml
[package]
name = "nexus"
version = "0.1.0"
authors = ["AJ"]
categories = ["command-line-utilities", "development-tools"]
edition = "2024"
keywords = ["refactoring", "cli", "code-generation", "diff", "llm"]
license = "MIT OR Apache-2.0"
readme = "README.md"
repository = "https://github.com/yourusername/nexus"
rust-version = "1.85"
description = "Safe multi-file refactoring CLI for AI-assisted code transformations"

[dependencies]
# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# CLI
clap = { version = "4", features = ["derive", "env"] }

# Async runtime (minimal for current_thread)
tokio = { version = "1", features = ["rt", "macros", "signal"] }

# Error handling
thiserror = "2"
anyhow = "1"

# Date/time
chrono = { version = "0.4", features = ["serde"] }

# Logging
log = "0.4"
env_logger = "0.11"

# Environment files (optional)
dotenvy = "0.15"

# Secrets handling
secrecy = "0.10"

[dev-dependencies]
tempfile = "3"

[profile.release]
lto = true
strip = true
```

### Dependency Rationale

| Crate | Version | Purpose |
|-------|---------|---------|
| `serde` | 1 | Core serialization with derive macros |
| `serde_json` | 1 | JSON parsing for schemas and settings |
| `clap` | 4 | CLI argument parsing with derive style |
| `tokio` | 1 | Async runtime for future API calls |
| `thiserror` | 2 | Library error types with derive |
| `anyhow` | 1 | Application error handling with context |
| `chrono` | 0.4 | Timestamps for events |
| `log` | 0.4 | Logging facade |
| `env_logger` | 0.11 | Simple logger implementation |
| `dotenvy` | 0.15 | Load `.env` files |
| `secrecy` | 0.10 | Secure API key handling |
| `tempfile` | 3 | Temporary directories for tests |

---

## 3. Implementation Steps

### Step 1: Initialize Cargo Project

**Files Created:**
- `Cargo.toml`
- `src/main.rs` (generated, will be replaced)
- `src/lib.rs` (new)

**Commands:**
```bash
cd /Users/aj/Desktop/Projects/Nexus
cargo init --name nexus
```

**Post-init Actions:**
1. Replace generated `Cargo.toml` with full version from Section 2
2. Create license files: `LICENSE-MIT`, `LICENSE-APACHE`
3. Ensure `Cargo.lock` is committed (for reproducible builds)

---

### Step 2: Create Error Types

**File:** `src/error.rs`

```rust
use std::path::PathBuf;
use thiserror::Error;

/// Core error type for Nexus library operations.
#[derive(Error, Debug)]
pub enum NexusError {
    /// Action blocked by permission policy
    #[error("permission denied: {action}")]
    PermissionDenied {
        action: String,
        #[source]
        reason: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Patch failed to apply
    #[error("patch failed for {path}: {reason}")]
    PatchFailed {
        path: PathBuf,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Settings file parse or validation error
    #[error("configuration error: {message}")]
    ConfigError {
        message: String,
        path: Option<PathBuf>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Failed to load config file
    #[error("failed to load config from {}: {source}", path.display())]
    ConfigLoad {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse config file
    #[error("failed to parse config at {}: {message}", path.display())]
    ConfigParse {
        path: PathBuf,
        message: String,
    },

    /// Config validation failed
    #[error("invalid config at {}: {source}", path.display())]
    ConfigValidation {
        path: PathBuf,
        #[source]
        source: SettingsValidationError,
    },

    /// Codex API call failed
    #[error("API error: {message}")]
    ApiError {
        message: String,
        status_code: Option<u16>,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// File read/write operation failed
    #[error("I/O error: {operation} on {}", path.display())]
    IoError {
        operation: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Invalid input
    #[error("validation error: {message}")]
    ValidationError {
        message: String,
        field: Option<String>,
    },

    /// JSON serialization/deserialization failed
    #[error("JSON error: {context}")]
    JsonError {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    /// Path security violation
    #[error("path rejected: {path} - {reason}")]
    PathRejected {
        path: String,
        reason: String,
    },

    /// API key not configured
    #[error("OPENAI_API_KEY environment variable not set")]
    MissingApiKey,
}

/// Settings validation errors
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

/// Result type alias for Nexus operations
pub type NexusResult<T> = Result<T, NexusError>;

/// Exit codes following sysexits.h conventions
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

/// Convert anyhow::Error to exit code
pub fn exit_code_from_anyhow(err: &anyhow::Error) -> u8 {
    if let Some(nexus_err) = err.downcast_ref::<NexusError>() {
        return nexus_err.into();
    }
    if err.downcast_ref::<std::io::Error>().is_some() {
        return exit_codes::IOERR;
    }
    exit_codes::GENERAL_ERROR
}
```

---

### Step 3: Create Type Definitions

#### 3.1 Types Module Structure

**File:** `src/types/mod.rs`

```rust
pub mod action;
pub mod event;
pub mod settings;

pub use action::*;
pub use event::*;
pub use settings::*;
```

#### 3.2 Action Types

**File:** `src/types/action.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent role enumeration (shared across schemas)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Router,
    Researcher,
    Planner,
    Executor,
    Reviewer,
    Tool,
}

/// Creator information for actions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreatedBy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Approval group for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalGroup {
    pub id: String,
    pub label: String,
    pub size: u32,
    pub index: u32,
}

/// The main ProposedAction type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    pub id: String,
    pub summary: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,

    #[serde(default = "default_risk")]
    pub risk: u8,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_tags: Vec<String>,

    #[serde(default = "default_true")]
    pub requires_approval: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<CreatedBy>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_group: Option<ApprovalGroup>,

    pub kind: ActionKindTag,

    pub details: ActionDetails,
}

fn default_risk() -> u8 { 1 }
fn default_true() -> bool { true }

/// Action kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKindTag {
    Handoff,
    Patch,
    Command,
    PlanPatch,
    AgendaPatch,
    FileCreate,
    FileRename,
    FileDelete,
}

/// Action details (variant-specific data)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionDetails {
    Handoff(HandoffDetails),
    Patch(PatchDetails),
    Command(CommandDetails),
    PlanPatch(PlanPatchDetails),
    AgendaPatch(AgendaPatchDetails),
    FileCreate(FileCreateDetails),
    FileRename(FileRenameDetails),
    FileDelete(FileDeleteDetails),
}

// --- Patch Format Types ---

/// Patch format discriminator
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatchFormat {
    #[default]
    Unified,
    SearchReplace,
    WholeFile,
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OnConflict {
    #[default]
    Fail,
    Ours,
    Theirs,
    Marker,
}

/// Fallback matching strategy
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackStrategy {
    #[default]
    None,
    Fuzzy,
    LineAnchor,
}

/// Match mode for search/replace
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    #[default]
    Exact,
    WhitespaceInsensitive,
}

/// Search/replace block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchReplaceBlock {
    pub file: String,
    pub search: String,
    pub replace: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub match_mode: MatchMode,
}

/// Patch action details
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchDetails {
    #[serde(default)]
    pub format: PatchFormat,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_replace_blocks: Option<Vec<SearchReplaceBlock>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub whole_file_content: Option<HashMap<String, String>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_file_sha256: Option<HashMap<String, String>>,

    #[serde(default, skip_serializing_if = "is_default")]
    pub on_conflict: OnConflict,

    #[serde(default, skip_serializing_if = "is_default")]
    pub fallback_strategy: FallbackStrategy,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fuzzy_threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_confidence: Option<f64>,
}

// --- Other Action Details ---

/// Handoff action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffDetails {
    pub from: AgentRole,
    pub to: AgentRole,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_patch_ref: Option<String>,
}

/// Command action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDetails {
    pub argv: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_s: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_allow: Vec<String>,
    #[serde(default)]
    pub requires_network: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

fn default_timeout() -> u32 { 1200 }

/// Plan patch action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanPatchDetails {
    pub plan_id: String,
    pub patch_ref: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub patch_mode: PatchMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatchMode {
    #[default]
    Replace,
    JsonPatch,
}

/// Agenda patch action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaPatchDetails {
    pub target_path: String,
    pub diff: String,
}

/// File create action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCreateDetails {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub ignore_if_exists: bool,
}

/// File rename action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRenameDetails {
    pub old_path: String,
    pub new_path: String,
    #[serde(default)]
    pub overwrite: bool,
}

/// File delete action details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDeleteDetails {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub ignore_if_missing: bool,
}

/// Helper for skip_serializing_if on Default values
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_patch_details_defaults() {
        let details = PatchDetails::default();
        assert_eq!(details.format, PatchFormat::Unified);
        assert_eq!(details.on_conflict, OnConflict::Fail);
        assert_eq!(details.fallback_strategy, FallbackStrategy::None);
    }

    #[test]
    fn test_deserialize_patch_action() {
        let json = r#"{
            "id": "action-1",
            "kind": "patch",
            "summary": "Update function name",
            "details": {
                "format": "unified",
                "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,1 +1,1 @@\n-old\n+new"
            }
        }"#;

        let action: ProposedAction = serde_json::from_str(json).unwrap();
        assert_eq!(action.id, "action-1");
        assert_eq!(action.kind, ActionKindTag::Patch);
    }
}
```

#### 3.3 Settings Types

**File:** `src/types/settings.rs`

```rust
use serde::{Deserialize, Serialize};
use crate::error::SettingsValidationError;

/// Permission mode enumeration
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    #[default]
    Default,
    AcceptEdits,
    Autopilot,
}

/// Autopilot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutopilotConfig {
    #[serde(default = "default_max_batch_cu")]
    pub max_batch_cu: u32,

    #[serde(default = "default_max_batch_steps")]
    pub max_batch_steps: u32,

    #[serde(default)]
    pub auto_approve_patches: bool,

    #[serde(default)]
    pub auto_approve_tests: bool,

    #[serde(default)]
    pub auto_handoffs: bool,
}

impl Default for AutopilotConfig {
    fn default() -> Self {
        Self {
            max_batch_cu: default_max_batch_cu(),
            max_batch_steps: default_max_batch_steps(),
            auto_approve_patches: false,
            auto_approve_tests: false,
            auto_handoffs: false,
        }
    }
}

fn default_max_batch_cu() -> u32 { 40 }
fn default_max_batch_steps() -> u32 { 8 }

/// Nexus settings (matches settings.schema.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NexusSettings {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,

    #[serde(default)]
    pub permission_mode: PermissionMode,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_paths: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_paths_write: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_commands: Vec<Vec<String>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ask_commands: Vec<Vec<String>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_commands: Vec<Vec<String>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autopilot: Option<AutopilotConfig>,
}

fn default_schema_version() -> String { "1.0".to_string() }

impl Default for NexusSettings {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            permission_mode: PermissionMode::Default,
            deny_paths: vec![
                ".env*".to_string(),
                "**/.ssh/**".to_string(),
                "**/.aws/**".to_string(),
                "**/.npmrc".to_string(),
                "**/.pypirc".to_string(),
            ],
            allow_paths_write: vec![],
            allow_commands: vec![],
            ask_commands: vec![],
            deny_commands: vec![
                vec!["sudo".to_string()],
                vec!["rm".to_string()],
            ],
            autopilot: None,
        }
    }
}

impl NexusSettings {
    /// Validate settings after loading
    pub fn validate(&self) -> Result<(), SettingsValidationError> {
        // Schema version must be "1.0"
        if self.schema_version != "1.0" {
            return Err(SettingsValidationError::InvalidSchemaVersion(
                self.schema_version.clone()
            ));
        }

        // Validate path patterns
        for path in &self.deny_paths {
            validate_path_pattern(path)?;
        }
        for path in &self.allow_paths_write {
            validate_path_pattern(path)?;
        }

        // Validate autopilot ranges
        if let Some(ref autopilot) = self.autopilot {
            if autopilot.max_batch_cu < 1 {
                return Err(SettingsValidationError::InvalidMaxBatchCu(
                    autopilot.max_batch_cu
                ));
            }
            if autopilot.max_batch_steps < 1 {
                return Err(SettingsValidationError::InvalidMaxBatchSteps(
                    autopilot.max_batch_steps
                ));
            }
        }

        Ok(())
    }
}

fn validate_path_pattern(path: &str) -> Result<(), SettingsValidationError> {
    // No path traversal
    if path.contains("..") {
        return Err(SettingsValidationError::InvalidPathPattern {
            path: path.to_string(),
            reason: "path traversal (..) not allowed".to_string(),
        });
    }
    // No absolute paths (except glob patterns like /**/*)
    if path.starts_with('/') && !path.starts_with("/**/") {
        return Err(SettingsValidationError::InvalidPathPattern {
            path: path.to_string(),
            reason: "absolute paths not allowed in patterns".to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = NexusSettings::default();
        assert_eq!(settings.schema_version, "1.0");
        assert_eq!(settings.permission_mode, PermissionMode::Default);
        assert!(!settings.deny_paths.is_empty());
    }

    #[test]
    fn test_validate_valid_settings() {
        let settings = NexusSettings::default();
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_schema_version() {
        let mut settings = NexusSettings::default();
        settings.schema_version = "2.0".to_string();
        assert!(matches!(
            settings.validate(),
            Err(SettingsValidationError::InvalidSchemaVersion(_))
        ));
    }

    #[test]
    fn test_validate_path_traversal() {
        let mut settings = NexusSettings::default();
        settings.deny_paths.push("../etc/passwd".to_string());
        assert!(matches!(
            settings.validate(),
            Err(SettingsValidationError::InvalidPathPattern { .. })
        ));
    }
}
```

#### 3.4 Event Types

**File:** `src/types/event.rs`

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::AgentRole;

/// Trace information for correlation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
}

/// Actor information (who caused the event)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Actor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Payload reference for large/external data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadRef {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Run event (append-only log entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEvent {
    /// Schema version (e.g., "nexus/1")
    pub v: String,

    pub run_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,

    /// Event type (e.g., "action.proposed", "permission.granted")
    #[serde(rename = "type")]
    pub event_type: String,

    pub time: DateTime<Utc>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<TraceInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,

    /// Dynamic payload (additionalProperties: true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_ref: Option<PayloadRef>,
}

impl RunEvent {
    /// Create a new event with required fields
    pub fn new(run_id: impl Into<String>, event_type: impl Into<String>) -> Self {
        Self {
            v: "nexus/1".to_string(),
            run_id: run_id.into(),
            workflow_id: None,
            node_id: None,
            event_type: event_type.into(),
            time: Utc::now(),
            trace: None,
            actor: None,
            payload: None,
            payload_ref: None,
        }
    }

    /// Add payload to event
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Add actor to event
    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_event() {
        let event = RunEvent::new("run-123", "action.proposed");
        assert_eq!(event.v, "nexus/1");
        assert_eq!(event.run_id, "run-123");
        assert_eq!(event.event_type, "action.proposed");
    }

    #[test]
    fn test_serialize_event() {
        let event = RunEvent::new("run-123", "action.proposed");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"v\":\"nexus/1\""));
        assert!(json.contains("\"type\":\"action.proposed\""));
    }
}
```

---

### Step 4: Create Settings Loader

**File:** `src/settings.rs`

```rust
use crate::error::NexusError;
use crate::types::NexusSettings;
use log::{debug, info};
use secrecy::SecretString;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime configuration (settings + secrets from environment)
#[derive(Debug)]
pub struct NexusConfig {
    pub settings: NexusSettings,
    pub settings_path: Option<PathBuf>,
    api_key: Option<SecretString>,
}

impl NexusConfig {
    /// Load complete configuration from disk and environment
    pub fn load() -> Result<Self, NexusError> {
        let (settings, settings_path) = load_settings()?;
        let api_key = load_api_key();

        if api_key.is_none() {
            debug!("OPENAI_API_KEY not set; LLM operations will fail");
        }

        Ok(NexusConfig {
            settings,
            settings_path,
            api_key,
        })
    }

    /// Check if API key is available
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Get API key reference, error if not set
    pub fn require_api_key(&self) -> Result<&SecretString, NexusError> {
        self.api_key.as_ref().ok_or(NexusError::MissingApiKey)
    }

    /// Check if settings were loaded from a file
    pub fn has_settings_file(&self) -> bool {
        self.settings_path.is_some()
    }
}

/// Discover the settings file path in current directory
fn discover_settings_path() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let settings_path = cwd.join(".nexus").join("settings.json");

    if settings_path.exists() {
        Some(settings_path)
    } else {
        None
    }
}

/// Load settings, returning the path if loaded from file
fn load_settings() -> Result<(NexusSettings, Option<PathBuf>), NexusError> {
    match discover_settings_path() {
        Some(path) => {
            debug!("Loading settings from {:?}", path);
            let settings = load_from_file(&path)?;
            Ok((settings, Some(path)))
        }
        None => {
            info!("No .nexus/settings.json found, using defaults");
            Ok((NexusSettings::default(), None))
        }
    }
}

/// Load and validate settings from a specific file
fn load_from_file(path: &Path) -> Result<NexusSettings, NexusError> {
    let content = fs::read_to_string(path).map_err(|e| NexusError::ConfigLoad {
        path: path.to_path_buf(),
        source: e,
    })?;

    if content.trim().is_empty() {
        return Err(NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: "settings file is empty".to_string(),
        });
    }

    let mut settings: NexusSettings = serde_json::from_str(&content).map_err(|e| {
        NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: format!("JSON parse error at line {}, column {}: {}",
                e.line(), e.column(), e),
        }
    })?;

    merge_with_defaults(&mut settings);

    settings.validate().map_err(|e| NexusError::ConfigValidation {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(settings)
}

/// Ensure defaults are applied to missing optional fields
fn merge_with_defaults(settings: &mut NexusSettings) {
    let defaults = NexusSettings::default();

    if settings.deny_paths.is_empty() {
        settings.deny_paths = defaults.deny_paths;
    }

    if settings.deny_commands.is_empty() {
        settings.deny_commands = defaults.deny_commands;
    }
}

/// Load API key from environment
fn load_api_key() -> Option<SecretString> {
    env::var("OPENAI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(SecretString::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_from_env() {
        env::set_var("OPENAI_API_KEY", "sk-test-key");
        let key = load_api_key();
        assert!(key.is_some());
        env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn test_empty_api_key_ignored() {
        env::set_var("OPENAI_API_KEY", "");
        let key = load_api_key();
        assert!(key.is_none());
        env::remove_var("OPENAI_API_KEY");
    }
}
```

---

### Step 5: Create CLI Module

**File:** `src/cli.rs`

```rust
use clap::Parser;
use std::path::PathBuf;

/// Validate task is non-empty
fn validate_task(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Err("task description cannot be empty".into())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Validate config path
fn validate_config_path(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    // Allow non-existent paths (will use defaults)
    if !path.exists() {
        return Ok(path);
    }

    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() => Ok(path),
        Ok(_) => Err(format!("'{}' is not a file", s)),
        Err(e) => Err(format!("cannot access '{}': {}", s, e)),
    }
}

/// Safe multi-file refactoring CLI
///
/// Nexus takes a refactoring task description, uses Codex to propose
/// changes, prompts for approval, then applies patches with full
/// audit logging.
#[derive(Parser, Debug)]
#[command(name = "nexus")]
#[command(version, about)]
#[command(
    after_help = "Examples:\n  \
        nexus \"rename getUserData to fetchUserProfile\"\n  \
        nexus --dry-run \"extract validation logic\"\n  \
        nexus -v --config custom.json \"refactor task\""
)]
pub struct Cli {
    /// The refactoring task to execute
    ///
    /// Describe the refactoring in natural language. Be specific
    /// about what to rename, move, extract, or restructure.
    #[arg(value_name = "TASK", value_parser = validate_task)]
    pub task: String,

    /// Path to configuration file
    #[arg(
        short,
        long,
        value_name = "FILE",
        env = "NEXUS_CONFIG",
        default_value = ".nexus/settings.json",
        value_parser = validate_config_path,
    )]
    pub config: PathBuf,

    /// Preview changes without applying them
    ///
    /// Shows proposed patches and what would change, but doesn't
    /// modify any files. Use to review before committing.
    #[arg(long, env = "NEXUS_DRY_RUN")]
    pub dry_run: bool,

    /// Increase output verbosity
    ///
    /// Use -v for info, -vv for debug, -vvv for trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

impl Cli {
    /// Determine log level from verbosity count
    pub fn log_level(&self) -> &'static str {
        match self.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_basic_parse() {
        let cli = Cli::parse_from(["nexus", "rename foo to bar"]);
        assert_eq!(cli.task, "rename foo to bar");
        assert!(!cli.dry_run);
        assert_eq!(cli.verbose, 0);
    }

    #[test]
    fn test_all_flags() {
        let cli = Cli::parse_from([
            "nexus",
            "--dry-run",
            "-vvv",
            "--config",
            "custom.json",
            "my task",
        ]);
        assert!(cli.dry_run);
        assert_eq!(cli.verbose, 3);
        assert_eq!(cli.config, PathBuf::from("custom.json"));
    }

    #[test]
    fn test_log_level() {
        let cli = Cli::parse_from(["nexus", "task"]);
        assert_eq!(cli.log_level(), "warn");

        let cli = Cli::parse_from(["nexus", "-v", "task"]);
        assert_eq!(cli.log_level(), "info");

        let cli = Cli::parse_from(["nexus", "-vv", "task"]);
        assert_eq!(cli.log_level(), "debug");

        let cli = Cli::parse_from(["nexus", "-vvv", "task"]);
        assert_eq!(cli.log_level(), "trace");
    }
}
```

---

### Step 6: Create Library Root

**File:** `src/lib.rs`

```rust
pub mod cli;
pub mod error;
pub mod settings;
pub mod types;

pub use cli::Cli;
pub use error::{NexusError, NexusResult};
pub use settings::NexusConfig;
pub use types::*;
```

---

### Step 7: Create Main Entry Point

**File:** `src/main.rs`

```rust
use anyhow::{Context, Result};
use clap::Parser;
use std::process::ExitCode;

use nexus::cli::Cli;
use nexus::error::exit_code_from_anyhow;
use nexus::settings::NexusConfig;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("Error: {e:?}");
            ExitCode::from(exit_code_from_anyhow(&e))
        }
    }
}

fn run() -> Result<()> {
    // Load .env if present
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize logging
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(cli.log_level())
    ).init();

    log::info!("Task: {}", cli.task);

    // Load configuration
    let config = NexusConfig::load()
        .context("failed to load configuration")?;

    log::debug!("Config path: {:?}", config.settings_path);
    log::debug!("Permission mode: {:?}", config.settings.permission_mode);

    if cli.dry_run {
        println!("[DRY RUN] Would execute: {}", cli.task);
        println!("Settings loaded: {}", config.has_settings_file());
        println!("API key available: {}", config.has_api_key());
        return Ok(());
    }

    // TODO: Phase 2+ - Implement actual execution
    println!("Executing: {}", cli.task);
    println!("(Implementation pending - Phase 2+)");

    Ok(())
}
```

---

## 4. File Manifest

| File | Purpose |
|------|---------|
| `Cargo.toml` | Project metadata and dependencies |
| `LICENSE-MIT` | MIT license file |
| `LICENSE-APACHE` | Apache 2.0 license file |
| `src/main.rs` | CLI entry point with async runtime |
| `src/lib.rs` | Library root, re-exports modules |
| `src/cli.rs` | Clap-based CLI argument definitions |
| `src/error.rs` | NexusError enum and exit codes |
| `src/settings.rs` | Settings loader with validation |
| `src/types/mod.rs` | Type module exports |
| `src/types/action.rs` | ProposedAction and action detail types |
| `src/types/event.rs` | RunEvent and trace types |
| `src/types/settings.rs` | NexusSettings and validation |

---

## 5. Testing Strategy

### Unit Tests (Inline)

Each module includes `#[cfg(test)]` modules:

- `src/types/action.rs` - Serde round-trip tests
- `src/types/settings.rs` - Default values, validation rules
- `src/types/event.rs` - Event creation and serialization
- `src/cli.rs` - Argument parsing, flag combinations
- `src/settings.rs` - Config loading, API key handling

### Integration Tests

Create `tests/integration.rs`:

```rust
use nexus::types::{ProposedAction, NexusSettings, RunEvent};
use std::fs;

#[test]
fn test_deserialize_test_fixtures() {
    // Test against .nexus/test-fixtures/
    let fixture_path = ".nexus/test-fixtures/actions/valid-patch.json";
    if std::path::Path::new(fixture_path).exists() {
        let content = fs::read_to_string(fixture_path).unwrap();
        let action: ProposedAction = serde_json::from_str(&content).unwrap();
        assert!(!action.id.is_empty());
    }
}

#[test]
fn test_cli_help() {
    use std::process::Command;
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .output()
        .expect("Failed to run nexus --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Safe multi-file refactoring CLI"));
}
```

### Test Commands

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_default_settings

# Run integration tests only
cargo test --test integration
```

---

## 6. Verification Checklist

After completing all steps, verify Phase 1 is complete:

- [ ] `cargo build` succeeds with no warnings
- [ ] `cargo test` passes all tests
- [ ] `nexus --help` displays formatted help text
- [ ] `nexus --version` shows version from Cargo.toml
- [ ] `nexus "test task"` prints task and stub message
- [ ] `nexus --dry-run "test"` shows dry run output
- [ ] `nexus -vvv "test"` enables trace logging
- [ ] Without `.nexus/settings.json`: uses defaults silently
- [ ] With valid `.nexus/settings.json`: loads and validates
- [ ] With invalid settings: shows clear error message
- [ ] `OPENAI_API_KEY` detected when set
- [ ] Types serialize/deserialize matching JSON schemas

### Manual Verification Commands

```bash
# Build
cargo build

# Run tests
cargo test

# Check help
cargo run -- --help

# Check version
cargo run -- --version

# Basic execution
cargo run -- "test task"

# Dry run
cargo run -- --dry-run "test task"

# Verbose output
cargo run -- -vvv "test task"

# With API key
OPENAI_API_KEY=sk-test cargo run -- --dry-run "test task"
```

---

## 7. Implementation Order

Execute steps in this order to minimize rework:

1. **Step 1: cargo init** - Create project skeleton
2. **Step 2: error.rs** - Error types needed by all modules
3. **Step 3.1: types/mod.rs** - Module structure
4. **Step 3.3: types/settings.rs** - Settings types (needed by loader)
5. **Step 3.2: types/action.rs** - Action types
6. **Step 3.4: types/event.rs** - Event types
7. **Step 4: settings.rs** - Settings loader
8. **Step 5: cli.rs** - CLI argument parsing
9. **Step 6: lib.rs** - Library exports
10. **Step 7: main.rs** - Entry point

After each step: `cargo build` to catch errors early.

---

## 8. Dependencies on Other Phases

Phase 1 has **no dependencies** on other phases.

Phase 1 **enables**:
- Phase 2 (Event Log): Uses `RunEvent` type from `types/event.rs`
- Phase 3 (Permission Gate): Uses `NexusSettings` from settings loader
- Phase 4 (Tool Gateway): Uses `ProposedAction` and action detail types
- Phase 5 (Executor): Uses `NexusConfig` for API key access
- Phase 6 (Engine): Uses CLI, config, and all types

---

## 9. Notes from Research

### Rust Edition 2024
Using Edition 2024 (stable as of Rust 1.85.0). Key changes:
- Resolver v3 is default (MSRV-aware)
- Async closures supported
- New prelude additions

### Tokio Configuration
Using `current_thread` runtime with minimal features:
- `rt` - Current-thread runtime
- `macros` - `#[tokio::main]` attribute
- `signal` - Ctrl+C handling (Phase 2+)

File I/O uses `std::fs` directly (not async) since operations are fast.

### Serde Patterns
Action types use:
- `#[serde(rename_all = "snake_case")]` for enum variants
- `#[serde(default)]` paired with `#[serde(skip_serializing_if)]`
- `#[serde(untagged)]` for ActionDetails variants

### Error Handling
- `thiserror` for typed library errors
- `anyhow` with `.context()` at CLI boundary
- Exit codes follow sysexits.h conventions
