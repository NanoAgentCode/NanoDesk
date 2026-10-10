//! Chat facade; provider payloads and SSE transport live in focused modules.
mod protocol;
mod parameters;
mod openai;
mod anthropic;
mod transport;
mod embeddings;
mod catalog;

use tauri::{AppHandle, Emitter};
use crate::error::{AppError, AppResult};
use crate::models::{ChatRequest, ChatResponse, ChatStreamRequest, ChatStreamEvent, ModelConfig};
use parameters::{ensure_chat_model, is_anthropic_provider};
use openai::{send_openai_chat_completion, send_openai_chat_completion_stream};
use anthropic::{send_anthropic_chat_completion, send_anthropic_chat_completion_stream};
pub use embeddings::create_embeddings;
pub use catalog::list_available_models;
pub(crate) use parameters::is_asr_model_id;

pub async fn send_chat_completion(
    config: ModelConfig,
    request: ChatRequest,
) -> AppResult<ChatResponse> {
    ensure_chat_model(&config)?;
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
    ensure_chat_model(&config)?;
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
    ensure_chat_model(&config)?;
    if is_anthropic_provider(&config.provider) {
        send_anthropic_chat_completion_stream(config, request, emit).await
    } else {
        send_openai_chat_completion_stream(config, request, emit).await
    }
}

#[cfg(test)]
use {parameters::{resolve_generation_params, GenerationParams}, catalog::{infer_model_kind, model_list_endpoint, parse_model_list}};
#[cfg(test)]
mod model_list_tests {
    use super::{
        infer_model_kind, model_list_endpoint, parse_model_list, resolve_generation_params,
        GenerationParams,
    };
    use crate::models::{AvailableModelInfo, ModelConfig};

    fn model_config() -> ModelConfig {
        let now = chrono::Utc::now();
        ModelConfig {
            id: "config-1".to_string(),
            name: "Test".to_string(),
            provider: "openai-compatible".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            model: "test-model".to_string(),
            api_key: String::new(),
            temperature: 0.7,
            max_tokens: Some(2048),
            context_window: 32_768,
            top_p: Some(0.85),
            reasoning_effort: "medium".to_string(),
            model_kind: "chat".to_string(),
            routing_group: "默认组".to_string(),
            routing_enabled: true,
            routing_cost: 3,
            routing_quality: 3,
            routing_speed: 3,
            routing_tasks: Vec::new(),
            embedding_provider: String::new(),
            embedding_base_url: String::new(),
            embedding_model: String::new(),
            embedding_api_key: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn classifies_asr_and_rejects_transcription_models_for_chat() {
        for name in ["Qwen/Qwen3-ASR-1.7B", "whisper-1", "gpt-4o-transcribe", "FunAudioLLM/SenseVoiceSmall"] {
            assert_eq!(infer_model_kind(name, &[], None, None), "asr");
            let config = ModelConfig { model: name.into(), ..model_config() };
            assert!(super::ensure_chat_model(&config).is_err());
        }
        assert_eq!(infer_model_kind("custom", &["speech-to-text".into()], None, None), "asr");
        assert_eq!(infer_model_kind("gpt-audio", &[], None, None), "chat");
        assert!(super::ensure_chat_model(&ModelConfig { model_kind: "asr".into(), ..model_config() }).is_err());
    }

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
        let body = r#"{"data":[{"id":"glm-4.5"},{"id":"glm-4-air","max_context_length":65536},{"id":"glm-4.5","context_length":131072},{"id":"text-vector","capabilities":["embedding"]},{"id":" "}]}"#;
        assert_eq!(
            parse_model_list(body).unwrap(),
            vec![
                AvailableModelInfo {
                    id: "glm-4-air".to_string(),
                    context_window: Some(65_536),
                    suggested_kind: "chat".to_string()
                },
                AvailableModelInfo {
                    id: "glm-4.5".to_string(),
                    context_window: Some(131_072),
                    suggested_kind: "chat".to_string()
                },
                AvailableModelInfo {
                    id: "text-vector".to_string(),
                    context_window: None,
                    suggested_kind: "embedding".to_string()
                }
            ]
        );
    }

    #[test]
    fn infers_embedding_models_from_common_ids() {
        assert_eq!(
            infer_model_kind("text-embedding-3-small", &[], None, None),
            "embedding"
        );
        assert_eq!(
            infer_model_kind("BAAI/bge-m3", &[], None, None),
            "embedding"
        );
        assert_eq!(infer_model_kind("gpt-4o-mini", &[], None, None), "chat");
        assert_eq!(
            infer_model_kind(
                "multi",
                &["completion".into(), "embedding".into()],
                None,
                None
            ),
            "both"
        );
    }

    #[test]
    fn resolves_saved_generation_parameters_and_request_overrides() {
        let config = model_config();
        assert_eq!(
            resolve_generation_params(&config, None, None, None, None),
            GenerationParams {
                temperature: 0.7,
                max_tokens: Some(2048),
                top_p: Some(0.85),
                reasoning_effort: Some("medium".to_string()),
            }
        );
        assert_eq!(
            resolve_generation_params(
                &config,
                Some(0.1),
                Some(256),
                Some(0.5),
                Some("low".to_string()),
            ),
            GenerationParams {
                temperature: 0.1,
                max_tokens: Some(256),
                top_p: Some(0.5),
                reasoning_effort: Some("low".to_string()),
            }
        );
    }
}
