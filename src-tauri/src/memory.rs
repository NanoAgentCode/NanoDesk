use tauri::State;

use crate::error::{AppError, AppResult};
use crate::llm::create_embeddings;
use crate::models::{Memory, MemoryDraft, MemoryPatch, ModelConfig};
use crate::AppState;

const DEFAULT_MEMORY_LIMIT: i64 = 8;
const MAX_LAZY_BACKFILL: i64 = 128;

#[tauri::command]
pub async fn list_memories(state: State<'_, AppState>) -> AppResult<Vec<Memory>> {
    state.db.lock().await.list_memories()
}

#[tauri::command]
pub async fn list_enabled_memories(state: State<'_, AppState>) -> AppResult<Vec<Memory>> {
    state.db.lock().await.list_enabled_memories()
}

#[tauri::command]
pub async fn list_relevant_memories(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<Memory>> {
    let limit = limit.unwrap_or(DEFAULT_MEMORY_LIMIT).clamp(1, 30);
    let query = query.trim().to_string();
    if query.is_empty() {
        let mut memories = state.db.lock().await.list_enabled_memories()?;
        memories.truncate(limit as usize);
        return Ok(memories);
    }
    if state.db.lock().await.list_enabled_memories()?.is_empty() {
        return Ok(Vec::new());
    }

    let embedding_config = state
        .db
        .lock()
        .await
        .get_model_config("embedding-config")
        .ok();
    if let Some(config) = embedding_config.as_ref() {
        lazy_backfill_embeddings(&state, config).await;
    }

    let query_embedding = match embedding_config.as_ref() {
        Some(config) => create_embeddings(config, vec![query.clone()])
            .await
            .ok()
            .and_then(|mut embeddings| embeddings.pop()),
        None => None,
    };

    state.db.lock().await.search_hybrid_memories(
        &query,
        query_embedding.as_deref(),
        embedding_config.as_ref().map(embedding_model),
        limit,
    )
}

#[tauri::command]
pub async fn search_memories(state: State<'_, AppState>, query: String) -> AppResult<Vec<Memory>> {
    state.db.lock().await.search_memories(&query)
}

#[tauri::command]
pub async fn create_memory(state: State<'_, AppState>, draft: MemoryDraft) -> AppResult<Memory> {
    let memory = state.db.lock().await.create_memory(draft)?;
    index_memory_embedding(&state, &memory).await;
    Ok(memory)
}

#[tauri::command]
pub async fn update_memory(state: State<'_, AppState>, patch: MemoryPatch) -> AppResult<Memory> {
    let memory = state.db.lock().await.update_memory(patch)?;
    index_memory_embedding(&state, &memory).await;
    Ok(memory)
}

#[tauri::command]
pub async fn delete_memory(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_memory(&id)
}

pub(crate) async fn retrieve_for_cli(
    db: &crate::db::Database,
    query: &str,
    limit: i64,
) -> AppResult<Vec<Memory>> {
    if db.list_enabled_memories()?.is_empty() {
        return Ok(Vec::new());
    }
    let config = db.get_model_config("embedding-config").ok();
    if let Some(config) = config.as_ref() {
        let missing =
            db.list_memories_missing_embedding(embedding_model(config), MAX_LAZY_BACKFILL)?;
        if !missing.is_empty() {
            if let Ok(embeddings) = embed_memory_batch(config, &missing).await {
                for (memory, embedding) in missing.iter().zip(embeddings.iter()) {
                    db.upsert_memory_embedding(memory, embedding_model(config), embedding)?;
                }
            }
        }
    }
    let query_embedding = match config.as_ref() {
        Some(config) => create_embeddings(config, vec![query.to_string()])
            .await
            .ok()
            .and_then(|mut embeddings| embeddings.pop()),
        None => None,
    };
    db.search_hybrid_memories(
        query,
        query_embedding.as_deref(),
        config.as_ref().map(embedding_model),
        limit,
    )
}

async fn lazy_backfill_embeddings(state: &State<'_, AppState>, config: &ModelConfig) {
    let model = embedding_model(config).to_string();
    let missing = match state
        .db
        .lock()
        .await
        .list_memories_missing_embedding(&model, MAX_LAZY_BACKFILL)
    {
        Ok(memories) => memories,
        Err(error) => {
            crate::logging::warn(
                "memory",
                "failed to inspect memory vector index",
                serde_json::json!({ "error": error.to_string() }),
            );
            return;
        }
    };
    if missing.is_empty() {
        return;
    }

    match embed_memory_batch(config, &missing).await {
        Ok(embeddings) => {
            let db = state.db.lock().await;
            for (memory, embedding) in missing.iter().zip(embeddings.iter()) {
                if let Err(error) = db.upsert_memory_embedding(memory, &model, embedding) {
                    crate::logging::warn(
                        "memory",
                        "failed to persist memory embedding",
                        serde_json::json!({ "memory_id": memory.id, "error": error.to_string() }),
                    );
                }
            }
        }
        Err(error) => crate::logging::warn(
            "memory",
            "memory embedding backfill skipped",
            serde_json::json!({ "error": error.to_string() }),
        ),
    }
}

async fn index_memory_embedding(state: &State<'_, AppState>, memory: &Memory) {
    if !memory.enabled {
        return;
    }
    let config = state
        .db
        .lock()
        .await
        .get_model_config("embedding-config")
        .ok();
    let Some(config) = config else {
        return;
    };
    if !state
        .db
        .lock()
        .await
        .memory_needs_embedding(memory, embedding_model(&config))
        .unwrap_or(true)
    {
        return;
    }

    match embed_memory_batch(&config, std::slice::from_ref(memory)).await {
        Ok(embeddings) => {
            if let Some(embedding) = embeddings.first() {
                if let Err(error) = state.db.lock().await.upsert_memory_embedding(
                    memory,
                    embedding_model(&config),
                    embedding,
                ) {
                    crate::logging::warn(
                        "memory",
                        "failed to update memory vector index",
                        serde_json::json!({ "memory_id": memory.id, "error": error.to_string() }),
                    );
                }
            }
        }
        Err(error) => crate::logging::warn(
            "memory",
            "memory saved without vector index",
            serde_json::json!({ "memory_id": memory.id, "error": error.to_string() }),
        ),
    }
}

async fn embed_memory_batch(config: &ModelConfig, memories: &[Memory]) -> AppResult<Vec<Vec<f32>>> {
    const EMBEDDING_BATCH_SIZE: usize = 32;

    let mut embeddings = Vec::with_capacity(memories.len());
    for batch in memories.chunks(EMBEDDING_BATCH_SIZE) {
        let texts = batch.iter().map(memory_embedding_text).collect::<Vec<_>>();
        embeddings.extend(create_embeddings(config, texts).await?);
    }
    if embeddings.len() != memories.len() {
        return Err(AppError::Message(
            "embeddings 返回数量与记忆数量不一致".to_string(),
        ));
    }
    Ok(embeddings)
}

fn memory_embedding_text(memory: &Memory) -> String {
    let tags = memory.tags.join(", ");
    format!(
        "标题：{}\n标签：{}\n内容：{}",
        memory.title, tags, memory.content
    )
    .chars()
    .take(12_000)
    .collect()
}

fn embedding_model(config: &ModelConfig) -> &str {
    let model = config.embedding_model.trim();
    if model.is_empty() {
        "text-embedding-3-small"
    } else {
        model
    }
}
