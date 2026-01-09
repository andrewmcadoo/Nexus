# Phase 1: Settings Loader Design Patterns

**Research Date:** 2026-01-08
**Confidence:** High (multiple authoritative sources consulted)
**Scope:** Config file discovery, loading, merging, validation, and environment variable integration for Nexus CLI

---

## 1. Summary of Findings

This research addresses five key questions for implementing the Nexus settings loader:

| Question | Recommended Solution |
|----------|---------------------|
| Config file discovery | Project-local first (`.nexus/settings.json`), no home directory fallback for v0 |
| Default merging | `figment` crate with `Serialized::defaults()` + `merge()` pattern |
| Validation approach | Two-phase: serde for structure, custom `Validate` trait for business rules at load time |
| Missing file handling | Return `Default::default()` silently; never auto-create |
| Environment variable override | Single override: `OPENAI_API_KEY` per ADR-006 (not stored in config) |

**Key Insight:** Nexus settings are project-scoped (per-repo `.nexus/settings.json`), not user-global. The loader should be simple: check if file exists, load it, merge with defaults. No complex XDG chain needed for v0.

---

## 2. Config Discovery Algorithm

### 2.1 Nexus-Specific Discovery (v0)

For Nexus v0, config discovery is intentionally simple:

```
1. Start from current working directory
2. Look for `.nexus/settings.json` in cwd
3. If not found, use hardcoded defaults
4. Never create the file automatically
```

**Rationale:**
- Nexus is repo-centric; settings are per-project, not per-user
- Simplicity over flexibility for v0
- User can create `.nexus/settings.json` manually or via `nexus init`

### 2.2 Discovery Algorithm (Rust)

```rust
use std::path::{Path, PathBuf};
use std::env;

/// Resolve the settings file path for the current project.
/// Returns None if no .nexus directory exists in cwd.
pub fn discover_settings_path() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let settings_path = cwd.join(".nexus").join("settings.json");

    if settings_path.exists() {
        Some(settings_path)
    } else {
        None
    }
}

/// Alternative: Discover .nexus directory (may need creation)
pub fn discover_nexus_dir() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let nexus_dir = cwd.join(".nexus");

    if nexus_dir.is_dir() {
        Some(nexus_dir)
    } else {
        None
    }
}
```

### 2.3 Future Extension (v1+)

