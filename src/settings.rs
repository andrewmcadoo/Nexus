use crate::error::NexusError;
use crate::types::NexusSettings;
use log::debug;
use secrecy::SecretString;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Runtime configuration (settings + secrets from environment).
#[derive(Debug)]
pub struct NexusConfig {
    pub settings: NexusSettings,
    pub settings_path: Option<PathBuf>,
    api_key: Option<SecretString>,
}

impl NexusConfig {
    /// Load the application's configuration from disk and environment.
    ///
    /// Attempts to discover and parse a settings file, merges defaults, validates the resulting
    /// settings, and reads the `OPENAI_API_KEY` environment variable (if present).
    ///
    /// # Returns
    ///
    /// `Ok(NexusConfig)` containing the loaded `settings`, optional `settings_path` (the file path
    /// used when a settings file was found), and optional `api_key`; `Err(NexusError)` if loading,
    /// parsing, or validation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// let config = NexusConfig::load().expect("failed to load nexus config");
    /// // Access settings: config.settings
    /// // Check for an API key: config.has_api_key()
    /// ```
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

    /// Check if API key is available.
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Return a reference to the configured API key.
    ///
    /// # Errors
    ///
    /// Returns `NexusError::MissingApiKey` if no API key is configured.
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = NexusConfig::load();
    /// let api_key = cfg.require_api_key().unwrap();
    /// assert!(!api_key.expose_secret().is_empty());
    /// ```
    pub fn require_api_key(&self) -> Result<&SecretString, NexusError> {
        self.api_key.as_ref().ok_or(NexusError::MissingApiKey)
    }

    /// Indicates whether the active configuration was loaded from a settings file.
    ///
    /// # Returns
    ///
    /// `true` if a settings file path was recorded when the configuration was loaded, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = NexusConfig::load().unwrap();
    /// let _was_from_file = cfg.has_settings_file();
    /// ```
    pub fn has_settings_file(&self) -> bool {
        self.settings_path.is_some()
    }
}

/// Locate a settings.json file at ".nexus/settings.json" inside the current working directory.
///
/// If the file exists, returns `Some(PathBuf)` pointing to it; otherwise returns `None`.
///
/// # Examples
///
/// ```
/// // Handle the optional settings path without assuming the file exists.
/// if let Some(path) = crate::discover_settings_path() {
///     println!("Found settings at: {}", path.display());
/// } else {
///     println!("No settings file discovered in the current directory.");
/// }
/// ```
fn discover_settings_path() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let settings_path = cwd.join(".nexus").join("settings.json");

    if settings_path.exists() {
        Some(settings_path)
    } else {
        None
    }
}

/// Load Nexus settings, optionally from a settings file in the current working directory.
///
/// If a settings file is discovered, it is read, parsed, merged with defaults, and validated;
/// otherwise the default `NexusSettings` is returned. The returned `Option<PathBuf>` is the
/// path of the file that was used, or `None` when defaults were applied.
///
/// # Examples
///
/// ```
/// let (settings, path) = load_settings().unwrap();
/// match path {
///     Some(p) => println!("Loaded settings from {:?}", p),
///     None => println!("Using default settings"),
/// }
/// ```
fn load_settings() -> Result<(NexusSettings, Option<PathBuf>), NexusError> {
    match discover_settings_path() {
        Some(path) => {
            debug!("Loading settings from {:?}", path);
            let settings = load_from_file(&path)?;
            Ok((settings, Some(path)))
        }
        None => Ok((NexusSettings::default(), None)),
    }
}

/// Load and validate Nexus settings from the given JSON file.
///
/// This reads the file at `path`, parses it as JSON into `NexusSettings`, applies default
/// values for missing optional fields, and runs settings validation before returning the result.
///
/// # Errors
///
/// Returns `NexusError::ConfigLoad` if the file cannot be read, `NexusError::ConfigParse` if the
/// file is empty or contains invalid JSON (including line/column information), or
/// `NexusError::ConfigValidation` if the parsed settings fail validation.
///
/// # Examples
///
/// ```
/// use std::fs;
/// use std::path::Path;
/// use tempfile::NamedTempFile;
///
/// let mut file = NamedTempFile::new().unwrap();
/// fs::write(file.path(), r#"{"some_setting": "value"}"#).unwrap();
/// let settings = crate::settings::load_from_file(file.path()).unwrap();
/// // use `settings` as needed; here we just assert it was loaded
/// assert!(settings.some_setting == "value");
/// ```
fn load_from_file(path: &Path) -> Result<NexusSettings, NexusError> {
    let content = fs::read_to_string(path).map_err(|err| NexusError::ConfigLoad {
        path: path.to_path_buf(),
        source: err,
    })?;

    if content.trim().is_empty() {
        return Err(NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: "settings file is empty".to_string(),
        });
    }

    let mut settings: NexusSettings = serde_json::from_str(&content).map_err(|err| {
        NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: format!(
                "JSON parse error at line {}, column {}: {}",
                err.line(),
                err.column(),
                err
            ),
        }
    })?;

    merge_with_defaults(&mut settings);

    settings.validate().map_err(|err| NexusError::ConfigValidation {
        path: path.to_path_buf(),
        source: err,
    })?;

    Ok(settings)
}

/// Apply default values for optional fields that are currently empty in `settings`.
///
/// This populates `deny_paths` and `deny_commands` from `NexusSettings::default()` when those
/// fields are empty, leaving any non-empty fields unchanged.
///
/// # Examples
///
/// ```
/// let mut s = NexusSettings::default();
/// s.deny_paths.clear();
/// s.deny_commands.clear();
///
/// merge_with_defaults(&mut s);
///
/// let defaults = NexusSettings::default();
/// assert_eq!(s.deny_paths, defaults.deny_paths);
/// assert_eq!(s.deny_commands, defaults.deny_commands);
/// ```
fn merge_with_defaults(settings: &mut NexusSettings) {
    let defaults = NexusSettings::default();

    if settings.deny_paths.is_empty() {
        settings.deny_paths = defaults.deny_paths;
    }

    if settings.deny_commands.is_empty() {
        settings.deny_commands = defaults.deny_commands;
    }
}

/// Load the OpenAI API key from the `OPENAI_API_KEY` environment variable.
///
/// Returns `Some(SecretString)` containing the API key if the `OPENAI_API_KEY` environment
/// variable is set to a non-empty value, `None` otherwise.
///
/// # Examples
///
/// ```
/// std::env::set_var("OPENAI_API_KEY", "sk-test-key");
/// let key = load_api_key();
/// assert!(key.is_some());
/// std::env::remove_var("OPENAI_API_KEY");
/// ```
fn load_api_key() -> Option<SecretString> {
    env::var("OPENAI_API_KEY")
        .ok()
        .filter(|value| !value.is_empty())
        .map(|value| SecretString::new(value.into_boxed_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_api_key_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("OPENAI_API_KEY", "sk-test-key");
        }
        let key = load_api_key();
        assert!(key.is_some());
        unsafe {
            env::remove_var("OPENAI_API_KEY");
        }
    }

    #[test]
    fn test_empty_api_key_ignored() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("OPENAI_API_KEY", "");
        }
        let key = load_api_key();
        assert!(key.is_none());
        unsafe {
            env::remove_var("OPENAI_API_KEY");
        }
    }
}