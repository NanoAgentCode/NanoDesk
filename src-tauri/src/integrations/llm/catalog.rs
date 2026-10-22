use super::{
    parameters::{is_anthropic_provider, is_asr_model_id},
    protocol::*,
};
use crate::error::{AppError, AppResult};
use crate::models::{AvailableModelInfo, ModelConfigDraft};

pub async fn list_available_models(draft: &ModelConfigDraft) -> AppResult<Vec<AvailableModelInfo>> {
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

pub(super) fn model_list_endpoint(provider: &str, base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if is_anthropic_provider(provider) && !base.ends_with("/v1") {
        format!("{base}/v1/models?limit=1000")
    } else if is_anthropic_provider(provider) {
        format!("{base}/models?limit=1000")
    } else {
        format!("{base}/models")
    }
}

pub(super) fn parse_model_list(body: &str) -> AppResult<Vec<AvailableModelInfo>> {
    let parsed: ModelListResponse = serde_json::from_str(body)?;
    let mut models = parsed
        .data
        .into_iter()
        .filter_map(|item| {
            let id = item.id.trim().to_string();
            (!id.is_empty()).then_some(AvailableModelInfo {
                suggested_kind: infer_model_kind(
                    &id,
                    &item.capabilities,
                    item.model_type.as_deref(),
                    item.task.as_deref(),
                )
                .to_string(),
                id,
                context_window: item
                    .context_window
                    .or(item.context_length)
                    .or(item.max_context_length),
            })
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    let mut deduplicated: Vec<AvailableModelInfo> = Vec::with_capacity(models.len());
    for model in models {
        if let Some(existing) = deduplicated.last_mut().filter(|item| item.id == model.id) {
            if existing.context_window.is_none() {
                existing.context_window = model.context_window;
            }
        } else {
            deduplicated.push(model);
        }
    }
    if deduplicated.is_empty() {
        return Err(AppError::Message("服务商未返回可用模型".to_string()));
    }
    Ok(deduplicated)
}

pub(super) fn infer_model_kind(
    model_id: &str,
    capabilities: &[String],
    model_type: Option<&str>,
    task: Option<&str>,
) -> &'static str {
    let metadata = capabilities
        .iter()
        .map(String::as_str)
        .chain(model_type)
        .chain(task)
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if [
        "asr",
        "transcrib",
        "speech-to-text",
        "speech_to_text",
        "speech-recognition",
        "speech_recognition",
    ]
    .iter()
    .any(|marker| metadata.contains(marker))
        || is_asr_model_id(model_id)
    {
        return "asr";
    }
    let supports_embedding = metadata.contains("embed") || metadata.contains("pooling");
    let supports_chat = metadata.contains("chat")
        || metadata.contains("completion")
        || metadata.contains("generate");
    if supports_embedding && supports_chat {
        return "both";
    }
    if supports_embedding {
        return "embedding";
    }
    if supports_chat {
        return "chat";
    }
    let id = model_id.to_ascii_lowercase();
    if ["embedding", "embed", "bge-", "e5-", "gte-", "nomic-embed"]
        .iter()
        .any(|marker| id.contains(marker))
    {
        "embedding"
    } else {
        "chat"
    }
}
