use serde::{Deserialize, Serialize};

use crate::error::SettingsValidationError;

/// Permission mode enumeration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    #[default]
    Default,
    AcceptEdits,
    Autopilot,
}

/// Autopilot configuration.
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
    /// Creates an AutopilotConfig populated with the library's default limits and disabled auto-actions.
    ///
    /// The default values are:
    /// - `max_batch_cu = 40`
    /// - `max_batch_steps = 8`
    /// - `auto_approve_patches = false`
    /// - `auto_approve_tests = false`
    /// - `auto_handoffs = false`
    ///
    /// # Examples
    ///
    /// ```
    /// use nexus::AutopilotConfig;
    ///
    /// let cfg = AutopilotConfig::default();
    /// assert_eq!(cfg.max_batch_cu, 40);
    /// assert_eq!(cfg.max_batch_steps, 8);
    /// assert!(!cfg.auto_approve_patches);
    /// assert!(!cfg.auto_approve_tests);
    /// assert!(!cfg.auto_handoffs);
    /// ```
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

/// Returns the default maximum compute units allowed per batch for autopilot (40).
fn default_max_batch_cu() -> u32 {
    40
}

/// Default maximum number of steps allowed per batch for autopilot (8).
fn default_max_batch_steps() -> u32 {
    8
}

/// Nexus settings (matches .nexus/schemas/settings.schema.json).
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

/// Returns the default schema version used by Nexus settings ("1.0").
fn default_schema_version() -> String {
    "1.0".to_string()
}

impl Default for NexusSettings {
    /// Creates a NexusSettings initialized with the module's canonical defaults.
    ///
    /// Defaults:
    /// - `schema_version` = "1.0"
    /// - `permission_mode` = `PermissionMode::Default`
    /// - `deny_paths` includes [".env*", "**/.ssh/**", "**/.aws/**", "**/.npmrc", "**/.pypirc"]
    /// - `deny_commands` includes `["sudo"]` and `["rm"]`
    /// - `autopilot` = `None`
    ///
    /// # Examples
    ///
    /// ```
    /// use nexus::{NexusSettings, PermissionMode};
    ///
    /// let s = NexusSettings::default();
    /// assert_eq!(s.schema_version, "1.0");
    /// assert!(s.deny_paths.contains(&".env*".to_string()));
    /// assert_eq!(s.permission_mode, PermissionMode::Default);
    /// assert!(s.autopilot.is_none());
    /// ```
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
            allow_paths_write: Vec::new(),
            allow_commands: Vec::new(),
            ask_commands: Vec::new(),
            deny_commands: vec![vec!["sudo".to_string()], vec!["rm".to_string()]],
            autopilot: None,
        }
    }
}

impl NexusSettings {
    /// Validate that the settings conform to the expected schema and constraints.
    ///
    /// This checks that the `schema_version` equals "1.0", validates each pattern in
    /// `deny_paths` and `allow_paths_write`, and verifies that any present `autopilot`
    /// configuration has `max_batch_cu` and `max_batch_steps` greater than or equal to 1.
    ///
    /// # Returns
    ///
    /// `Ok(())` if all validations pass; `Err(SettingsValidationError)` with the first
    /// encountered validation failure otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use nexus::NexusSettings;
    ///
    /// let settings = NexusSettings::default();
    /// assert!(settings.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<(), SettingsValidationError> {
        if self.schema_version != "1.0" {
            return Err(SettingsValidationError::InvalidSchemaVersion(
                self.schema_version.clone(),
            ));
        }

        for path in &self.deny_paths {
            validate_path_pattern(path)?;
        }
        for path in &self.allow_paths_write {
            validate_path_pattern(path)?;
        }

        if let Some(ref autopilot) = self.autopilot {
            if autopilot.max_batch_cu < 1 {
                return Err(SettingsValidationError::InvalidMaxBatchCu(
                    autopilot.max_batch_cu,
                ));
            }
            if autopilot.max_batch_steps < 1 {
                return Err(SettingsValidationError::InvalidMaxBatchSteps(
                    autopilot.max_batch_steps,
                ));
            }
        }

