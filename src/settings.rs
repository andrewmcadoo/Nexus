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

    /// Load configuration honoring an explicit config path.
    ///
    /// If the path exists, it is loaded directly. If it does not exist,
    /// this falls back to defaults with `settings_path` set to `None`.
    /// Invalid JSON in an existing file still returns an error.
    pub fn load_with_config_path(config_path: &Path) -> Result<Self, NexusError> {
        let (settings, settings_path) = load_settings_with_preference(config_path)?;
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
    pub fn require_api_key(&self) -> Result<&SecretString, NexusError> {
        self.api_key.as_ref().ok_or(NexusError::MissingApiKey)
    }

    /// Indicates whether the active configuration was loaded from a settings file.
    pub fn has_settings_file(&self) -> bool {
        self.settings_path.is_some()
    }
}

/// Locate a settings.json file at ".nexus/settings.json" inside the current working directory.
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

/// Load settings preferring an explicit path; fall back to defaults if missing.
fn load_settings_with_preference(
    config_path: &Path,
) -> Result<(NexusSettings, Option<PathBuf>), NexusError> {
    if config_path.exists() {
        debug!("Loading settings from explicit path {:?}", config_path);
        let settings = load_from_file(config_path)?;
        return Ok((settings, Some(config_path.to_path_buf())));
    }

    debug!("Config file not found at {:?}, using defaults", config_path);
    Ok((NexusSettings::default(), None))
}

/// Load and validate settings from a specific file.
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

    let mut settings: NexusSettings =
        serde_json::from_str(&content).map_err(|err| NexusError::ConfigParse {
            path: path.to_path_buf(),
            message: format!(
                "JSON parse error at line {}, column {}: {}",
                err.line(),
                err.column(),
                err
            ),
        })?;

    merge_with_defaults(&mut settings);

    settings
        .validate()
        .map_err(|err| NexusError::ConfigValidation {
            path: path.to_path_buf(),
            source: err,
        })?;

    Ok(settings)
}

/// Apply default values for optional fields that are currently empty in `settings`.
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
