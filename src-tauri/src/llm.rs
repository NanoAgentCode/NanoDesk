use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};
use crate::models::{
    ChatMessage, ChatRequest, ChatResponse, ChatStreamEvent, ChatStreamRequest, ModelConfig,
    ModelConfigDraft,
};

#[derive(Debug, Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: i64,
    completion_tokens: i64,
}

#[derive(Debug, Serialize)]
struct OpenAiEmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelListItem>,
}

#[derive(Debug, Deserialize)]
struct ModelListItem {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamDelta {
    content: Option<String>,
    reasoning: Option<String>,
    reasoning_content: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnthropicChatRequest {
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicChatResponse {
    content: Vec<AnthropicContentBlock>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: i64,
    output_tokens: i64,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamChunk {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<AnthropicStreamDelta>,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    thinking: Option<String>,
}

enum ParsedStreamDelta {
    Content(String),
    Reasoning(String),
}

pub async fn send_chat_completion(
    config: ModelConfig,
    request: ChatRequest,
) -> AppResult<ChatResponse> {
    if is_anthropic_provider(&config.provider) {
        send_anthropic_chat_completion(config, request).await
    } else {
        send_openai_chat_completion(config, request).await
    }
}

pub async fn send_chat_completion_stream(
    app: AppHandle,
    config: ModelConfig,
    request: ChatStreamRequest,
) -> AppResult<()> {
    stream_chat_completion(config, request, |event| {
        app.emit("chat-stream", event)
            .map_err(|err| AppError::Message(err.to_string()))
    })
    .await
}

pub async fn stream_chat_completion<F>(
    config: ModelConfig,
    request: ChatStreamRequest,
    emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    if is_anthropic_provider(&config.provider) {
        send_anthropic_chat_completion_stream(config, request, emit).await
    } else {
        send_openai_chat_completion_stream(config, request, emit).await
    }
}

pub async fn create_embeddings(
    config: &ModelConfig,
    input: Vec<String>,
) -> AppResult<Vec<Vec<f32>>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let base_url = if config.embedding_base_url.trim().is_empty() {
        config.base_url.trim()
    } else {
        config.embedding_base_url.trim()
    };
    let model = if config.embedding_model.trim().is_empty() {
        "text-embedding-3-small"
    } else {
        config.embedding_model.trim()
    };
    let api_key = if config.embedding_api_key.trim().is_empty() {
        config.api_key.trim()
    } else {
        config.embedding_api_key.trim()
    };

    if api_key.is_empty() && !base_url.contains("localhost") {
        return Err(AppError::Message("missing embeddings api key".to_string()));
    }

    let endpoint = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let payload = OpenAiEmbeddingRequest {
        model: model.to_string(),
        input,
    };

    let client = reqwest::Client::new();
    let mut builder = client
        .post(endpoint)
        .header("content-type", "application/json")
        .json(&payload);
    if !api_key.is_empty() {
        builder = builder.bearer_auth(api_key);
    }

    let response = builder.send().await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "embeddings request failed with {status}: {text}"
        )));
    }

    let parsed: OpenAiEmbeddingResponse = serde_json::from_str(&text)?;
    let mut data = parsed.data;
    data.sort_by_key(|item| item.index);
    let embeddings = data
        .into_iter()
        .map(|item| item.embedding)
        .collect::<Vec<_>>();
    Ok(embeddings)
}

pub async fn list_available_models(draft: &ModelConfigDraft) -> AppResult<Vec<String>> {
    let base_url = draft.base_url.trim();
    if base_url.is_empty() {
        return Err(AppError::Message("模型接口地址不能为空".to_string()));
    }
    if draft.api_key.trim().is_empty() && !base_url.contains("localhost") {
        return Err(AppError::Message("请先填写 API Key".to_string()));
    }

    let endpoint = model_list_endpoint(&draft.provider, base_url);
    let client = reqwest::Client::new();
    let mut builder = client.get(endpoint).header("accept", "application/json");
    if is_anthropic_provider(&draft.provider) {
        if !draft.api_key.trim().is_empty() {
            builder = builder.header("x-api-key", draft.api_key.trim());
        }
        builder = builder.header("anthropic-version", "2023-06-01");
    } else if !draft.api_key.trim().is_empty() {
        builder = builder.bearer_auth(draft.api_key.trim());
    }

    let response = builder.send().await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "获取模型列表失败 ({status}): {text}"
        )));
    }

    parse_model_list(&text)
}

fn model_list_endpoint(provider: &str, base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if is_anthropic_provider(provider) && !base.ends_with("/v1") {
        format!("{base}/v1/models?limit=1000")
    } else if is_anthropic_provider(provider) {
        format!("{base}/models?limit=1000")
    } else {
        format!("{base}/models")
    }
}