        Ok(())
    }
}

/// Validates a path glob pattern for Nexus settings.
///
/// Ensures the pattern does not contain path traversal (`..`), is not an absolute
/// path (except globs beginning with `"/**/"`), contains no control characters,
/// and does not use Windows-specific absolute path formats.
///
/// # Windows Path Handling
///
/// Rejects Windows drive letters (e.g., `C:\`) and UNC paths (e.g., `\\server\share`)
/// since glob patterns should be relative to the project root.
fn validate_path_pattern(path: &str) -> Result<(), SettingsValidationError> {
    if path.contains("..") {
        return Err(SettingsValidationError::InvalidPathPattern {
            path: path.to_string(),
            reason: "path traversal (..) not allowed".to_string(),
        });
    }

    // Unix absolute paths (except /**/globs)
    if path.starts_with('/') && !path.starts_with("/**/") {
        return Err(SettingsValidationError::InvalidPathPattern {
            path: path.to_string(),
            reason: "absolute paths not allowed in patterns".to_string(),
        });
    }

    // Windows drive letters (C:\, D:\, etc.)
    if path.len() >= 2 {
        let bytes = path.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Err(SettingsValidationError::InvalidPathPattern {
                path: path.to_string(),
                reason: "Windows drive paths not allowed in patterns".to_string(),
            });
        }
    }

    // Windows UNC paths (\\server\share)
    if path.starts_with("\\\\") {
        return Err(SettingsValidationError::InvalidPathPattern {
            path: path.to_string(),
            reason: "UNC paths not allowed in patterns".to_string(),
        });
    }

    // Control characters (using is_control() for comprehensive check including DEL)
    if path.chars().any(|ch| ch.is_control()) {
        return Err(SettingsValidationError::InvalidPathPattern {
            path: path.to_string(),
            reason: "control characters not allowed in patterns".to_string(),
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
        assert_eq!(
            settings.deny_paths,
            vec![
                ".env*".to_string(),
                "**/.ssh/**".to_string(),
                "**/.aws/**".to_string(),
                "**/.npmrc".to_string(),
                "**/.pypirc".to_string(),
            ]
        );
        assert!(settings.allow_paths_write.is_empty());
        assert!(settings.allow_commands.is_empty());
        assert!(settings.ask_commands.is_empty());
        assert_eq!(
            settings.deny_commands,
            vec![vec!["sudo".to_string()], vec!["rm".to_string()]]
        );
        assert!(settings.autopilot.is_none());
    }

    #[test]
    fn test_validate_valid_settings() {
        let settings = NexusSettings::default();
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_schema_version() {
        let settings = NexusSettings {
            schema_version: "2.0".to_string(),
            ..Default::default()
        };
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

    #[test]
    fn test_validate_absolute_path() {
        let mut settings = NexusSettings::default();
        settings.allow_paths_write.push("/etc/passwd".to_string());
        assert!(matches!(
            settings.validate(),
            Err(SettingsValidationError::InvalidPathPattern { .. })
        ));
    }

    #[test]
    fn test_validate_glob_absolute_allowed() {
        assert!(validate_path_pattern("/**/foo").is_ok());
    }

    #[test]
    fn test_validate_windows_drive_path() {
        let result = validate_path_pattern("C:\\Users\\test");
        assert!(matches!(
            result,
            Err(SettingsValidationError::InvalidPathPattern { reason, .. })
            if reason.contains("Windows drive")
        ));
    }

    #[test]
    fn test_validate_windows_unc_path() {
        let result = validate_path_pattern("\\\\server\\share\\file");
        assert!(matches!(
            result,
            Err(SettingsValidationError::InvalidPathPattern { reason, .. })
            if reason.contains("UNC")
        ));
    }

    #[test]
    fn test_validate_control_characters() {
        // Test DEL character (0x7F) which is_control() catches
        let result = validate_path_pattern("foo\x7Fbar");
        assert!(matches!(
            result,
            Err(SettingsValidationError::InvalidPathPattern { reason, .. })
            if reason.contains("control characters")
        ));
    }
}
