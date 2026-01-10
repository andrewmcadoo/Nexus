use futures::{Stream, StreamExt};

use crate::error::NexusError;

use super::StreamChunk;
use super::client::{ChatChunk, UsageInfo};

const PRIMARY_CHOICE_INDEX: usize = 0;
const FINISH_REASON_STOP: &str = "stop";

pub struct StreamHandler;

impl StreamHandler {
    pub async fn accumulate<S>(stream: S) -> Result<(String, Option<UsageInfo>), NexusError>
    where
        S: Stream<Item = Result<ChatChunk, NexusError>> + Unpin,
    {
        Self::with_callback(stream, |_| {}).await
    }

    pub async fn with_callback<S, F>(
        mut stream: S,
        mut callback: F,
    ) -> Result<(String, Option<UsageInfo>), NexusError>
    where
        S: Stream<Item = Result<ChatChunk, NexusError>> + Unpin,
        F: FnMut(StreamChunk),
    {
        let mut content = String::new();
        let mut usage = None;

        while let Some(result) = stream.next().await {
            let chunk = result?;
            update_usage(&mut usage, &chunk);

            if let Some(choice) = chunk.choices.get(PRIMARY_CHOICE_INDEX) {
                if let Some(text) = choice.delta.content.as_ref() {
                    content.push_str(text);
                    callback(StreamChunk::Text(text.clone()));
                }

                if is_finish_stop(&choice.finish_reason) {
                    callback(StreamChunk::Done);
                }
            }
        }

        Ok((content, usage))
    }
}

fn update_usage(usage: &mut Option<UsageInfo>, chunk: &ChatChunk) {
    if let Some(chunk_usage) = chunk.usage.clone() {
        *usage = Some(chunk_usage);
    }
}

fn is_finish_stop(reason: &Option<String>) -> bool {
    matches!(reason.as_deref(), Some(FINISH_REASON_STOP))
}

#[cfg(test)]
mod tests {
    use super::StreamHandler;
    use crate::error::NexusError;
    use crate::executor::StreamChunk;
    use crate::executor::client::{ChatChunk, ChunkChoice, Delta};
    use futures::stream;
    use std::sync::{Arc, Mutex};

    const DEFAULT_ID: &str = "test-id";
    const DEFAULT_OBJECT: &str = "chat.completion.chunk";
    const DEFAULT_MODEL: &str = "test-model";
    const DEFAULT_CREATED: i64 = 1;
    const DEFAULT_CHOICE_INDEX: u32 = 0;

    fn mock_chunk(content: Option<String>, finish_reason: Option<String>) -> ChatChunk {
        ChatChunk {
            id: DEFAULT_ID.to_string(),
            object: DEFAULT_OBJECT.to_string(),
            created: DEFAULT_CREATED,
            model: DEFAULT_MODEL.to_string(),
            choices: vec![ChunkChoice {
                index: DEFAULT_CHOICE_INDEX,
                delta: Delta {
                    content,
                    role: None,
                },
                finish_reason,
            }],
            usage: None,
        }
    }

    #[tokio::test]
    async fn test_accumulate_empty_stream() {
        // Arrange
        let stream = stream::iter(Vec::<Result<ChatChunk, NexusError>>::new());

        // Act
        let result = StreamHandler::accumulate(stream).await;

        // Assert
        let (content, usage) = result.expect("accumulate should succeed");
        assert!(content.is_empty());
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn test_accumulate_single_chunk() {
        // Arrange
        let stream = stream::iter(vec![Ok(mock_chunk(Some("Hello".to_string()), None))]);

        // Act
        let result = StreamHandler::accumulate(stream).await;

        // Assert
        let (content, usage) = result.expect("accumulate should succeed");
        assert_eq!(content, "Hello");
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn test_accumulate_multiple_chunks() {
        // Arrange
        let stream = stream::iter(vec![
            Ok(mock_chunk(Some("Hello".to_string()), None)),
            Ok(mock_chunk(Some(" ".to_string()), None)),
            Ok(mock_chunk(Some("world".to_string()), None)),
        ]);

        // Act
        let result = StreamHandler::accumulate(stream).await;

        // Assert
        let (content, usage) = result.expect("accumulate should succeed");
        assert_eq!(content, "Hello world");
        assert!(usage.is_none());
    }

    #[derive(Debug, PartialEq)]
    enum ObservedChunk {
        Text(String),
        Done,
    }

    #[tokio::test]
    async fn test_with_callback_receives_all_chunks() {
        // Arrange
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_handle = Arc::clone(&observed);
        let stream = stream::iter(vec![
            Ok(mock_chunk(Some("Hello".to_string()), None)),
            Ok(mock_chunk(
                Some(" world".to_string()),
                Some(super::FINISH_REASON_STOP.to_string()),
            )),
        ]);

        // Act
        let result = StreamHandler::with_callback(stream, move |chunk| {
            let mut guard = observed_handle
                .lock()
                .expect("observed chunks lock should not be poisoned");
            match chunk {
                StreamChunk::Text(text) => guard.push(ObservedChunk::Text(text)),
                StreamChunk::Done => guard.push(ObservedChunk::Done),
                other => panic!("unexpected stream chunk: {:?}", other),
            }
        })
        .await;

        // Assert
        let (content, usage) = result.expect("with_callback should succeed");
        assert_eq!(content, "Hello world");
        assert!(usage.is_none());

        let guard = observed
            .lock()
            .expect("observed chunks lock should not be poisoned");
        assert_eq!(
            *guard,
            vec![
                ObservedChunk::Text("Hello".to_string()),
                ObservedChunk::Text(" world".to_string()),
                ObservedChunk::Done,
            ]
        );
    }
}