fn parse_model_list(body: &str) -> AppResult<Vec<String>> {
    let parsed: ModelListResponse = serde_json::from_str(body)?;
    let mut models = parsed
        .data
        .into_iter()
        .map(|item| item.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    if models.is_empty() {
        return Err(AppError::Message("服务商未返回可用模型".to_string()));
    }
    Ok(models)
}

async fn send_openai_chat_completion(
    config: ModelConfig,
    request: ChatRequest,
) -> AppResult<ChatResponse> {
    ensure_api_key(&config)?;
    let endpoint = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let payload = OpenAiChatRequest {
        model: config.model,
        messages: request.messages,
        temperature: request.temperature.unwrap_or(0.4),
        stream: None,
        max_tokens: request.max_tokens,
    };

    let client = reqwest::Client::new();
    let mut builder = client
        .post(endpoint)
        .header("content-type", "application/json")
        .json(&payload);

    if !config.api_key.trim().is_empty() {
        builder = builder.bearer_auth(config.api_key);
    }

    let response = builder.send().await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "model request failed with {status}: {text}"
        )));
    }

    let parsed: OpenAiChatResponse = serde_json::from_str(&text)?;
    let usage = parsed.usage;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| AppError::Message("model returned no choices".to_string()))?;

    Ok(ChatResponse {
        content,
        input_tokens: usage.as_ref().map(|usage| usage.prompt_tokens),
        output_tokens: usage.as_ref().map(|usage| usage.completion_tokens),
    })
}

async fn send_anthropic_chat_completion(
    config: ModelConfig,
    request: ChatRequest,
) -> AppResult<ChatResponse> {
    ensure_api_key(&config)?;
    let endpoint = anthropic_messages_endpoint(&config.base_url);
    let payload = build_anthropic_payload(
        config.model,
        request.messages,
        request.temperature.unwrap_or(0.4),
        None,
        request.max_tokens.unwrap_or(4096),
    );

    let response = reqwest::Client::new()
        .post(endpoint)
        .header("content-type", "application/json")
        .header("x-api-key", config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(AppError::Message(format!(
            "model request failed with {status}: {text}"
        )));
    }

    let parsed: AnthropicChatResponse = serde_json::from_str(&text)?;
    let usage = parsed.usage;
    let content = parsed
        .content
        .into_iter()
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("");

    Ok(ChatResponse {
        content,
        input_tokens: usage.as_ref().map(|usage| usage.input_tokens),
        output_tokens: usage.as_ref().map(|usage| usage.output_tokens),
    })
}

async fn send_openai_chat_completion_stream<F>(
    config: ModelConfig,
    request: ChatStreamRequest,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    if let Err(err) = ensure_api_key(&config) {
        emit_stream_error(&mut emit, &request.request_id, &err.to_string());
        return Err(err);
    }

    let endpoint = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let payload = OpenAiChatRequest {
        model: config.model,
        messages: request.messages,
        temperature: request.temperature.unwrap_or(0.4),
        stream: Some(true),
        max_tokens: None,
    };

    let mut builder = reqwest::Client::new()
        .post(endpoint)
        .header("content-type", "application/json")
        .json(&payload);

    if !config.api_key.trim().is_empty() {
        builder = builder.bearer_auth(config.api_key);
    }

    stream_sse_response(request.request_id, builder, StreamProvider::OpenAi, emit).await
}

async fn send_anthropic_chat_completion_stream<F>(
    config: ModelConfig,
    request: ChatStreamRequest,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    if let Err(err) = ensure_api_key(&config) {
        emit_stream_error(&mut emit, &request.request_id, &err.to_string());
        return Err(err);
    }

    let endpoint = anthropic_messages_endpoint(&config.base_url);
    let payload = build_anthropic_payload(
        config.model,
        request.messages,
        request.temperature.unwrap_or(0.4),
        Some(true),
        4096,
    );
    let builder = reqwest::Client::new()
        .post(endpoint)
        .header("content-type", "application/json")
        .header("x-api-key", config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload);

    stream_sse_response(request.request_id, builder, StreamProvider::Anthropic, emit).await
}

async fn stream_sse_response<F>(
    request_id: String,
    builder: reqwest::RequestBuilder,
    provider: StreamProvider,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    let response = match builder.send().await {
        Ok(response) => response,
        Err(err) => {
            emit_stream_error(&mut emit, &request_id, &err.to_string());
            return Err(AppError::from(err));
        }
    };
    let status = response.status();

    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        emit_stream_error(
            &mut emit,
            &request_id,
            &format!("model request failed with {status}: {text}"),
        );
        return Err(AppError::Message(format!(
            "model request failed with {status}: {text}"
        )));
    }

    let mut buffer = String::new();
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                emit_stream_error(&mut emit, &request_id, &err.to_string());
                return Err(AppError::from(err));
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();
            process_sse_line(&mut emit, &request_id, &line, provider)?;
        }
    }

    if !buffer.trim().is_empty() {
        process_sse_line(&mut emit, &request_id, buffer.trim(), provider)?;
    }

    emit(ChatStreamEvent::Done { request_id })?;
    Ok(())
}