For future versions, consider XDG-compliant user defaults using the [`directories`](https://lib.rs/crates/directories) crate:

```rust
use directories::ProjectDirs;

/// Get user-level config path (future use)
pub fn user_config_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "nexus", "nexus")
        .map(|dirs| dirs.config_dir().join("defaults.json"))
}
```

The discovery chain would become:
1. `.nexus/settings.json` (project-local, highest priority)
2. `~/.config/nexus/defaults.json` (user defaults, lower priority)
3. Hardcoded defaults (lowest priority)

---

## 3. Default Merging Strategy

### 3.1 Recommended Approach: `figment` Crate

The [`figment`](https://docs.rs/figment/latest/figment/) crate provides hierarchical configuration with clean merge semantics:

```rust
use figment::{Figment, providers::{Format, Json, Serialized, Env}};
use serde::{Deserialize, Serialize};

/// Load settings with defaults merged from file
pub fn load_settings() -> Result<NexusSettings, figment::Error> {
    let figment = Figment::from(Serialized::defaults(NexusSettings::default()));

    // Only merge file if it exists
    let figment = if let Some(path) = discover_settings_path() {
        figment.merge(Json::file(path))
    } else {
        figment
    };

    // OPENAI_API_KEY is handled separately per ADR-006
    // No env var overrides for settings in v0

    figment.extract()
}
```

**Why `figment`:**
- Values from merged providers replace those from previous providers
- Supports optional file loading (graceful missing file handling)
- Type-safe extraction with detailed error messages
- Metadata tracking shows origin of each value (useful for debugging)

### 3.2 Alternative: Manual Merge with `serde_json`

For simpler dependencies, manual merging works:

```rust
use serde_json::Value;
use std::fs;

pub fn load_settings_manual() -> Result<NexusSettings, crate::error::NexusError> {
    let defaults = serde_json::to_value(NexusSettings::default())?;

    let loaded = match discover_settings_path() {
        Some(path) => {
            let content = fs::read_to_string(&path)
                .map_err(|e| NexusError::ConfigLoad {
                    path: path.clone(),
                    source: e
                })?;
            serde_json::from_str::<Value>(&content)
                .map_err(|e| NexusError::ConfigParse {
                    path: path.clone(),
                    source: e
                })?
        }
        None => Value::Object(serde_json::Map::new()),
    };

    let merged = merge_json(defaults, loaded);
    serde_json::from_value(merged).map_err(|e| e.into())
}

/// Deep merge two JSON values (loaded overwrites defaults)
fn merge_json(default: Value, loaded: Value) -> Value {
    match (default, loaded) {
        (Value::Object(mut def), Value::Object(load)) => {
            for (key, load_val) in load {
                let merged = match def.remove(&key) {
                    Some(def_val) => merge_json(def_val, load_val),
                    None => load_val,
                };
                def.insert(key, merged);
            }
            Value::Object(def)
        }
        // For non-objects, loaded value wins
        (_, loaded) => loaded,
    }
}
```

### 3.3 Merge Behavior Summary

| Field Type | Default Value | File Value | Result |
|------------|---------------|------------|--------|
| Scalar | `"default"` | `"custom"` | `"custom"` |
| Scalar | `"default"` | (missing) | `"default"` |
| Object | `{a: 1, b: 2}` | `{b: 3}` | `{a: 1, b: 3}` |
| Array | `[1, 2]` | `[3, 4]` | `[3, 4]` (replace, not concat) |

---

## 4. Validation Recommendations

### 4.1 What to Validate at Load Time

**Validate immediately (fail fast):**
1. **Schema version** - Must be `"1.0"` (const in schema)
2. **Permission mode** - Must be valid enum variant
3. **Path patterns** - Syntactically valid globs (no path traversal)
4. **Integer ranges** - `max_batch_cu >= 1`, `max_batch_steps >= 1`

**Defer validation (runtime):**
1. **Path existence** - Files in `deny_paths` need not exist
2. **Command availability** - Commands in `allow_commands` validated when executed
3. **Cross-field dependencies** - Complex business rules

### 4.2 Two-Phase Validation Pattern

From the [serde patterns research](/Users/aj/Desktop/Projects/Nexus/.nexus/research/phase1-serde-patterns.md), use two-phase validation:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsValidationError {
    #[error("Invalid schema version: expected '1.0', got '{0}'")]
    InvalidSchemaVersion(String),

    #[error("Invalid permission mode: {0}")]
    InvalidPermissionMode(String),

    #[error("Invalid path pattern '{path}': {reason}")]
    InvalidPathPattern { path: String, reason: String },

    #[error("max_batch_cu must be >= 1, got {0}")]
    InvalidMaxBatchCu(u32),

    #[error("max_batch_steps must be >= 1, got {0}")]
    InvalidMaxBatchSteps(u32),
}

impl NexusSettings {
    /// Validate settings after loading
    pub fn validate(&self) -> Result<(), SettingsValidationError> {
        // 1. Schema version
        if self.schema_version != "1.0" {
            return Err(SettingsValidationError::InvalidSchemaVersion(
                self.schema_version.clone()
            ));
        }

        // 2. Path patterns (no traversal)
        for path in &self.deny_paths {
            validate_path_pattern(path)?;
        }
        for path in &self.allow_paths_write {
            validate_path_pattern(path)?;
        }

        // 3. Autopilot ranges
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
    // No absolute paths (in the context of repo-relative patterns)
    if path.starts_with('/') && !path.starts_with("/**/") {
        return Err(SettingsValidationError::InvalidPathPattern {
            path: path.to_string(),
            reason: "absolute paths not allowed in patterns".to_string(),
        });
    }
    Ok(())
}
```

### 4.3 Validation Trait (Optional)

For consistency with other types, define a `Validate` trait:

```rust
pub trait Validate {
    type Error;
    fn validate(&self) -> Result<(), Self::Error>;
}

impl Validate for NexusSettings {
    type Error = SettingsValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // ... implementation above
    }
}
```

---

## 5. Missing File Handling

### 5.1 Recommended Behavior

| Scenario | Action | User Feedback |
|----------|--------|---------------|
| `.nexus/` dir missing | Use defaults | Silent (debug log only) |
| `.nexus/settings.json` missing | Use defaults | Silent (debug log only) |
| File exists but empty | Parse error | Error message |
| File exists but malformed JSON | Parse error | Error with line/column |
| File exists but invalid schema | Validation error | Error with specific field |

**Never auto-create** the settings file. Users should:
1. Run `nexus init` to create `.nexus/` structure
2. Or manually create `.nexus/settings.json`

### 5.2 Implementation

```rust
use log::{debug, info};

pub fn load_settings() -> Result<NexusSettings, NexusError> {
    match discover_settings_path() {
        Some(path) => {
            debug!("Loading settings from {:?}", path);
            load_from_file(&path)
        }
        None => {
            info!("No settings file found, using defaults");
            Ok(NexusSettings::default())
        }
    }
}

fn load_from_file(path: &Path) -> Result<NexusSettings, NexusError> {
    let content = fs::read_to_string(path)
        .map_err(|e| NexusError::ConfigLoad {
            path: path.to_path_buf(),
            source: e,
        })?;

    // Handle empty file gracefully
    if content.trim().is_empty() {
        return Err(NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: "settings file is empty".to_string(),
        });
    }

    let settings: NexusSettings = serde_json::from_str(&content)
        .map_err(|e| NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: format!("JSON parse error: {}", e),
        })?;

    // Validate after parsing
    settings.validate().map_err(|e| NexusError::ConfigValidation {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(settings)
}
```

---

## 6. Environment Variable Integration

### 6.1 ADR-006 Constraint

Per ADR-006, the OpenAI API key comes **only** from environment:
- `OPENAI_API_KEY` environment variable
- **NOT** stored in `.nexus/settings.json`
- **NOT** stored in any config file

This is a security requirement: API keys should never be committed to version control.

### 6.2 Implementation Pattern

Separate the API key from settings:

```rust
use std::env;
use secrecy::{ExposeSecret, SecretString};

/// Runtime configuration (settings + secrets)
pub struct NexusConfig {
    pub settings: NexusSettings,
    pub api_key: Option<SecretString>,
}

impl NexusConfig {
    pub fn load() -> Result<Self, NexusError> {
        let settings = load_settings()?;
        let api_key = load_api_key();

        Ok(NexusConfig { settings, api_key })
    }

    /// Check if API key is available (required for LLM operations)
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Get API key, returning error if not set
    pub fn require_api_key(&self) -> Result<&SecretString, NexusError> {
        self.api_key.as_ref().ok_or(NexusError::MissingApiKey)
    }
}

fn load_api_key() -> Option<SecretString> {
    env::var("OPENAI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(SecretString::new)
}
```

### 6.3 Using the `secrecy` Crate

The [`secrecy`](https://leapcell.io/blog/secure-configuration-and-secrets-management-in-rust-with-secrecy-and-environment-variables) crate provides:
- `SecretString` wrapper that zeros memory on drop
- `Debug` implementation that redacts the value
- Prevents accidental logging of sensitive data

```toml
[dependencies]
secrecy = "0.10"
```

### 6.4 Future Environment Overrides (v1+)

If needed later, allow env vars to override settings:

```rust
use figment::providers::Env;

pub fn load_settings_with_env() -> Result<NexusSettings, figment::Error> {
    Figment::from(Serialized::defaults(NexusSettings::default()))
        .merge(Json::file(".nexus/settings.json"))
        .merge(Env::prefixed("NEXUS_").split("__"))
        .extract()
}
```

This would allow `NEXUS_PERMISSION_MODE=autopilot` to override the file setting.

---

## 7. Complete Implementation Example

### 7.1 Dependencies

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
log = "0.4"
secrecy = "0.10"

# Optional: for advanced config merging
# figment = { version = "0.10", features = ["json"] }
```

### 7.2 Settings Types (from serde-patterns research)

```rust
// /Users/aj/Desktop/Projects/Nexus/src/types/settings.rs

use serde::{Deserialize, Serialize};

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
    /// Always "1.0" for this schema version
    #[serde(default = "default_schema_version")]
    pub schema_version: String,

    #[serde(default)]
    pub permission_mode: PermissionMode,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_paths: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_paths_write: Vec<String>,

    /// Commands as argv arrays
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
```

### 7.3 Settings Loader Module

```rust
// /Users/aj/Desktop/Projects/Nexus/src/settings.rs

use crate::error::NexusError;
use crate::types::settings::{NexusSettings, SettingsValidationError};
use log::{debug, info, warn};
use secrecy::SecretString;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime configuration (settings + secrets from environment)
#[derive(Debug)]
pub struct NexusConfig {
    pub settings: NexusSettings,
    /// Path where settings were loaded from (None = defaults)
    pub settings_path: Option<PathBuf>,
    /// API key from OPENAI_API_KEY env var (ADR-006)
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
    // Read file content
    let content = fs::read_to_string(path).map_err(|e| NexusError::ConfigLoad {
        path: path.to_path_buf(),
        source: e,
    })?;

    // Handle empty file
    if content.trim().is_empty() {
        return Err(NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: "settings file is empty".to_string(),
        });
    }

    // Parse JSON
    let mut settings: NexusSettings = serde_json::from_str(&content).map_err(|e| {
        NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: format!("JSON parse error at line {}, column {}: {}",
                e.line(), e.column(), e),
        }
    })?;

    // Merge with defaults for any missing optional fields
    merge_with_defaults(&mut settings);

    // Validate business rules
    settings.validate().map_err(|e| NexusError::ConfigValidation {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(settings)
}

/// Ensure defaults are applied to missing optional fields
fn merge_with_defaults(settings: &mut NexusSettings) {
    let defaults = NexusSettings::default();

    // Merge deny_paths with defaults if empty
    if settings.deny_paths.is_empty() {
        settings.deny_paths = defaults.deny_paths;
    }

    // Merge deny_commands with defaults if empty
    if settings.deny_commands.is_empty() {
        settings.deny_commands = defaults.deny_commands;
    }
}

/// Load API key from environment (ADR-006)
fn load_api_key() -> Option<SecretString> {
    env::var("OPENAI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(SecretString::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_load_defaults_when_no_file() {
        // Run from a temp dir with no .nexus
        let temp = TempDir::new().unwrap();
        env::set_current_dir(&temp).unwrap();

        let (settings, path) = load_settings().unwrap();

        assert!(path.is_none());
        assert_eq!(settings.schema_version, "1.0");
        assert_eq!(settings.permission_mode, PermissionMode::Default);
    }

    #[test]
    fn test_load_from_file() {
        let temp = TempDir::new().unwrap();
        let nexus_dir = temp.path().join(".nexus");
        fs::create_dir(&nexus_dir).unwrap();

        let settings_content = r#"{
            "schema_version": "1.0",
            "permission_mode": "acceptEdits",
            "allow_paths_write": ["src/**"]
        }"#;

        let settings_path = nexus_dir.join("settings.json");
        fs::write(&settings_path, settings_content).unwrap();

        env::set_current_dir(&temp).unwrap();

        let (settings, path) = load_settings().unwrap();

        assert!(path.is_some());
        assert_eq!(settings.permission_mode, PermissionMode::AcceptEdits);
        assert_eq!(settings.allow_paths_write, vec!["src/**"]);
        // Defaults should be merged in
        assert!(!settings.deny_paths.is_empty());
    }

    #[test]
    fn test_invalid_schema_version() {
        let temp = TempDir::new().unwrap();
        let nexus_dir = temp.path().join(".nexus");
        fs::create_dir(&nexus_dir).unwrap();

        let settings_content = r#"{
            "schema_version": "2.0",
            "permission_mode": "default"
        }"#;

        fs::write(nexus_dir.join("settings.json"), settings_content).unwrap();
        env::set_current_dir(&temp).unwrap();

        let result = load_settings();
        assert!(result.is_err());
    }

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

### 7.4 Error Types

```rust
// Add to /Users/aj/Desktop/Projects/Nexus/src/error.rs

use crate::types::settings::SettingsValidationError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NexusError {
    #[error("Failed to load config from {path}: {source}")]
    ConfigLoad {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse config at {path}: {message}")]
    ConfigParse {
        path: PathBuf,
        message: String,
    },

    #[error("Invalid config at {path}: {source}")]
    ConfigValidation {
        path: PathBuf,
        source: SettingsValidationError,
    },

    #[error("OPENAI_API_KEY environment variable not set")]
    MissingApiKey,

    // ... other error variants
}
```

---

## 8. Recommendations Summary

### 8.1 For v0 Implementation

1. **Keep it simple**: Single file discovery (`.nexus/settings.json` in cwd)
2. **Use serde directly**: No need for `figment` in v0; manual merge is sufficient
3. **Validate at load time**: Fail fast on invalid settings
4. **Never auto-create**: Settings file created only by explicit `nexus init`
5. **Separate secrets**: API key from `OPENAI_API_KEY` env var only

### 8.2 Crate Dependencies

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
log = "0.4"
secrecy = "0.10"

[dev-dependencies]
tempfile = "3"
```

### 8.3 Future Enhancements (v1+)

1. Add `directories` crate for XDG-compliant user defaults
2. Add `figment` for multi-source config merging
3. Support `NEXUS_*` env var overrides
4. Add config schema evolution/migration

---

## 9. Sources

- [Command Line Applications in Rust - Config Files](https://rust-cli.github.io/book/in-depth/config-files.html) - Official CLI book guidance
- [config-rs Crate](https://github.com/rust-cli/config-rs) - Layered configuration system
- [Figment Documentation](https://docs.rs/figment/latest/figment/) - Hierarchical configuration library
- [directories Crate](https://lib.rs/crates/directories) - Platform-specific standard directories
- [Building a Robust Configuration System in Rust](https://tore.dev/en/blog/rust-config-file) - Practical patterns for config loading
- [Secure Configuration and Secrets Management in Rust](https://leapcell.io/blog/secure-configuration-and-secrets-management-in-rust-with-secrecy-and-environment-variables) - secrecy crate patterns
- [dotenvy Documentation](https://docs.rs/dotenvy/latest/dotenvy/) - Environment variable loading
- [validator Crate](https://github.com/Keats/validator) - Struct validation
- [serde_valid Documentation](https://docs.rs/serde_valid/latest/serde_valid/) - JSON Schema-based validation

---

## 10. Next Steps

1. Create `src/settings.rs` with loader implementation
2. Add `SettingsValidationError` to `src/error.rs`
3. Write unit tests with `tempfile` crate
4. Add integration test loading from `.nexus/test-fixtures/`
5. Implement `nexus init` command to create `.nexus/settings.json`
