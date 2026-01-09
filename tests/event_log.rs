use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use nexus::NexusError;
use nexus::event_log::{EventLogReader, EventLogWriter, filter_by_run, filter_by_type, helpers};
use nexus::types::RunEvent;

fn temp_log_path() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("nested").join("events.jsonl");
    (dir, path)
}

fn write_content(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dirs");
    }
    fs::write(path, content).expect("write log content");
}

fn read_json_lines(path: &Path) -> Vec<serde_json::Value> {
    let file = File::open(path).expect("open log file");
    let reader = BufReader::new(file);
    let mut values = Vec::new();

    for line in reader.lines() {
        let line = line.expect("read log line");
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(&line).expect("parse json line");
        values.push(value);
    }

    values
}

fn assert_round_trip(expected: &RunEvent, actual: &RunEvent) {
    assert_eq!(actual.v, expected.v, "version mismatch");
    assert_eq!(actual.run_id, expected.run_id, "run_id mismatch");
    assert_eq!(
        actual.event_type, expected.event_type,
        "event_type mismatch"
    );
    assert_eq!(actual.time, expected.time, "time mismatch");
    assert_eq!(actual.payload, expected.payload, "payload mismatch");

    match (&expected.actor, &actual.actor) {
        (Some(expected_actor), Some(actual_actor)) => {
            assert_eq!(
                actual_actor.agent, expected_actor.agent,
                "actor.agent mismatch"
            );
            assert_eq!(
                actual_actor.provider, expected_actor.provider,
                "actor.provider mismatch"
            );
            assert_eq!(
                actual_actor.model, expected_actor.model,
                "actor.model mismatch"
            );
        }
        (None, None) => {}
        _ => panic!("actor presence mismatch"),
    }
}

#[test]
fn test_writer_creates_dirs_and_appends_jsonl() {
    let (_dir, path) = temp_log_path();

    let mut writer = EventLogWriter::open(&path).expect("open event log writer");
    let event = helpers::run_started("run_100", "do the thing");
    writer.append(&event).expect("append event");
    writer.sync().expect("sync event log");
    drop(writer);

    assert!(path.exists(), "expected log file to exist");

    let content = fs::read_to_string(&path).expect("read log file");
    assert!(content.ends_with('\n'), "expected trailing newline");

    let lines = read_json_lines(&path);
    assert_eq!(lines.len(), 1);

    let value = &lines[0];
    assert_eq!(
        value.get("run_id").and_then(|v| v.as_str()),
        Some("run_100")
    );
    assert_eq!(
        value.get("type").and_then(|v| v.as_str()),
        Some("run.started")
    );
    assert_eq!(value.get("event_seq").and_then(|v| v.as_u64()), Some(1));
}

#[test]
fn test_writer_event_seq_and_order() {
    let (_dir, path) = temp_log_path();

    let events = vec![
        helpers::run_started("run_order", "order test"),
        helpers::action_proposed("run_order", "act_1", "patch", "Update file", None),
        helpers::run_completed("run_order", "success", 1),
    ];

    let mut writer = EventLogWriter::open(&path).expect("open event log writer");
    for event in &events {
        writer.append(event).expect("append event");
    }
    writer.sync().expect("sync event log");
    drop(writer);

    let lines = read_json_lines(&path);
    assert_eq!(lines.len(), events.len());

    for (idx, value) in lines.iter().enumerate() {
        let seq = value
            .get("event_seq")
            .and_then(|v| v.as_u64())
            .expect("event_seq should be u64");
        assert_eq!(seq, (idx as u64) + 1);

        let event_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .expect("event type should be string");
        assert_eq!(event_type, events[idx].event_type);
    }
}

#[test]
fn test_reader_iter_reports_malformed_and_skips_empty_lines() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("events.jsonl");

    let content = concat!(
        r#"{"v":"nexus/1","run_id":"run_1","type":"run.started","time":"2026-01-08T12:00:00Z"}"#,
        "\n",
        "\n",
        "not json\n",
        r#"{"v":"nexus/1","run_id":"run_1","type":"run.completed","time":"2026-01-08T12:00:01Z"}"#,
        "\n"
    );
    write_content(&path, content);

    let mut reader = EventLogReader::open(&path).expect("open event log reader");
    let results: Vec<_> = reader.iter().collect();

    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(matches!(
        results[1],
        Err(NexusError::EventLogCorrupted { line: 3, .. })
    ));
    assert!(results[2].is_ok());
}

