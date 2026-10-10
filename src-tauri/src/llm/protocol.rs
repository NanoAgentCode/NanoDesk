use crate::models::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(super) struct OpenAiChatRequest {
    pub(super) model: String,
    pub(super) messages: Vec<ChatMessage>,
    pub(super) temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_effort: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiChatResponse {
    pub(super) choices: Vec<OpenAiChoice>,
    pub(super) usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiUsage {
    pub(super) prompt_tokens: i64,
    pub(super) completion_tokens: i64,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAiEmbeddingRequest {
    pub(super) model: String,
    pub(super) input: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiEmbeddingResponse {
    pub(super) data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiEmbeddingData {
    pub(super) embedding: Vec<f32>,
    pub(super) index: usize,
}

#[derive(Debug, Deserialize)]
pub(super) struct ModelListResponse {
    pub(super) data: Vec<ModelListItem>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ModelListItem {
    pub(super) id: String,
    #[serde(default)]
    pub(super) capabilities: Vec<String>,
    #[serde(default, rename = "type")]
    pub(super) model_type: Option<String>,
    #[serde(default)]
    pub(super) task: Option<String>,
    #[serde(default)]
    pub(super) context_window: Option<u32>,
    #[serde(default)]
    pub(super) context_length: Option<u32>,
    #[serde(default)]
    pub(super) max_context_length: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiChoice {
    pub(super) message: ChatMessage,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiStreamChunk {
    pub(super) choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiStreamChoice {
    pub(super) delta: OpenAiStreamDelta,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiStreamDelta {
    pub(super) content: Option<String>,
    pub(super) reasoning: Option<String>,
    pub(super) reasoning_content: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct AnthropicChatRequest {
    pub(super) model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system: Option<String>,
    pub(super) messages: Vec<AnthropicMessage>,
    pub(super) max_tokens: u32,
    pub(super) temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stream: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(super) struct AnthropicMessage {
    pub(super) role: String,
    pub(super) content: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicChatResponse {
    pub(super) content: Vec<AnthropicContentBlock>,
    pub(super) usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicUsage {
    pub(super) input_tokens: i64,
    pub(super) output_tokens: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicContentBlock {
    pub(super) text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicStreamChunk {
    #[serde(rename = "type")]
    pub(super) event_type: String,
    pub(super) delta: Option<AnthropicStreamDelta>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    pub(super) delta_type: Option<String>,
    pub(super) text: Option<String>,
    pub(super) thinking: Option<String>,
}

pub(super) enum ParsedStreamDelta {
    Content(String),
    Reasoning(String),
}

#[derive(Debug, Clone, Copy)]
pub(super) enum StreamProvider {
    OpenAi,
    Anthropic,
}
