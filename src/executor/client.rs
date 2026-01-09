use crate::error::NexusError;
use bytes::Bytes;
use futures::{Stream, StreamExt};
use rand::Rng;
use reqwest::header::{CONTENT_TYPE, HeaderMap, RETRY_AFTER};
use reqwest::{Client, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::pin::Pin;
use std::time::Duration;
use tokio_retry::RetryIf;
use tokio_retry::strategy::ExponentialBackoff;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MAX_RETRIES: usize = 3;
const CHAT_COMPLETIONS_PATH: &str = "chat/completions";

const RETRY_BASE_MILLIS: u64 = 100;
const RETRY_MAX_SECS: u64 = 30;
const RETRY_FACTOR: u64 = 2;
const JITTER_DIVISOR: u128 = 2;

const REQUEST_TIMEOUT_SECS: u64 = 60;

const SSE_DELIMITER: &[u8] = b"\n\n";
const SSE_DATA_PREFIX: &str = "data:";
const SSE_DONE_SENTINEL: &str = "[DONE]";

pub struct CodexClient {
    client: Client,
    api_key: SecretString,
    base_url: String,
    max_retries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
    pub usage: Option<UsageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    pub content: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

impl CodexClient {
    pub fn new(api_key: SecretString) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|err| {
                log::error!("failed to build reqwest client with timeout: {err}");
                Client::new()
            });

        Self {
            client,
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        let trimmed = url.into().trim_end_matches('/').to_string();
        if trimmed.is_empty() {
            self.base_url = DEFAULT_BASE_URL.to_string();
        } else {
            self.base_url = trimmed;
        }
        self
    }

    pub fn with_max_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }

    pub async fn chat_completion_stream(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<impl Stream<Item = Result<ChatChunk, NexusError>>, NexusError> {
        request.stream = true;
        let response = self.send_with_retry(&request).await?;
        let bytes_stream = response.bytes_stream();

        let state = StreamState::new(bytes_stream);
        let stream = futures::stream::unfold(state, |mut state| async move {
            loop {
                if let Some(chunk) = state.pending.pop_front() {
                    return Some((Ok(chunk), state));
                }

                if state.done {
                    return None;
                }

                match state.stream.next().await {
                    Some(Ok(bytes)) => match state.consume_bytes(bytes) {
                        Ok(done) => {
                            state.done = done;
                        }
                        Err(err) => {
                            state.done = true;
                            return Some((Err(err), state));
                        }
                    },
                    Some(Err(err)) => {
                        state.done = true;
                        return Some((Err(map_stream_error(err)), state));
                    }
                    None => {
                        if !state.buffer.is_empty() {
                            let err = NexusError::StreamInterrupted {
                                message: "stream closed with incomplete event".to_string(),
                            };
                            state.done = true;
                            return Some((Err(err), state));
                        }
                        return None;
                    }
                }
            }
        });

        Ok(stream)
    }

    async fn send_with_retry(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<reqwest::Response, NexusError> {
        let strategy = build_retry_strategy(self.max_retries);
        RetryIf::spawn(
            strategy,
            || async { self.send_request(request).await },
            |err: &RetryableError| err.is_retryable(),
        )
        .await
        .map_err(RetryableError::into_nexus)
    }

    async fn send_request(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<reqwest::Response, RetryableError> {
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            CHAT_COMPLETIONS_PATH
        );
        let response = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .bearer_auth(self.api_key.expose_secret())
            .json(request)
            .send()
            .await
            .map_err(map_request_error)?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let retry_after = parse_retry_after(response.headers());
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(RetryableError::Retryable(NexusError::RateLimited {
                retry_after,
            }));
        }

        let body = response.text().await.map_err(|err| {
            let api_error = NexusError::ApiError {
                message: "failed to read error response body".to_string(),
                status_code: Some(status.as_u16()),
                source: Some(Box::new(err)),
            };
            classify_status_error(status, api_error)
        })?;

        let message = if body.is_empty() {
            format!("request failed with status {}", status)
        } else {
            body
        };
        let api_error = NexusError::ApiError {
            message,
            status_code: Some(status.as_u16()),
            source: None,
        };
        Err(classify_status_error(status, api_error))
    }
}

struct StreamState {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    buffer: Vec<u8>,
    pending: VecDeque<ChatChunk>,
    done: bool,
}

impl StreamState {
    fn new(stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static) -> Self {
        Self {
            stream: Box::pin(stream),
            buffer: Vec::new(),
            pending: VecDeque::new(),
            done: false,
        }
    }

    fn consume_bytes(&mut self, bytes: Bytes) -> Result<bool, NexusError> {
        self.buffer.extend_from_slice(&bytes);
        parse_sse_events(&mut self.buffer, &mut self.pending)
    }
}

enum StreamEvent {
    Chunk(ChatChunk),
    Done,
    Empty,
}

enum RetryableError {
    Retryable(NexusError),
    Fatal(NexusError),
}

impl RetryableError {
    fn is_retryable(&self) -> bool {
        matches!(self, RetryableError::Retryable(_))
    }

    fn into_nexus(self) -> NexusError {
        match self {
            RetryableError::Retryable(err) | RetryableError::Fatal(err) => err,
        }
    }
}

fn build_retry_strategy(max_retries: usize) -> impl Iterator<Item = Duration> {
    ExponentialBackoff::from_millis(RETRY_BASE_MILLIS)
        .factor(RETRY_FACTOR)
        .max_delay(Duration::from_secs(RETRY_MAX_SECS))
        .map(apply_jitter)
        .take(max_retries)
}

fn apply_jitter(duration: Duration) -> Duration {
    if duration.is_zero() {
        return duration;
    }
    let max_jitter = duration.as_millis().saturating_div(JITTER_DIVISOR);
    let max_jitter = u64::try_from(max_jitter).unwrap_or(u64::MAX);
    let jitter_ms = rand::thread_rng().gen_range(0..=max_jitter);
    duration + Duration::from_millis(jitter_ms)
}

fn map_request_error(err: reqwest::Error) -> RetryableError {
    if err.is_timeout() {
        return RetryableError::Retryable(NexusError::RequestTimeout {
            timeout_secs: REQUEST_TIMEOUT_SECS,
        });
    }

    if err.is_connect() {
        return RetryableError::Retryable(NexusError::ApiError {
            message: "connection error".to_string(),
            status_code: None,
            source: Some(Box::new(err)),
        });
    }

    RetryableError::Fatal(NexusError::ApiError {
        message: "request failed".to_string(),
        status_code: None,
        source: Some(Box::new(err)),
    })
}

fn classify_status_error(status: StatusCode, error: NexusError) -> RetryableError {
    if is_retryable_status(status) {
        RetryableError::Retryable(error)
    } else {
        RetryableError::Fatal(error)
    }
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
}

fn parse_retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn map_stream_error(err: reqwest::Error) -> NexusError {
    if err.is_timeout() {
        NexusError::RequestTimeout {
            timeout_secs: REQUEST_TIMEOUT_SECS,
        }
    } else {
        NexusError::StreamInterrupted {
            message: format!("stream error: {err}"),
        }
    }
}

fn parse_sse_events(
    buffer: &mut Vec<u8>,
    pending: &mut VecDeque<ChatChunk>,
) -> Result<bool, NexusError> {
    let mut done = false;
    loop {
        let Some(delimiter_index) = find_delimiter(buffer) else {
            break;
        };
        let event_bytes: Vec<u8> = buffer.drain(..delimiter_index).collect();
        buffer.drain(..SSE_DELIMITER.len());

        if event_bytes.is_empty() {
            continue;
        }

        let event_str =
            std::str::from_utf8(&event_bytes).map_err(|err| NexusError::StreamInterrupted {
                message: format!("invalid UTF-8 in SSE event: {err}"),
            })?;

        match parse_event(event_str)? {
            StreamEvent::Chunk(chunk) => pending.push_back(chunk),
            StreamEvent::Done => {
                done = true;
                break;
            }
            StreamEvent::Empty => {}
        }
    }
    Ok(done)
}

fn find_delimiter(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(SSE_DELIMITER.len())
        .position(|window| window == SSE_DELIMITER)
}

fn parse_event(event: &str) -> Result<StreamEvent, NexusError> {
    let mut data_lines = Vec::new();
    for line in event.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(payload) = line.strip_prefix(SSE_DATA_PREFIX) {
            let payload = payload.strip_prefix(' ').unwrap_or(payload);
            data_lines.push(payload);
        }
    }

    if data_lines.is_empty() {
        return Ok(StreamEvent::Empty);
    }

    let data = data_lines.join("\n");
    if data == SSE_DONE_SENTINEL {
        return Ok(StreamEvent::Done);
    }

    let chunk = serde_json::from_str(&data).map_err(|err| NexusError::StreamInterrupted {
        message: format!("failed to parse stream chunk: {err}"),
    })?;
    Ok(StreamEvent::Chunk(chunk))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_API_KEY: &str = "test-key";
    const CUSTOM_BASE_URL: &str = "https://example.com";
    const CUSTOM_BASE_URL_WITH_SLASH: &str = "https://example.com/";
    const CUSTOM_MAX_RETRIES: usize = 7;

    #[test]
    fn test_new_creates_client_with_defaults() {
        // Arrange
        let api_key = SecretString::from(TEST_API_KEY);

        // Act
        let client = CodexClient::new(api_key);

        // Assert
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
        assert_eq!(client.max_retries, DEFAULT_MAX_RETRIES);
    }

    #[test]
    fn test_with_base_url_overrides_default() {
        // Arrange
        let api_key = SecretString::from(TEST_API_KEY);

        // Act
        let client = CodexClient::new(api_key).with_base_url(CUSTOM_BASE_URL_WITH_SLASH);

        // Assert
        assert_eq!(client.base_url, CUSTOM_BASE_URL);
    }

    #[test]
    fn test_with_base_url_empty_uses_default() {
        // Arrange
        let api_key = SecretString::from(TEST_API_KEY);

        // Act
        let client = CodexClient::new(api_key)
            .with_base_url(CUSTOM_BASE_URL_WITH_SLASH)
            .with_base_url("");

        // Assert
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn test_with_max_retries_sets_value() {
        // Arrange
        let api_key = SecretString::from(TEST_API_KEY);

        // Act
        let client = CodexClient::new(api_key)
            .with_base_url(CUSTOM_BASE_URL_WITH_SLASH)
            .with_max_retries(CUSTOM_MAX_RETRIES);

        // Assert
        assert_eq!(client.base_url, CUSTOM_BASE_URL);
        assert_eq!(client.max_retries, CUSTOM_MAX_RETRIES);
    }
}
