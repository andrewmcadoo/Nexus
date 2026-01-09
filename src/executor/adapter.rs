use async_trait::async_trait;
use chrono::Utc;
use secrecy::SecretString;
use std::time::Instant;

use super::client::{ChatCompletionRequest, ChatMessage as ClientChatMessage, CodexClient};
use super::parser::ResponseParser;
use super::prompt::{ChatMessage as PromptChatMessage, PromptBuilder};
use super::streaming::StreamHandler;
use super::{ExecuteOptions, Executor, FileContext, StreamChunk};
use crate::error::NexusError;
use crate::event_log::{EventLogWriter, helpers};
use crate::types::{ActionKindTag, ProposedAction};

const DEFAULT_MODEL: &str = "gpt-5.2-codex";
const RUN_ID_PREFIX: &str = "run_";
const RUN_ID_TIME_FORMAT: &str = "%Y%m%d_%H%M%S";
const RUN_ID_MILLIS_WIDTH: usize = 3;

pub struct CodexAdapter {
    client: CodexClient,
    parser: ResponseParser,
    prompt_builder: PromptBuilder,
    model: String,
}

impl CodexAdapter {
    pub fn new(api_key: SecretString) -> Self {
        Self {
            client: CodexClient::new(api_key),
            parser: ResponseParser::new(),
            prompt_builder: PromptBuilder::new(),
            model: DEFAULT_MODEL.to_string(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        let trimmed = model.into().trim().to_string();
        if trimmed.is_empty() {
            self.model = DEFAULT_MODEL.to_string();
        } else {
            self.model = trimmed;
        }
        self
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.client = self.client.with_base_url(url);
        self
    }

    fn build_request(
        &self,
        task: &str,
        files: &[FileContext],
        options: &ExecuteOptions,
    ) -> ChatCompletionRequest {
        let prompt_messages =
            self.prompt_builder
                .build_messages(task, files, options.preferred_format.clone());
        let messages = to_client_messages(prompt_messages);

        ChatCompletionRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            max_tokens: options.max_tokens,
            temperature: options.temperature,
            stream_options: None,
        }
    }

    /// Internal execution method that accepts a run_id parameter.
    ///
    /// This ensures consistent run_id across logged events and returned actions.
    async fn execute_internal(
        &self,
        task: &str,
        files: &[FileContext],
        options: &ExecuteOptions,
        run_id: &str,
    ) -> Result<Vec<ProposedAction>, NexusError> {
        if options.dry_run {
            return Ok(Vec::new());
        }

        let request = self.build_request(task, files, options);
        let stream = self.client.chat_completion_stream(request).await?;
        let stream = Box::pin(stream);
        let (response, _usage) = StreamHandler::accumulate(stream).await?;
        self.parser.parse(&response, run_id)
    }

    /// Internal streaming execution method that accepts a run_id parameter.
    async fn execute_streaming_internal(
        &self,
        task: &str,
        files: &[FileContext],
        options: &ExecuteOptions,
        run_id: &str,
        on_chunk: Box<dyn Fn(StreamChunk) + Send>,
    ) -> Result<Vec<ProposedAction>, NexusError> {
        if options.dry_run {
            on_chunk(StreamChunk::Done);
            return Ok(Vec::new());
        }

        let request = self.build_request(task, files, options);
        let stream = self.client.chat_completion_stream(request).await?;
        let stream = Box::pin(stream);
        let callback = move |chunk| on_chunk(chunk);
        let (response, _usage) = StreamHandler::with_callback(stream, callback).await?;
        self.parser.parse(&response, run_id)
    }

    pub async fn execute_with_logging(
        &self,
        task: &str,
        files: &[FileContext],
        options: ExecuteOptions,
        writer: &mut EventLogWriter,
    ) -> Result<Vec<ProposedAction>, NexusError> {
        let run_id = generate_run_id();
        let started_at = Instant::now();

        let started = helpers::executor_started(&run_id, task, files.len(), &self.model);
        writer.append(&started)?;

        // Use the same run_id for execution to ensure event-action correlation
        let result = self.execute_internal(task, files, &options, &run_id).await;
        match result {
            Ok(actions) => {
                for action in &actions {
                    let kind = action_kind_label(&action.kind);
                    let event =
                        helpers::action_proposed(&run_id, &action.id, kind, &action.summary, None);
                    writer.append(&event)?;
                }

                let duration_ms = started_at.elapsed().as_millis();
                let completed = helpers::executor_completed(&run_id, actions.len(), duration_ms);
                writer.append(&completed)?;
                writer.sync()?;
                Ok(actions)
            }
            Err(err) => {
                let status_code = match &err {
                    NexusError::ApiError { status_code, .. } => *status_code,
                    _ => None,
                };

                let failed = helpers::executor_failed(&run_id, &err.to_string(), status_code);
                writer.append(&failed)?;
                writer.sync()?;
                Err(err)
            }
        }
    }
}

#[async_trait]
impl Executor for CodexAdapter {
    async fn execute(
        &self,
        task: &str,
        files: Vec<FileContext>,
        options: ExecuteOptions,
    ) -> Result<Vec<ProposedAction>, NexusError> {
        let run_id = generate_run_id();
        self.execute_internal(task, &files, &options, &run_id).await
    }

    async fn execute_streaming(
        &self,
        task: &str,
        files: Vec<FileContext>,
        options: ExecuteOptions,
        on_chunk: Box<dyn Fn(StreamChunk) + Send>,
    ) -> Result<Vec<ProposedAction>, NexusError> {
        let run_id = generate_run_id();
        self.execute_streaming_internal(task, &files, &options, &run_id, on_chunk)
            .await
    }
}

fn generate_run_id() -> String {
    let now = Utc::now();
    let timestamp = now.format(RUN_ID_TIME_FORMAT).to_string();
    let millis = now.timestamp_subsec_millis();
    format!(
        "{RUN_ID_PREFIX}{timestamp}_{millis:0width$}",
        width = RUN_ID_MILLIS_WIDTH
    )
}

fn to_client_messages(messages: Vec<PromptChatMessage>) -> Vec<ClientChatMessage> {
    messages
        .into_iter()
        .map(|message| ClientChatMessage {
            role: message.role,
            content: message.content,
        })
        .collect()
}

fn action_kind_label(kind: &ActionKindTag) -> &'static str {
    match kind {
        ActionKindTag::Handoff => "handoff",
        ActionKindTag::Patch => "patch",
        ActionKindTag::Command => "command",
        ActionKindTag::PlanPatch => "plan_patch",
        ActionKindTag::AgendaPatch => "agenda_patch",
        ActionKindTag::FileCreate => "file_create",
        ActionKindTag::FileRename => "file_rename",
        ActionKindTag::FileDelete => "file_delete",
    }
}
