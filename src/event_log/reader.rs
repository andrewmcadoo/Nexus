//! Event log reader with streaming iteration and shared locking.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::error::NexusError;
use crate::types::RunEvent;

/// Event log reader with shared locking for concurrent access.
///
/// Reads JSONL files line by line, parsing each as a RunEvent.
/// Uses shared locks to allow multiple readers while blocking writers.
pub struct EventLogReader {
    reader: BufReader<File>,
    line_number: usize,
    path: PathBuf,
}

impl EventLogReader {
    /// Opens log file for reading with shared lock.
    ///
    /// Shared lock allows multiple readers, blocks if writer has exclusive lock.
    ///
    /// # Errors
    /// - `NexusError::EventLogNotFound` if file doesn't exist
    pub fn open(path: &Path) -> Result<Self, NexusError> {
        if !path.exists() {
            return Err(NexusError::EventLogNotFound(path.to_path_buf()));
        }

        let file = File::open(path).map_err(|e| NexusError::IoError {
            operation: "open log file".to_string(),
            path: path.to_path_buf(),
            source: e,
        })?;

        FileExt::lock_shared(&file).map_err(|e| NexusError::IoError {
            operation: "acquire shared lock".to_string(),
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(Self {
            reader: BufReader::new(file),
            line_number: 0,
            path: path.to_path_buf(),
        })
    }

    /// Returns an iterator over events, parsing each line.
    ///
    /// Malformed lines yield `Err`, caller decides to skip or abort.
    /// Empty lines are automatically skipped.
    pub fn iter(&mut self) -> EventIterator<'_> {
        EventIterator { reader: self }
    }

    /// Loads all events into memory (for resume/replay operations).
    ///
    /// Skips malformed lines with warning to stderr.
    /// Returns all successfully parsed events.
    pub fn load_all(&mut self) -> Result<Vec<RunEvent>, NexusError> {
        let mut events = Vec::new();

        for result in self.iter() {
            match result {
                Ok(event) => events.push(event),
                Err(e @ NexusError::EventLogCorrupted { .. }) => {
                    eprintln!("Warning: skipping malformed event: {}", e);
                }
                Err(e) => return Err(e),
            }
        }

        Ok(events)
    }

    /// Reads next line and parses as RunEvent.
    fn read_next(&mut self) -> Option<Result<RunEvent, NexusError>> {
        loop {
            let mut line = String::new();

            match self.reader.read_line(&mut line) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    self.line_number += 1;

                    if line.trim().is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<RunEvent>(&line) {
                        Ok(event) => return Some(Ok(event)),
                        Err(e) => {
                            return Some(Err(NexusError::EventLogCorrupted {
                                line: self.line_number,
                                message: e.to_string(),
                            }));
                        }
                    }
                }
                Err(e) => {
                    return Some(Err(NexusError::IoError {
                        operation: "read line".to_string(),
                        path: self.path.clone(),
                        source: e,
                    }));
                }
            }
        }
    }

    /// Returns the current line number (for error reporting).
    pub fn line_number(&self) -> usize {
        self.line_number
    }
}

/// Iterator over events in the log file.
pub struct EventIterator<'a> {
    reader: &'a mut EventLogReader,
}

impl Iterator for EventIterator<'_> {
    type Item = Result<RunEvent, NexusError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.reader.read_next()
    }
}

impl Drop for EventLogReader {
    fn drop(&mut self) {
        // Lock is released automatically when file handle is dropped
    }
}

/// Filter events by run_id.
pub fn filter_by_run<'a>(
    events: impl Iterator<Item = Result<RunEvent, NexusError>> + 'a,
    run_id: &'a str,
) -> impl Iterator<Item = Result<RunEvent, NexusError>> + 'a {
    events.filter(move |result| match result {
        Ok(event) => event.run_id == run_id,
        Err(_) => true,
    })
}