#[test]
fn test_reader_load_all_skips_corrupted_lines() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("events.jsonl");

    let content = concat!(
        r#"{"v":"nexus/1","run_id":"run_1","type":"run.started","time":"2026-01-08T12:00:00Z"}"#,
        "\n",
        "not json\n",
        r#"{"v":"nexus/1","run_id":"run_1","type":"run.completed","time":"2026-01-08T12:00:01Z"}"#,
        "\n"
    );
    write_content(&path, content);

    let mut reader = EventLogReader::open(&path).expect("open event log reader");
    let events = reader.load_all().expect("load all events");

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, "run.started");
    assert_eq!(events[1].event_type, "run.completed");
}

#[test]
fn test_reader_not_found() {
    let dir = TempDir::new().expect("create temp dir");
    let path = dir.path().join("missing.jsonl");

    let result = EventLogReader::open(&path);
    assert!(matches!(result, Err(NexusError::EventLogNotFound(_))));
}

#[test]
fn test_round_trip_with_helpers() {
    let (_dir, path) = temp_log_path();

    let events = vec![
        helpers::run_started("run_round", "test round trip"),
        helpers::action_proposed("run_round", "act_01", "patch", "Update file", None),
        helpers::permission_granted("run_round", "act_01", "once"),
        helpers::tool_executed(
            "run_round",
            "act_01",
            vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
        ),
        helpers::run_completed("run_round", "success", 2),
    ];

    let mut writer = EventLogWriter::open(&path).expect("open event log writer");
    for event in &events {
        writer.append(event).expect("append event");
    }
    writer.sync().expect("sync event log");
    drop(writer);

    let mut reader = EventLogReader::open(&path).expect("open event log reader");
    let loaded = reader.load_all().expect("load all events");

    assert_eq!(loaded.len(), events.len());
    for (expected, actual) in events.iter().zip(loaded.iter()) {
        assert_round_trip(expected, actual);
    }
}

#[test]
fn test_filter_by_run_and_type() {
    let (_dir, path) = temp_log_path();

    let events = vec![
        RunEvent::new("run_A", "run.started"),
        RunEvent::new("run_B", "run.started"),
        RunEvent::new("run_A", "run.completed"),
    ];

    let mut writer = EventLogWriter::open(&path).expect("open event log writer");
    for event in &events {
        writer.append(event).expect("append event");
    }
    writer.sync().expect("sync event log");
    drop(writer);

    let mut reader = EventLogReader::open(&path).expect("open event log reader");
    let filtered_run: Vec<_> = filter_by_run(reader.iter(), "run_A")
        .filter_map(|result| result.ok())
        .collect();

    assert_eq!(filtered_run.len(), 2);
    assert!(filtered_run.iter().all(|event| event.run_id == "run_A"));

    let mut reader = EventLogReader::open(&path).expect("open event log reader");
    let filtered_type: Vec<_> = filter_by_type(reader.iter(), "run.started")
        .filter_map(|result| result.ok())
        .collect();

    assert_eq!(filtered_type.len(), 2);
    assert!(
        filtered_type
            .iter()
            .all(|event| event.event_type == "run.started")
    );
}

#[test]
fn test_multiple_readers_allowed() {
    let (_dir, path) = temp_log_path();

    let mut writer = EventLogWriter::open(&path).expect("open event log writer");
    writer
        .append(&RunEvent::new("run_concurrent", "run.started"))
        .expect("append event");
    writer.sync().expect("sync event log");
    drop(writer);

    let _reader1 = EventLogReader::open(&path).expect("open first reader");
    let _reader2 = EventLogReader::open(&path).expect("open second reader");
}

#[test]
fn test_second_writer_blocked() {
    let (_dir, path) = temp_log_path();

    let _writer1 = EventLogWriter::open(&path).expect("open first writer");
    let result = EventLogWriter::open(&path);

    assert!(matches!(result, Err(NexusError::EventLogLocked)));
}

#[test]
fn test_fixture_sample_run() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixture_path = Path::new(manifest_dir)
        .join(".nexus")
        .join("test-fixtures")
        .join("events")
        .join("sample-run.jsonl");

    if !fixture_path.exists() {
        eprintln!("fixture missing at {}, skipping", fixture_path.display());
        return;
    }

    let mut reader = EventLogReader::open(&fixture_path).expect("open fixture log");
    let events = reader.load_all().expect("load fixture events");

    assert_eq!(events.len(), 5);

    let expected_types = [
        "run.started",
        "action.proposed",
        "permission.granted",
        "tool.executed",
        "run.completed",
    ];

    for (idx, event) in events.iter().enumerate() {
        assert_eq!(event.run_id, "run_001");
        assert_eq!(event.event_type, expected_types[idx]);
    }

    for window in events.windows(2) {
        assert!(
            window[0].time <= window[1].time,
            "expected non-decreasing event times"
        );
    }
}
