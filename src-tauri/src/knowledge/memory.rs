use crate::app_state::CommandState as State;

use crate::error::{AppError, AppResult};
use crate::llm::create_embeddings;
use crate::models::{Memory, MemoryDraft, MemoryPatch, ModelConfig};
use crate::AppState;

const DEFAULT_MEMORY_LIMIT: i64 = 8;
const MAX_LAZY_BACKFILL: i64 = 128;

#[tauri::command]
pub async fn get_global_memory_enabled(state: State<'_, AppState>) -> AppResult<bool> {
    state.db.lock().await.global_memory_enabled()
}

#[tauri::command]
pub async fn set_global_memory_enabled(state: State<'_, AppState>, enabled: bool) -> AppResult<bool> {
    state.db.lock().await.set_global_memory_enabled(enabled)
}

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
    let plan = {
        let db = state.db.lock().await;
        plan_memory_retrieval(&db, &query, limit)?
    };
    let prepared = prepare_memory_retrieval(plan).await;
    let db = state.db.lock().await;
    finish_memory_retrieval(&db, prepared)
}

#[tauri::command]
pub async fn search_memories(state: State<'_, AppState>, query: String) -> AppResult<Vec<Memory>> {
    state.db.lock().await.search_memories(&query)
}

#[tauri::command]
pub async fn create_memory(state: State<'_, AppState>, draft: MemoryDraft, from_chat: Option<bool>) -> AppResult<Memory> {
    let memory = {
        let db = state.db.lock().await;
        save_memory_draft(&db, draft, from_chat.unwrap_or(false))?
    };
    index_memory_embedding(&state, &memory).await;
    Ok(memory)
}

fn save_memory_draft(db: &crate::db::Database, draft: MemoryDraft, from_chat: bool) -> AppResult<Memory> {
    if from_chat && !db.global_memory_enabled()? {
        return Err(AppError::Message("全局记忆已关闭".into()));
    }
    db.create_memory(draft)
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

pub(crate) struct MemoryRetrievalPlan {
    query: String,
    limit: i64,
    has_memories: bool,
    config: Option<ModelConfig>,
    missing: Vec<Memory>,
}

pub(crate) struct PreparedMemoryRetrieval {
    plan: MemoryRetrievalPlan,
    missing_embeddings: Option<Vec<Vec<f32>>>,
    query_embedding: Option<Vec<f32>>,
}

impl PreparedMemoryRetrieval {
    pub(crate) fn query_embedding(&self) -> Option<&[f32]> {
        self.query_embedding.as_deref()
    }
}

pub(crate) fn plan_memory_retrieval(
    db: &crate::db::Database,
    query: &str,
    limit: i64,
) -> AppResult<MemoryRetrievalPlan> {
    let has_memories = db.global_memory_enabled()? && !db.list_enabled_memories()?.is_empty();
    let config = if has_memories && !query.is_empty() {
        db.get_model_config("embedding-config").ok()
    } else {
        None
    };
    let missing = match config.as_ref() {
        Some(config) => {
            db.list_memories_missing_embedding(embedding_model(config), MAX_LAZY_BACKFILL)?
        }
        None => Vec::new(),
    };
    Ok(MemoryRetrievalPlan {
        query: query.to_string(),
        limit,
        has_memories,
        config,
        missing,
    })
}

pub(crate) async fn prepare_memory_retrieval(plan: MemoryRetrievalPlan) -> PreparedMemoryRetrieval {
    let missing_embeddings = match plan.config.as_ref() {
        Some(config) if !plan.missing.is_empty() => {
            embed_memory_batch(config, &plan.missing).await.ok()
        }
        _ => None,
    };
    let query_embedding = match plan.config.as_ref() {
        Some(config) => create_embeddings(config, vec![plan.query.clone()])
            .await
            .ok()
            .and_then(|mut embeddings| embeddings.pop()),
        None => None,
    };
    PreparedMemoryRetrieval {
        plan,
        missing_embeddings,
        query_embedding,
    }
}

pub(crate) fn finish_memory_retrieval(
    db: &crate::db::Database,
    prepared: PreparedMemoryRetrieval,
) -> AppResult<Vec<Memory>> {
    let PreparedMemoryRetrieval {
        plan,
        missing_embeddings,
        query_embedding,
    } = prepared;
    if !db.global_memory_enabled()? || !plan.has_memories {
        return Ok(Vec::new());
    }
    if plan.query.is_empty() {
        let mut memories = db.list_enabled_memories()?;
        memories.truncate(plan.limit as usize);
        return Ok(memories);
    }
    if let (Some(config), Some(embeddings)) = (plan.config.as_ref(), missing_embeddings.as_ref()) {
        for (memory, embedding) in plan.missing.iter().zip(embeddings.iter()) {
            if let Err(error) =
                db.upsert_memory_embedding(memory, embedding_model(config), embedding)
            {
                crate::logging::warn(
                    "memory",
                    "failed to persist memory embedding",
                    serde_json::json!({ "memory_id": memory.id, "error": error.to_string() }),
                );
            }
        }
    }
    db.search_hybrid_memories(
        &plan.query,
        query_embedding.as_deref(),
        plan.config.as_ref().map(embedding_model),
        plan.limit,
    )
}

async fn index_memory_embedding(state: &State<'_, AppState>, memory: &Memory) {
    if !memory.enabled {
        return;
    }
    if !state.db.lock().await.global_memory_enabled().unwrap_or(false) {
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

#[cfg(test)]
mod global_memory_tests {
    use super::*;
    #[test]
    fn global_switch_rechecks_chat_writes_but_allows_manual_management() {
        let db = crate::db::Database::open(std::path::PathBuf::from(":memory:")).unwrap();
        db.set_global_memory_enabled(false).unwrap();
        let draft = || MemoryDraft { title: "发布约定".into(), content: "发布前运行测试".into(), tags: vec![], enabled: Some(true) };
        assert!(save_memory_draft(&db, draft(), true).is_err());
        assert!(db.list_memories().unwrap().is_empty());
        assert!(save_memory_draft(&db, draft(), false).is_ok());
        db.set_global_memory_enabled(true).unwrap();
        assert!(save_memory_draft(&db, draft(), true).is_ok());
    }

    #[tokio::test]
    async fn disabled_memory_skips_retrieval_and_rechecks_in_flight_results() {
        let db = crate::db::Database::open(std::path::PathBuf::from(":memory:")).unwrap();
        db.create_memory(MemoryDraft { title: "Rust".into(), content: "Use Rust".into(), tags: vec![], enabled: Some(true) }).unwrap();
        let prepared = prepare_memory_retrieval(plan_memory_retrieval(&db, "", 8).unwrap()).await;
        db.set_global_memory_enabled(false).unwrap();
        assert!(finish_memory_retrieval(&db, prepared).unwrap().is_empty());
        let disabled = plan_memory_retrieval(&db, "Rust", 8).unwrap();
        assert!(!disabled.has_memories);
        assert!(disabled.config.is_none());
        db.set_global_memory_enabled(true).unwrap();
        let prepared = prepare_memory_retrieval(plan_memory_retrieval(&db, "", 8).unwrap()).await;
        assert_eq!(finish_memory_retrieval(&db, prepared).unwrap().len(), 1);
    }
}
