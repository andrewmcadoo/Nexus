//! Event log writer with atomic appends and exclusive locking.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use serde::ser::Error as SerError;

use crate::error::NexusError;
use crate::types::RunEvent;

/// Append-only event log writer with exclusive file locking.
///
/// Events are written as JSONL (one JSON object per line).
/// Uses OS-level `O_APPEND` for atomic writes and `fs2` for exclusive locking.
pub struct EventLogWriter {
    writer: BufWriter<File>,
    event_seq: u64,
    path: PathBuf,
}

impl EventLogWriter {
    /// Opens log file for writing, creates if not exists.
    ///
    /// Acquires exclusive lock immediately (non-blocking).
    /// Scans existing file to determine the next event_seq.
    pub fn open(path: &Path) -> Result<Self, NexusError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| NexusError::IoError {
                    operation: "create directory".to_string(),
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }

        let file = Self::open_file(path)?;
        file.try_lock_exclusive()
            .map_err(|_| NexusError::EventLogLocked)?;

        let max_seq = Self::scan_max_event_seq(path)?;
        let next_seq = if max_seq == 0 { 1 } else { max_seq + 1 };

        Ok(Self {
            writer: BufWriter::new(file),
            event_seq: next_seq,
            path: path.to_path_buf(),
        })
    }

    /// Opens log file with correct options.
    #[cfg(unix)]
    fn open_file(path: &Path) -> Result<File, NexusError> {
        use std::os::unix::fs::OpenOptionsExt;

        OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| NexusError::IoError {
                operation: "open log file".to_string(),
                path: path.to_path_buf(),
                source: e,
            })
    }

    #[cfg(not(unix))]
    fn open_file(path: &Path) -> Result<File, NexusError> {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map_err(|e| NexusError::IoError {
                operation: "open log file".to_string(),
                path: path.to_path_buf(),
                source: e,
            })
    }

    /// Scans an existing JSONL file to find the maximum event_seq.
    fn scan_max_event_seq(path: &Path) -> Result<u64, NexusError> {
        let file = File::open(path).map_err(|e| NexusError::IoError {
            operation: "read log file".to_string(),
            path: path.to_path_buf(),
            source: e,
        })?;

        let reader = BufReader::new(file);
        let mut max_seq = 0u64;

        for line in reader.lines() {
            let line = line.map_err(|e| NexusError::IoError {
                operation: "read line".to_string(),
                path: path.to_path_buf(),
                source: e,
            })?;

            if line.trim().is_empty() {
                continue;
            }

            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Skipping corrupted line in event log: {}", e);
                    continue;
                }
            };
            if let Some(seq) = value.get("event_seq").and_then(|v| v.as_u64()) {
                max_seq = max_seq.max(seq);
            }
        }

        Ok(max_seq)
    }

    /// Appends an event to the log, assigning the next event_seq.
    ///
    /// Does NOT sync to disk (call `sync()` for durability).
    pub fn append(&mut self, event: &RunEvent) -> Result<(), NexusError> {
        let mut value = serde_json::to_value(event)?;
        let obj = match value.as_object_mut() {
            Some(obj) => obj,
            None => {
                return Err(NexusError::Serialization(serde_json::Error::custom(
                    "RunEvent did not serialize to a JSON object",
                )));
            }
        };
        obj.insert(
            "event_seq".to_string(),
            serde_json::Value::Number(self.event_seq.into()),
        );

        serde_json::to_writer(&mut self.writer, &value)?;
        self.writer
            .write_all(b"\n")
            .map_err(|e| NexusError::IoError {
                operation: "write newline".to_string(),
                path: self.path.clone(),
                source: e,
            })?;

        self.event_seq += 1;
        Ok(())
    }

    /// Flushes buffer and syncs data to disk.
    pub fn sync(&mut self) -> Result<(), NexusError> {
        self.writer.flush().map_err(|e| NexusError::IoError {
            operation: "flush buffer".to_string(),
            path: self.path.clone(),
            source: e,
        })?;

        self.writer
            .get_ref()
            .sync_data()
            .map_err(|e| NexusError::IoError {
                operation: "sync to disk".to_string(),
                path: self.path.clone(),
                source: e,
            })?;

        Ok(())
    }

    /// Returns the next event_seq that will be assigned.
    pub fn next_seq(&self) -> u64 {
        self.event_seq
    }
}

