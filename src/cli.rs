use clap::Parser;
use std::path::PathBuf;

/// Validate task is non-empty.
fn validate_task(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        Err("task description cannot be empty".into())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Validate config path.
fn validate_config_path(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    // Allow non-existent paths (defaults will be used).
    if !path.exists() {
        return Ok(path);
    }

    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() => Ok(path),
        Ok(_) => Err(format!("'{}' is not a file", s)),
        Err(e) => Err(format!("cannot access '{}': {}", s, e)),
    }
}

/// Safe multi-file refactoring CLI.
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
    /// The refactoring task to execute.
    ///
    /// Describe the refactoring in natural language. Be specific
    /// about what to rename, move, extract, or restructure.
    #[arg(value_name = "TASK", value_parser = validate_task)]
    pub task: String,

    /// Path to configuration file.
    #[arg(
        short = 'c',
        long,
        value_name = "FILE",
        env = "NEXUS_CONFIG",
        default_value = ".nexus/settings.json",
        value_parser = validate_config_path,
    )]
    pub config: PathBuf,

    /// Preview changes without applying them.
    ///
    /// Shows proposed patches and what would change, but doesn't
    /// modify any files. Use to review before committing.
    #[arg(long, env = "NEXUS_DRY_RUN")]
    pub dry_run: bool,

    /// Increase output verbosity.
    ///
    /// Use -v for info, -vv for debug, -vvv for trace.
    #[arg(short, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

impl Cli {
    /// Determine log level from verbosity count.
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
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_clean_env<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let old_config = std::env::var("NEXUS_CONFIG").ok();
        let old_dry_run = std::env::var("NEXUS_DRY_RUN").ok();

        unsafe {
            std::env::remove_var("NEXUS_CONFIG");
            std::env::remove_var("NEXUS_DRY_RUN");
        }

        let result = f();

        unsafe {
            match old_config {
                Some(value) => std::env::set_var("NEXUS_CONFIG", value),
                None => std::env::remove_var("NEXUS_CONFIG"),
            }
            match old_dry_run {
                Some(value) => std::env::set_var("NEXUS_DRY_RUN", value),
                None => std::env::remove_var("NEXUS_DRY_RUN"),
            }
        }

        result
    }

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_basic_parse() {
        let cli = with_clean_env(|| Cli::parse_from(["nexus", "rename foo to bar"]));
        assert_eq!(cli.task, "rename foo to bar");
        assert!(!cli.dry_run);
        assert_eq!(cli.verbose, 0);
        assert_eq!(cli.config, PathBuf::from(".nexus/settings.json"));
    }

    #[test]
    fn test_all_flags() {
        let cli = with_clean_env(|| {
            Cli::parse_from([
                "nexus",
                "--dry-run",
                "-vvv",
                "--config",
                "custom.json",
                "my task",
            ])
        });
        assert!(cli.dry_run);
        assert_eq!(cli.verbose, 3);
        assert_eq!(cli.config, PathBuf::from("custom.json"));
    }

    #[test]
    fn test_log_level() {
        let cli = Cli {
            task: "task".to_string(),
            config: PathBuf::from(".nexus/settings.json"),
            dry_run: false,
            verbose: 0,
        };
        assert_eq!(cli.log_level(), "warn");

        let cli = Cli { verbose: 1, ..cli };
        assert_eq!(cli.log_level(), "info");

        let cli = Cli { verbose: 2, ..cli };
        assert_eq!(cli.log_level(), "debug");

        let cli = Cli { verbose: 3, ..cli };
        assert_eq!(cli.log_level(), "trace");
    }
}