fn process_sse_line<F>(
    emit: &mut F,
    request_id: &str,
    line: &str,
    provider: StreamProvider,
) -> AppResult<()>
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    if line.is_empty() || line.starts_with(':') || !line.starts_with("data:") {
        return Ok(());
    }

    let data = line.trim_start_matches("data:").trim();
    if data == "[DONE]" {
        return Ok(());
    }

    let delta = match provider {
        StreamProvider::OpenAi => parse_openai_delta(data),
        StreamProvider::Anthropic => parse_anthropic_delta(data),
    };

    if let Some(delta) = delta {
        let event = match delta {
            ParsedStreamDelta::Content(content) => ChatStreamEvent::Delta {
                request_id: request_id.to_string(),
                content,
            },
            ParsedStreamDelta::Reasoning(content) => ChatStreamEvent::ReasoningDelta {
                request_id: request_id.to_string(),
                content,
            },
        };

        emit(event)?;
    }

    Ok(())
}

fn parse_openai_delta(data: &str) -> Option<ParsedStreamDelta> {
    let parsed: OpenAiStreamChunk = serde_json::from_str(data).ok()?;
    let mut content_parts = Vec::new();
    let mut reasoning_parts = Vec::new();

    for choice in parsed.choices {
        if let Some(content) = choice.delta.content {
            content_parts.push(content);
        }
        if let Some(reasoning) = choice.delta.reasoning_content {
            reasoning_parts.push(reasoning);
        }
        if let Some(reasoning) = choice.delta.reasoning {
            reasoning_parts.push(reasoning);
        }
    }

    let content = content_parts.join("");
    if !content.is_empty() {
        return Some(ParsedStreamDelta::Content(content));
    }

    let reasoning = reasoning_parts.join("");
    if !reasoning.is_empty() {
        return Some(ParsedStreamDelta::Reasoning(reasoning));
    }

    None
}

fn parse_anthropic_delta(data: &str) -> Option<ParsedStreamDelta> {
    let parsed: AnthropicStreamChunk = serde_json::from_str(data).ok()?;
    if parsed.event_type != "content_block_delta" {
        return None;
    }
    let delta = parsed.delta?;

    if delta.delta_type.as_deref() == Some("thinking_delta") {
        return delta
            .thinking
            .filter(|thinking| !thinking.is_empty())
            .map(ParsedStreamDelta::Reasoning);
    }

    delta
        .text
        .filter(|text| !text.is_empty())
        .map(ParsedStreamDelta::Content)
}

fn build_anthropic_payload(
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    stream: Option<bool>,
    max_tokens: u32,
) -> AnthropicChatRequest {
    let mut system_parts = Vec::new();
    let mut anthropic_messages = Vec::new();

    for message in messages {
        if message.role == "system" {
            system_parts.push(message.content);
            continue;
        }

        anthropic_messages.push(AnthropicMessage {
            role: if message.role == "assistant" {
                "assistant".to_string()
            } else {
                "user".to_string()
            },
            content: message.content,
        });
    }

    AnthropicChatRequest {
        model,
        system: if system_parts.is_empty() {
            None
        } else {
            Some(system_parts.join("\n\n"))
        },
        messages: anthropic_messages,
        max_tokens,
        temperature,
        stream,
    }
}

fn anthropic_messages_endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

fn ensure_api_key(config: &ModelConfig) -> AppResult<()> {
    if config.api_key.trim().is_empty() && !config.base_url.contains("localhost") {
        return Err(AppError::Message("missing api key".to_string()));
    }
    Ok(())
}

fn is_anthropic_provider(provider: &str) -> bool {
    let provider = provider.trim().to_lowercase();
    provider == "anthropic" || provider == "claude"
}

#[cfg(test)]
mod model_list_tests {
    use super::{model_list_endpoint, parse_model_list};

    #[test]
    fn builds_provider_specific_model_list_endpoints() {
        assert_eq!(
            model_list_endpoint("openai-compatible", "https://api.openai.com/v1"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            model_list_endpoint("anthropic", "https://api.anthropic.com"),
            "https://api.anthropic.com/v1/models?limit=1000"
        );
        assert_eq!(
            model_list_endpoint("anthropic", "https://gateway.example/v1/"),
            "https://gateway.example/v1/models?limit=1000"
        );
    }

    #[test]
    fn parses_sorts_and_deduplicates_model_ids() {
        let body = r#"{"data":[{"id":"glm-4.5"},{"id":"glm-4-air"},{"id":"glm-4.5"},{"id":" "}]}"#;
        assert_eq!(
            parse_model_list(body).unwrap(),
            vec!["glm-4-air".to_string(), "glm-4.5".to_string()]
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum StreamProvider {
    OpenAi,
    Anthropic,
}

fn emit_stream_error<F>(emit: &mut F, request_id: &str, message: &str)
where
    F: FnMut(ChatStreamEvent) -> AppResult<()>,
{
    let _ = emit(ChatStreamEvent::Error {
        request_id: request_id.to_string(),
        message: message.to_string(),
    });
}
