use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use secrecy::SecretString;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use nexus::event_log::{EventLogReader, EventLogWriter};
use nexus::{
    ActionDetails, ActionKindTag, CodexAdapter, ExecuteOptions, Executor, NexusError, PatchFormat,
    ProposedAction, StreamChunk,
};

const API_PATH: &str = "/v1/chat/completions";
const FIXTURE_DIR: &str = "tests/fixtures/codex_responses";
const FIXTURE_UNIFIED_DIFF: &str = "unified_diff_single.txt";
const FIXTURE_SEARCH_REPLACE: &str = "search_replace.txt";
const TEST_API_KEY: &str = "test-key";
const TEST_TASK: &str = "Update lib";

const STATUS_OK: u16 = 200;
const STATUS_TOO_MANY_REQUESTS: u16 = 429;
const STATUS_UNAUTHORIZED: u16 = 401;
const STATUS_SERVER_ERROR: u16 = 500;
const RETRY_AFTER_SECONDS: u64 = 42;

const EXPECTED_ACTION_COUNT: usize = 1;
const EXPECTED_RUN_ID_COUNT: usize = 1;

fn fixture_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir).join(FIXTURE_DIR).join(name)
}

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).expect("read fixture")
}

fn adapter_for(server: &MockServer) -> CodexAdapter {
    let base_url = format!("{}/v1", server.uri());
    CodexAdapter::new(SecretString::from(TEST_API_KEY)).with_base_url(base_url)
}

fn execute_options(preferred_format: PatchFormat) -> ExecuteOptions {
    ExecuteOptions {
        dry_run: false,
        max_tokens: None,
        temperature: None,
        preferred_format,
    }
}

