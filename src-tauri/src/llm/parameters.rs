use super::protocol::{AnthropicChatRequest, AnthropicMessage};
use crate::error::{AppError, AppResult};
use crate::models::{ChatMessage, ModelConfig};

pub(super) fn build_anthropic_payload(
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    stream: Option<bool>,
    max_tokens: u32,
    top_p: Option<f32>,
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
        top_p,
        stream,
    }
}

#[derive(Debug, PartialEq)]
pub(super) struct GenerationParams {
    pub(super) temperature: f32,
    pub(super) max_tokens: Option<u32>,
    pub(super) top_p: Option<f32>,
    pub(super) reasoning_effort: Option<String>,
}

pub(super) fn resolve_generation_params(
    config: &ModelConfig,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    top_p: Option<f32>,
    reasoning_effort: Option<String>,
) -> GenerationParams {
    let reasoning_effort = reasoning_effort
        .or_else(|| {
            let configured = config.reasoning_effort.trim();
            (!configured.is_empty()).then(|| configured.to_string())
        })
        .filter(|value| !value.trim().is_empty());

    GenerationParams {
        temperature: temperature.unwrap_or(config.temperature),
        max_tokens: max_tokens.or(config.max_tokens),
        top_p: top_p.or(config.top_p),
        reasoning_effort,
    }
}

pub(super) fn anthropic_messages_endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

pub(super) fn ensure_api_key(config: &ModelConfig) -> AppResult<()> {
    if config.api_key.trim().is_empty() && !config.base_url.contains("localhost") {
        return Err(AppError::Message("missing api key".to_string()));
    }
    Ok(())
}

pub(super) fn ensure_chat_model(config: &ModelConfig) -> AppResult<()> {
    if config.model_kind == "asr" || is_asr_model_id(&config.model) {
        return Err(AppError::Message(
            "所选模型仅支持语音识别，不能用于对话".to_string(),
        ));
    }
    if config.model_kind == "embedding" {
        Err(AppError::Message(
            "所选模型仅支持嵌入用途，不能用于对话".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub(super) fn is_anthropic_provider(provider: &str) -> bool {
    let provider = provider.trim().to_lowercase();
    provider == "anthropic" || provider == "claude"
}

pub(crate) fn is_asr_model_id(model_id: &str) -> bool {
    let id = model_id.to_ascii_lowercase();
    ["asr", "whisper", "transcrib", "sensevoice", "telespeech"]
        .iter()
        .any(|marker| id.contains(marker))
}