impl Drop for EventLogWriter {
    fn drop(&mut self) {
        // Flush buffer (ignore errors in drop)
        let _ = self.writer.flush();
        // Lock is released automatically when file handle is dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_writer_creates_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        let writer = EventLogWriter::open(&path).unwrap();
        drop(writer);

        assert!(path.exists());
    }

    #[test]
    fn test_writer_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("dirs").join("test.jsonl");

        let writer = EventLogWriter::open(&path).unwrap();
        drop(writer);

        assert!(path.exists());
    }

    #[test]
    fn test_writer_starts_at_seq_1() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        let writer = EventLogWriter::open(&path).unwrap();
        assert_eq!(writer.next_seq(), 1);
    }

    #[test]
    fn test_writer_appends_valid_jsonl() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        {
            let mut writer = EventLogWriter::open(&path).unwrap();
            let event = RunEvent::new("run_123", "run.started");
            writer.append(&event).unwrap();
            writer.sync().unwrap();
        }

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"run_id\":\"run_123\""));
        assert!(content.contains("\"event_seq\":1"));
        assert!(content.ends_with('\n'));
    }

    #[test]
    fn test_writer_increments_event_seq() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        {
            let mut writer = EventLogWriter::open(&path).unwrap();
            writer.append(&RunEvent::new("run_123", "event1")).unwrap();
            writer.append(&RunEvent::new("run_123", "event2")).unwrap();
            writer.sync().unwrap();
        }

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"event_seq\":1"));
        assert!(content.contains("\"event_seq\":2"));
    }

    #[test]
    fn test_writer_continues_seq_on_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        {
            let mut writer = EventLogWriter::open(&path).unwrap();
            writer.append(&RunEvent::new("run_123", "event1")).unwrap();
            writer.sync().unwrap();
        }

        {
            let writer = EventLogWriter::open(&path).unwrap();
            assert_eq!(writer.next_seq(), 2);
        }
    }

    #[test]
    fn test_writer_handles_empty_file_on_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        std::fs::write(&path, "").unwrap();

        let writer = EventLogWriter::open(&path).unwrap();
        assert_eq!(writer.next_seq(), 1);
    }

    #[test]
    fn test_writer_handles_corrupted_line_on_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        // Write valid line, corrupted line, valid line
        std::fs::write(
            &path,
            "{\"event_seq\":1}\n\
             not valid json\n\
             {\"event_seq\":3}\n",
        )
        .unwrap();

        let writer = EventLogWriter::open(&path).unwrap();
        assert_eq!(writer.next_seq(), 4); // Should continue from max (3) + 1
    }

    #[test]
    fn test_writer_handles_file_without_event_seq() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        std::fs::write(
            &path,
            "{\"v\":\"nexus/1\",\"run_id\":\"run_1\",\"type\":\"test\"}\n",
        )
        .unwrap();

        let writer = EventLogWriter::open(&path).unwrap();
        assert_eq!(writer.next_seq(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_writer_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        let writer = EventLogWriter::open(&path).unwrap();
        drop(writer);

        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "File should have 0600 permissions");
    }

    #[test]
    fn test_writer_lock_prevents_second_open() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        let _writer1 = EventLogWriter::open(&path).unwrap();

        let result = EventLogWriter::open(&path);
        assert!(matches!(result, Err(NexusError::EventLogLocked)));
    }

    #[test]
    fn test_writer_lock_released_on_drop() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        {
            let _writer = EventLogWriter::open(&path).unwrap();
        }

        let writer2 = EventLogWriter::open(&path);
        assert!(writer2.is_ok());
    }
}
