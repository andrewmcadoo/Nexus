//! Event log module for append-only JSONL logging.
//!
//! Provides EventLogWriter and EventLogReader for recording
//! and replaying run events.

pub mod helpers;
mod reader;
mod writer;

pub use helpers::*;
pub use reader::EventLogReader;
pub use reader::{filter_by_run, filter_by_type};
pub use writer::EventLogWriter;

use std::path::{Path, PathBuf};

use crate::NexusError;

/// Internal helper for managing event log file paths.
/// Not exposed in public API.
#[allow(dead_code)] // Will be used by CLI integration in Phase 3+
pub(crate) struct EventLogPath {
    base_dir: PathBuf,
}

#[allow(dead_code)] // Will be used by CLI integration in Phase 3+
impl EventLogPath {
    /// Creates new EventLogPath from project root.
    /// Logs stored in `.nexus/runs/`
    pub fn new(project_root: &Path) -> Self {
        Self {
            base_dir: project_root.join(".nexus").join("runs"),
        }
    }

    /// Returns path to log file for given run_id.
    /// Validates run_id to prevent path traversal attacks.
    pub fn for_run(&self, run_id: &str) -> Result<PathBuf, NexusError> {
        Self::validate_run_id(run_id)?;
        Ok(self.base_dir.join(format!("{}.jsonl", run_id)))
    }

    /// Creates the runs directory if it doesn't exist.
    pub fn ensure_dir(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.base_dir)
    }

    /// Validates run_id contains no path traversal characters.
    /// SECURITY: run_id is user-controlled, must validate before path construction.
    fn validate_run_id(run_id: &str) -> Result<(), NexusError> {
        if run_id.trim().is_empty() {
            return Err(NexusError::InvalidRunId("empty run_id".to_string()));
        }
        if run_id.contains('/') || run_id.contains('\\') || run_id.contains("..") {
            return Err(NexusError::InvalidRunId(format!(
                "run_id contains invalid characters: {}",
                run_id
            )));
        }
        if run_id.len() > 255 {
            return Err(NexusError::InvalidRunId(
                "run_id exceeds 255 characters".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_log_path_valid() {
        let path = EventLogPath::new(Path::new("/project"));
        let result = path.for_run("run_123");
        assert!(result.is_ok());
        let expected = Path::new("/project")
            .join(".nexus")
            .join("runs")
            .join("run_123.jsonl");
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_event_log_path_rejects_traversal() {
        let path = EventLogPath::new(Path::new("/project"));
        assert!(path.for_run("../etc/passwd").is_err());
        assert!(path.for_run("foo/bar").is_err());
        assert!(path.for_run("foo\\bar").is_err());
        assert!(path.for_run("..").is_err(), "double-dot should be rejected");
        assert!(path.for_run("foo..bar").is_err());
    }

    #[test]
    fn test_event_log_path_rejects_empty() {
        let path = EventLogPath::new(Path::new("/project"));
        assert!(path.for_run("").is_err());
        assert!(path.for_run("   ").is_err());
        assert!(path.for_run("\t\n").is_err());
    }

    #[test]
    fn test_event_log_path_rejects_overlong() {
        let path = EventLogPath::new(Path::new("/project"));
        let ok = "a".repeat(255);
        let too_long = "a".repeat(256);
        assert!(path.for_run(&ok).is_ok());
        assert!(path.for_run(&too_long).is_err());
    }
}
