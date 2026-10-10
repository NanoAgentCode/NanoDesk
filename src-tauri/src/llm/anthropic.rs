use super::{
    parameters::{
        anthropic_messages_endpoint, build_anthropic_payload, ensure_api_key,
        resolve_generation_params,
    },
    protocol::*,
    transport::{emit_stream_error, stream_sse_response},
};
use crate::error::{AppError, AppResult};
use crate::models::{ChatRequest, ChatResponse, ChatStreamEvent, ChatStreamRequest, ModelConfig};

pub(super) async fn send_anthropic_chat_completion(
    config: ModelConfig,
    request: ChatRequest,
) -> AppResult<ChatResponse> {
    ensure_api_key(&config)?;
    let endpoint = anthropic_messages_endpoint(&config.base_url);
    let generation = resolve_generation_params(
        &config,
        request.temperature,
        request.max_tokens,
        request.top_p,
        request.reasoning_effort,
    );
    let payload = build_anthropic_payload(
        config.model,
        request.messages,
        generation.temperature,
        None,
        generation.max_tokens.unwrap_or(4096),
        generation.top_p,
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
pub(super) async fn send_anthropic_chat_completion_stream<F>(
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
    let generation = resolve_generation_params(
        &config,
        request.temperature,
        request.max_tokens,
        request.top_p,
        request.reasoning_effort,
    );
    let payload = build_anthropic_payload(
        config.model,
        request.messages,
        generation.temperature,
        Some(true),
        generation.max_tokens.unwrap_or(4096),
        generation.top_p,
    );
    let builder = reqwest::Client::new()
        .post(endpoint)
        .header("content-type", "application/json")
        .header("x-api-key", config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload);

    stream_sse_response(request.request_id, builder, StreamProvider::Anthropic, emit).await
}
