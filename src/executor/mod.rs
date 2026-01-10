use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod adapter;
pub mod client;
pub mod parser;
pub mod prompt;
pub mod streaming;

pub use adapter::CodexAdapter;
pub use client::{ChatChunk, ChatCompletionRequest, ChatMessage, UsageInfo};
pub use parser::ResponseParser;
pub use prompt::PromptBuilder;
pub use streaming::StreamHandler;

use crate::error::NexusError;
pub use crate::types::PatchFormat;
use crate::types::ProposedAction;

#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(
        &self,
        task: &str,
        files: Vec<FileContext>,
        options: ExecuteOptions,
    ) -> Result<Vec<ProposedAction>, NexusError>;

    async fn execute_streaming(
        &self,
        task: &str,
        files: Vec<FileContext>,
        options: ExecuteOptions,
        on_chunk: Box<dyn Fn(StreamChunk) + Send>,
    ) -> Result<Vec<ProposedAction>, NexusError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContext {
    pub path: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteOptions {
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub preferred_format: PatchFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamChunk {
    Text(String),
    Thinking(String),
    ActionStart { id: String, summary: String },
    ActionComplete(Box<ProposedAction>),
    Error(String),
    Done,
}