async fn mount_sse_response(server: &MockServer, body: String) {
    let response = ResponseTemplate::new(STATUS_OK).set_body_raw(body, "text/event-stream");
    Mock::given(method("POST"))
        .and(path(API_PATH))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn mount_status_response(server: &MockServer, status: u16, body: &str) {
    let response = ResponseTemplate::new(status).set_body_string(body);
    Mock::given(method("POST"))
        .and(path(API_PATH))
        .respond_with(response)
        .mount(server)
        .await;
}

fn assert_patch_format(action: &ProposedAction, expected: PatchFormat) {
    match &action.details {
        ActionDetails::Patch(details) => {
            assert_eq!(details.format, expected, "patch format mismatch")
        }
        _ => panic!("expected patch details"),
    }
}

#[tokio::test]
async fn test_executor_returns_actions_from_unified_diff() {
    // Arrange
    let server = MockServer::start().await;
    let body = load_fixture(FIXTURE_UNIFIED_DIFF);
    mount_sse_response(&server, body).await;
    let adapter = adapter_for(&server);
    let options = execute_options(PatchFormat::Unified);

    // Act
    let actions = adapter
        .execute(TEST_TASK, Vec::new(), options)
        .await
        .expect("execute");

    // Assert
    assert_eq!(actions.len(), EXPECTED_ACTION_COUNT);
    let action = &actions[0];
    assert_eq!(action.kind, ActionKindTag::Patch);
    assert_patch_format(action, PatchFormat::Unified);
}

#[tokio::test]
async fn test_executor_returns_actions_from_search_replace() {
    // Arrange
    let server = MockServer::start().await;
    let body = load_fixture(FIXTURE_SEARCH_REPLACE);
    mount_sse_response(&server, body).await;
    let adapter = adapter_for(&server);
    let options = execute_options(PatchFormat::SearchReplace);

    // Act
    let actions = adapter
        .execute(TEST_TASK, Vec::new(), options)
        .await
        .expect("execute");

    // Assert
    assert_eq!(actions.len(), EXPECTED_ACTION_COUNT);
    let action = &actions[0];
    assert_eq!(action.kind, ActionKindTag::Patch);
    assert_patch_format(action, PatchFormat::SearchReplace);
}

#[tokio::test]
async fn test_executor_handles_rate_limit() {
    // Arrange
    let server = MockServer::start().await;
    let response = ResponseTemplate::new(STATUS_TOO_MANY_REQUESTS)
        .insert_header("Retry-After", RETRY_AFTER_SECONDS.to_string());
    Mock::given(method("POST"))
        .and(path(API_PATH))
        .respond_with(response)
        .mount(&server)
        .await;
    let adapter = adapter_for(&server);
    let options = execute_options(PatchFormat::Unified);

    // Act
    let result = adapter.execute(TEST_TASK, Vec::new(), options).await;

    // Assert
    match result {
        Err(NexusError::RateLimited { retry_after }) => {
            assert_eq!(retry_after, Some(RETRY_AFTER_SECONDS))
        }
        Err(err) => panic!("expected rate limit error, got {err:?}"),
        Ok(_) => panic!("expected rate limit error, got ok"),
    }
}

#[tokio::test]
async fn test_executor_handles_unauthorized() {
    // Arrange
    let server = MockServer::start().await;
    mount_status_response(&server, STATUS_UNAUTHORIZED, "unauthorized").await;
    let adapter = adapter_for(&server);
    let options = execute_options(PatchFormat::Unified);

    // Act
    let result = adapter.execute(TEST_TASK, Vec::new(), options).await;

    // Assert
    match result {
        Err(NexusError::ApiError { status_code, .. }) => {
            assert_eq!(status_code, Some(STATUS_UNAUTHORIZED))
        }
        Err(err) => panic!("expected api error, got {err:?}"),
        Ok(_) => panic!("expected api error, got ok"),
    }
}

#[tokio::test]
async fn test_executor_handles_server_error() {
    // Arrange
    let server = MockServer::start().await;
    mount_status_response(&server, STATUS_SERVER_ERROR, "server error").await;
    let adapter = adapter_for(&server);
    let options = execute_options(PatchFormat::Unified);

    // Act
    let result = adapter.execute(TEST_TASK, Vec::new(), options).await;

    // Assert
    match result {
        Err(NexusError::ApiError { status_code, .. }) => {
            assert_eq!(status_code, Some(STATUS_SERVER_ERROR))
        }
        Err(err) => panic!("expected api error, got {err:?}"),
        Ok(_) => panic!("expected api error, got ok"),
    }
}

#[tokio::test]
async fn test_executor_streaming_receives_chunks() {
    // Arrange
    let server = MockServer::start().await;
    let body = load_fixture(FIXTURE_UNIFIED_DIFF);
    mount_sse_response(&server, body).await;
    let adapter = adapter_for(&server);
    let options = execute_options(PatchFormat::Unified);
    let observed: Arc<Mutex<Vec<StreamChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let observed_handle = Arc::clone(&observed);

    // Act
    let actions = adapter
        .execute_streaming(
            TEST_TASK,
            Vec::new(),
            options,
            Box::new(move |chunk| {
                let mut guard = observed_handle
                    .lock()
                    .expect("observed chunks lock should not be poisoned");
                guard.push(chunk);
            }),
        )
        .await
        .expect("execute streaming");

    // Assert
    assert_eq!(actions.len(), EXPECTED_ACTION_COUNT);
    let guard = observed
        .lock()
        .expect("observed chunks lock should not be poisoned");
    let has_text = guard
        .iter()
        .any(|chunk| matches!(chunk, StreamChunk::Text(_)));
    let has_done = guard.iter().any(|chunk| matches!(chunk, StreamChunk::Done));
    assert!(has_text, "expected text chunks");
    assert!(has_done, "expected done chunk");
}

#[tokio::test]
async fn test_executor_with_logging_emits_events() {
    // Arrange
    let server = MockServer::start().await;
    let body = load_fixture(FIXTURE_UNIFIED_DIFF);
    mount_sse_response(&server, body).await;
    let adapter = adapter_for(&server);
    let options = execute_options(PatchFormat::Unified);
    let dir = TempDir::new().expect("create temp dir");
    let log_path = dir.path().join("events.jsonl");
    let mut writer = EventLogWriter::open(&log_path).expect("open event log writer");

    // Act
    let actions = adapter
        .execute_with_logging(TEST_TASK, &[], options, &mut writer)
        .await
        .expect("execute with logging");

    // Assert
    assert_eq!(actions.len(), EXPECTED_ACTION_COUNT);
    drop(writer);

    let mut reader = EventLogReader::open(&log_path).expect("open event log reader");
    let events = reader.load_all().expect("load event log");
    assert!(!events.is_empty(), "expected logged events");

    let has_started = events
        .iter()
        .any(|event| event.event_type == "executor.started");
    let has_action = events
        .iter()
        .any(|event| event.event_type == "action.proposed");
    let has_completed = events
        .iter()
        .any(|event| event.event_type == "executor.completed");

    assert!(has_started, "expected executor.started event");
    assert!(has_action, "expected action.proposed event");
    assert!(has_completed, "expected executor.completed event");

    let run_ids: HashSet<String> = events.iter().map(|event| event.run_id.clone()).collect();
    assert_eq!(
        run_ids.len(),
        EXPECTED_RUN_ID_COUNT,
        "expected single run_id for events"
    );
}
