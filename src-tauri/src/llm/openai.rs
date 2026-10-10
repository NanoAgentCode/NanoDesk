use super::{
    parameters::{ensure_api_key, resolve_generation_params},
    protocol::*,
    transport::{emit_stream_error, stream_sse_response},
};
use crate::error::{AppError, AppResult};
use crate::models::{ChatRequest, ChatResponse, ChatStreamEvent, ChatStreamRequest, ModelConfig};

pub(super) async fn send_openai_chat_completion(
    config: ModelConfig,
    request: ChatRequest,
) -> AppResult<ChatResponse> {
    ensure_api_key(&config)?;
    let endpoint = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));
    let generation = resolve_generation_params(
        &config,
        request.temperature,
        request.max_tokens,
        request.top_p,
        request.reasoning_effort,
    );
    let payload = OpenAiChatRequest {
        model: config.model,
        messages: request.messages,
        temperature: generation.temperature,
        stream: None,
        max_tokens: generation.max_tokens,
        top_p: generation.top_p,
        reasoning_effort: generation.reasoning_effort,
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
pub(super) async fn send_openai_chat_completion_stream<F>(
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
    let generation = resolve_generation_params(
        &config,
        request.temperature,
        request.max_tokens,
        request.top_p,
        request.reasoning_effort,
    );
    let payload = OpenAiChatRequest {
        model: config.model,
        messages: request.messages,
        temperature: generation.temperature,
        stream: Some(true),
        max_tokens: generation.max_tokens,
        top_p: generation.top_p,
        reasoning_effort: generation.reasoning_effort,
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