/// Filter events by event_type.
pub fn filter_by_type<'a>(
    events: impl Iterator<Item = Result<RunEvent, NexusError>> + 'a,
    event_type: &'a str,
) -> impl Iterator<Item = Result<RunEvent, NexusError>> + 'a {
    events.filter(move |result| match result {
        Ok(event) => event.event_type == event_type,
        Err(_) => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_file(dir: &TempDir, content: &str) -> PathBuf {
        let path = dir.path().join("test.jsonl");
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_reader_opens_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = create_test_file(&dir, "");

        let reader = EventLogReader::open(&path);
        assert!(reader.is_ok());
    }

    #[test]
    fn test_reader_not_found() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.jsonl");
        let result = EventLogReader::open(&path);

        assert!(matches!(result, Err(NexusError::EventLogNotFound(_))));
    }

    #[test]
    fn test_reader_iterates_events() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"v":"nexus/1","run_id":"run_123","type":"run.started","time":"2026-01-08T12:00:00Z"}
{"v":"nexus/1","run_id":"run_123","type":"run.completed","time":"2026-01-08T12:00:01Z"}
"#;
        let path = create_test_file(&dir, content);

        let mut reader = EventLogReader::open(&path).unwrap();
        let events: Vec<_> = reader.iter().collect();

        assert_eq!(events.len(), 2);
        assert!(events[0].is_ok());
        assert!(events[1].is_ok());
        assert_eq!(events[0].as_ref().unwrap().event_type, "run.started");
        assert_eq!(events[1].as_ref().unwrap().event_type, "run.completed");
    }

    #[test]
    fn test_reader_skips_empty_lines() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"v":"nexus/1","run_id":"run_123","type":"run.started","time":"2026-01-08T12:00:00Z"}

{"v":"nexus/1","run_id":"run_123","type":"run.completed","time":"2026-01-08T12:00:01Z"}
"#;
        let path = create_test_file(&dir, content);

        let mut reader = EventLogReader::open(&path).unwrap();
        let events: Vec<_> = reader.iter().filter_map(|r| r.ok()).collect();

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_reader_handles_malformed_lines() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"v":"nexus/1","run_id":"run_123","type":"run.started","time":"2026-01-08T12:00:00Z"}
not valid json
{"v":"nexus/1","run_id":"run_123","type":"run.completed","time":"2026-01-08T12:00:01Z"}
"#;
        let path = create_test_file(&dir, content);

        let mut reader = EventLogReader::open(&path).unwrap();
        let results: Vec<_> = reader.iter().collect();

        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(matches!(
            results[1],
            Err(NexusError::EventLogCorrupted { line: 2, .. })
        ));
        assert!(results[2].is_ok());
    }

    #[test]
    fn test_reader_load_all() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"v":"nexus/1","run_id":"run_123","type":"run.started","time":"2026-01-08T12:00:00Z"}
{"v":"nexus/1","run_id":"run_123","type":"run.completed","time":"2026-01-08T12:00:01Z"}
"#;
        let path = create_test_file(&dir, content);

        let mut reader = EventLogReader::open(&path).unwrap();
        let events = reader.load_all().unwrap();

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_filter_by_run() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"v":"nexus/1","run_id":"run_A","type":"run.started","time":"2026-01-08T12:00:00Z"}
{"v":"nexus/1","run_id":"run_B","type":"run.started","time":"2026-01-08T12:00:01Z"}
{"v":"nexus/1","run_id":"run_A","type":"run.completed","time":"2026-01-08T12:00:02Z"}
"#;
        let path = create_test_file(&dir, content);

        let mut reader = EventLogReader::open(&path).unwrap();
        let filtered: Vec<_> = filter_by_run(reader.iter(), "run_A")
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|e| e.run_id == "run_A"));
    }

    #[test]
    fn test_filter_by_type() {
        let dir = TempDir::new().unwrap();
        let content = r#"{"v":"nexus/1","run_id":"run_123","type":"run.started","time":"2026-01-08T12:00:00Z"}
{"v":"nexus/1","run_id":"run_123","type":"action.proposed","time":"2026-01-08T12:00:01Z"}
{"v":"nexus/1","run_id":"run_123","type":"run.completed","time":"2026-01-08T12:00:02Z"}
"#;
        let path = create_test_file(&dir, content);

        let mut reader = EventLogReader::open(&path).unwrap();
        let filtered: Vec<_> = filter_by_type(reader.iter(), "run.started")
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event_type, "run.started");
    }
}
