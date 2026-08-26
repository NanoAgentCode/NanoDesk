use tauri::State;

use crate::db::RagFileReplacement;
use crate::error::{AppError, AppResult};
use crate::llm::create_embeddings;
use crate::models::{RagChunkMatch, RagFile, RagFileDraft};
use crate::AppState;

#[tauri::command]
pub async fn list_rag_files(
    state: State<'_, AppState>,
    conversation_id: String,
) -> AppResult<Vec<RagFile>> {
    state.db.lock().await.list_rag_files(&conversation_id)
}

#[tauri::command]
pub async fn delete_rag_file(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_rag_file(&id)
}

#[tauri::command]
pub async fn index_rag_file(state: State<'_, AppState>, draft: RagFileDraft) -> AppResult<RagFile> {
    const MAX_RAG_FILE_CHARS: usize = 2_000_000;

    let content = normalize_rag_text(&draft.content);
    if content.is_empty() {
        return Err(AppError::Message("文件没有可索引文本".to_string()));
    }
    if content.chars().count() > MAX_RAG_FILE_CHARS {
        return Err(AppError::Message(
            "文件过大，当前轻量 RAG 单文件最多支持约 200 万字符".to_string(),
        ));
    }

    let chunks = chunk_rag_text(&content);
    if chunks.is_empty() {
        return Err(AppError::Message("文件没有可索引文本".to_string()));
    }

    let config = {
        let db = state.db.lock().await;
        db.get_model_config("embedding-config")
            .or_else(|_| db.get_model_config(&draft.model_config_id))?
    };
    let embedding_model = if config.embedding_model.trim().is_empty() {
        "text-embedding-3-small".to_string()
    } else {
        config.embedding_model.trim().to_string()
    };
    let embeddings = create_embeddings(&config, chunks.clone()).await?;
    if embeddings.len() != chunks.len() {
        return Err(AppError::Message(
            "embeddings 返回数量与文本分块不一致".to_string(),
        ));
    }

    let content_hash = rag_content_hash(&draft.name, &content);
    state.db.lock().await.replace_rag_file(RagFileReplacement {
        conversation_id: &draft.conversation_id,
        name: &draft.name,
        mime: &draft.mime,
        size: draft.size,
        content_hash: &content_hash,
        chunks: &chunks,
        embeddings: &embeddings,
        embedding_model: &embedding_model,
    })
}

#[tauri::command]
pub async fn search_rag_context(
    state: State<'_, AppState>,
    conversation_id: String,
    query: String,
    model_config_id: String,
    limit: Option<i64>,
) -> AppResult<Vec<RagChunkMatch>> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let has_files = !state
        .db
        .lock()
        .await
        .list_rag_files(&conversation_id)?
        .is_empty();
    if !has_files {
        return Ok(Vec::new());
    }

    let config = {
        let db = state.db.lock().await;
        db.get_model_config("embedding-config")
            .or_else(|_| db.get_model_config(&model_config_id))?
    };
    let embeddings = create_embeddings(&config, vec![query]).await?;
    let Some(query_embedding) = embeddings.first() else {
        return Ok(Vec::new());
    };

    state
        .db
        .lock()
        .await
        .search_rag_chunks(&conversation_id, query_embedding, limit.unwrap_or(6))
}

fn rag_content_hash(name: &str, content: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn normalize_rag_text(content: &str) -> String {
    content
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn chunk_rag_text(content: &str) -> Vec<String> {
    const TARGET_CHARS: usize = 1_600;
    const OVERLAP_CHARS: usize = 180;
    const MAX_CHUNKS: usize = 120;

    let chars = content.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() && chunks.len() < MAX_CHUNKS {
        let mut end = (start + TARGET_CHARS).min(chars.len());
        if end < chars.len() {
            let search_start = start + TARGET_CHARS.saturating_sub(400);
            if let Some(boundary) = (search_start..end)
                .rev()
                .find(|idx| matches!(chars[*idx], '\n' | '。' | '！' | '？' | '.' | '!' | '?'))
            {
                end = boundary + 1;
            }
        }

        let text = chars[start..end]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if !text.is_empty() {
            chunks.push(text);
        }

        if end >= chars.len() {
            break;
        }
        start = end.saturating_sub(OVERLAP_CHARS);
        if start >= end {
            start = end;
        }
    }

    chunks
}
