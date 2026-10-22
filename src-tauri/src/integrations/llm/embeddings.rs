use super::protocol::*;
use crate::error::{AppError, AppResult};
use crate::models::ModelConfig;

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
