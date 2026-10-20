use tauri::{AppHandle, Emitter, State};

use crate::db::RagFileReplacement;
use crate::error::{AppError, AppResult};
use crate::llm::create_embeddings;
use crate::models::{RagChunkMatch, RagFile, RagFileDraft};
use crate::AppState;

#[tauri::command]
pub async fn list_rag_files(
    app: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
) -> AppResult<Vec<RagFile>> {
    let mut files = state.db.lock().await.list_rag_files(&conversation_id)?;
    // Resume only interrupted work. Failed jobs require an explicit reimport,
    // so polling cannot silently retry a failing provider or reset its budget.
    for file in &mut files {
        if let Some(mut progress) = file.preparation.clone().filter(|progress| matches!(progress.phase.as_str(), "preparing" | "reducing" | "queued")) {
            let model = state.db.lock().await.get_model_config(&progress.model_id);
            match model {
                Ok(model) => crate::document_insights::schedule(app.clone(), file.clone(), model),
                Err(_) => {
                    progress.phase = "failed".into();
                    progress.error = Some("预处理使用的模型已移除，请选择模型后重新上传同一文档".into());
                    state.db.lock().await.save_document_summary_node(&file.id, &file.content_hash, crate::document_insights::STATUS_KEY, 0, &serde_json::to_string(&progress)?)?;
                    file.preparation = Some(progress);
                }
            }
        }
    }
    Ok(files)
}

#[tauri::command]
pub async fn delete_rag_file(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.lock().await.delete_rag_file(&id)
}

#[tauri::command]
pub async fn index_rag_file(app: AppHandle, state: State<'_, AppState>, draft: RagFileDraft) -> AppResult<RagFile> {
    let model = state.db.lock().await.get_model_config(&draft.model_config_id)?;
    let mut file = index_with_state(&state, draft, |progress| { let _ = app.emit("document-index-progress", progress); }).await?;
    if file.status == "fulltext" { return Ok(file); }
    let progress = crate::document_insights::PreparationProgress { phase: "queued".into(), completed: 0, total: 0, model_id: model.id.clone(), error: None };
    state.db.lock().await.save_document_summary_node(&file.id, &file.content_hash, crate::document_insights::STATUS_KEY, 0, &serde_json::to_string(&progress)?)?;
    file.preparation = Some(progress);
    crate::document_insights::schedule(app, file.clone(), model);
    Ok(file)
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct IndexProgress {
    pub conversation_id: String,
    pub name: String,
    pub completed: usize,
    pub total: usize,
    pub phase: String,
}

pub(crate) fn text_tokens(text: &str) -> u32 {
    let (mut ascii, mut unicode) = (0u64, 0u64);
    for character in text.chars() {
        if character.is_ascii() { ascii += 1; } else { unicode += 1; }
    }
    (unicode + ascii.div_ceil(3)).min(u32::MAX as u64) as u32
}

pub(crate) fn fulltext_limit(model: &crate::models::ModelConfig) -> u32 {
    crate::context_budget::model_input_budget(model.context_window, model.max_tokens).saturating_sub(1024).saturating_div(4).min(4000)
}

pub(crate) async fn index_with_state(
    state: &AppState, draft: RagFileDraft, mut progress: impl FnMut(IndexProgress),
) -> AppResult<RagFile> {
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

    let chat_model = state.db.lock().await.get_model_config(&draft.model_config_id)?;
    let fulltext = text_tokens(&content) <= fulltext_limit(&chat_model);
    let chunks = if fulltext { vec![content.clone()] } else { chunk_rag_text(&content) };
    if chunks.is_empty() {
        return Err(AppError::Message("文件没有可索引文本".to_string()));
    }

    let content_hash = rag_content_hash(&draft.name, &content);
    let config = if fulltext { None } else { Some({
        let db = state.db.lock().await;
        db.get_model_config("embedding-config")
            .map_err(|_| AppError::from("长文档需要嵌入模型，请先在系统设置 → 模型路由中配置嵌入模型，再重试。"))?
    }) };
    let embedding_model = config.as_ref().map(embedding_identity).unwrap_or_else(|| "fulltext".into());
    let status = if fulltext { "fulltext" } else { "ready" };
    {
        let db = state.db.lock().await;
        if let Some(file) = db.list_rag_files(&draft.conversation_id)?.into_iter().find(|file| file.content_hash == content_hash && file.status == status) {
            if fulltext || db.rag_embedding_model(&file.id)?.as_deref() == Some(embedding_model.as_str()) {
                progress(IndexProgress { conversation_id: draft.conversation_id.clone(), name: draft.name.clone(), completed: chunks.len(), total: chunks.len(), phase: "reused".into() });
                return Ok(file);
            }
        }
    }
    let mut embeddings = Vec::new();
    if let Some(config) = config {
        let mut dimension = None;
        for batch in chunks.chunks(16) {
            progress(IndexProgress { conversation_id: draft.conversation_id.clone(), name: draft.name.clone(), completed: embeddings.len(), total: chunks.len(), phase: "indexing".into() });
            let vectors = create_embeddings(&config, batch.to_vec()).await?;
            validate_vectors(&vectors, batch.len(), &mut dimension)?;
            embeddings.extend(vectors);
        }
    } else {
        embeddings.resize(chunks.len(), Vec::new());
    }
    progress(IndexProgress { conversation_id: draft.conversation_id.clone(), name: draft.name.clone(), completed: chunks.len(), total: chunks.len(), phase: "saving".into() });
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

pub(crate) fn embedding_identity(config: &crate::models::ModelConfig) -> String {
    let provider = if config.embedding_provider.trim().is_empty() { &config.provider } else { &config.embedding_provider };
    let base = if config.embedding_base_url.trim().is_empty() { &config.base_url } else { &config.embedding_base_url };
    format!("{}|{}|{}", provider.trim(), base.trim().trim_end_matches('/'), embedding_model_name(config))
}

fn embedding_model_name(config: &crate::models::ModelConfig) -> &str {
    if config.embedding_model.trim().is_empty() { "text-embedding-3-small" } else { config.embedding_model.trim() }
}

fn validate_vectors(vectors: &[Vec<f32>], expected: usize, dimension: &mut Option<usize>) -> AppResult<()> {
    if vectors.len() != expected { return Err("embeddings 返回数量与文本分块不一致".into()); }
    for vector in vectors {
        if vector.is_empty() || vector.iter().any(|value| !value.is_finite()) { return Err("嵌入服务返回无效向量".into()); }
        if dimension.is_some_and(|dim| dim != vector.len()) { return Err("嵌入服务返回的向量维度不一致".into()); }
        *dimension = Some(vector.len());
    }
    Ok(())
}

#[tauri::command]
pub async fn search_rag_context(
    state: State<'_, AppState>,
    conversation_id: String,
    query: String,
    model_config_id: String,
    limit: Option<i64>,
) -> AppResult<Vec<RagChunkMatch>> {
    search_rag_with_state(&state, conversation_id, query, model_config_id, limit).await
}

pub(crate) async fn search_rag_with_state(
    state: &AppState, conversation_id: String, query: String,
    model_config_id: String, limit: Option<i64>,
) -> AppResult<Vec<RagChunkMatch>> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let has_files = state
        .db
        .lock()
        .await
        .list_rag_files(&conversation_id)?
        .iter().any(|file| file.status == "ready");
    if !has_files {
        return Ok(Vec::new());
    }

    let config = {
        let db = state.db.lock().await;
        let config = db.get_model_config("embedding-config")
            .or_else(|_| db.get_model_config(&model_config_id))?;
        for file in db.list_rag_files(&conversation_id)?.into_iter().filter(|file| file.status == "ready") {
            if let Some(saved) = db.rag_embedding_model(&file.id)? {
                if saved != embedding_identity(&config) && saved != embedding_model_name(&config) {
                    return Err("文档索引使用的嵌入模型已变更，请重新上传文档或重新建立索引。".into());
                }
            }
        }
        config
    };
    let embeddings = create_embeddings(&config, vec![query.clone()]).await?;
    validate_vectors(&embeddings, 1, &mut None)?;
    let query_embedding = &embeddings[0];

    state
        .db
        .lock()
        .await
        .search_rag_hybrid(&conversation_id, &query, query_embedding, limit.unwrap_or(6))
}

pub(crate) fn rag_content_hash(name: &str, content: &str) -> String {
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

    let chars = content.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_documents_keep_the_tail_beyond_120_chunks() {
        let content = format!("{}最终独有内容 TAIL-INDEX-8972", "长文档正文。".repeat(50000));
        let chunks = chunk_rag_text(&content);
        assert!(chunks.len() > 120);
        assert!(chunks.last().unwrap().contains("TAIL-INDEX-8972"));
    }

    #[test]
    fn chunking_has_overlap_and_makes_progress_without_punctuation() {
        let content = "中".repeat(6500);
        let chunks = chunk_rag_text(&content);
        assert_eq!(chunks.len(), 5);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 1600));
        assert_eq!(chunks.iter().map(|chunk| chunk.chars().count()).sum::<usize>(), 6500 + 4 * 180);
    }

    #[test]
    fn invalid_embedding_batches_fail_before_any_index_is_saved() {
        assert!(validate_vectors(&[], 1, &mut None).is_err());
        assert!(validate_vectors(&[vec![]], 1, &mut None).is_err());
        assert!(validate_vectors(&[vec![f32::NAN]], 1, &mut None).is_err());
        assert!(validate_vectors(&[vec![1.0], vec![1.0, 2.0]], 2, &mut None).is_err());
        let mut dimension = None;
        validate_vectors(&[vec![0.1, 0.2]], 1, &mut dimension).unwrap();
        assert!(validate_vectors(&[vec![0.1]], 1, &mut dimension).is_err());
    }

    #[test]
    fn embedding_identity_tracks_the_effective_fallback_endpoint() {
        let draft: crate::models::ModelConfigDraft = serde_json::from_value(serde_json::json!({"name":"test","provider":"openai-compatible","base_url":"http://localhost:1000/v1","model":"chat","api_key":""})).unwrap();
        let mut model = crate::models::ModelConfig::from_draft_for_connectivity(draft);
        let original = embedding_identity(&model);
        model.base_url = "http://localhost:2000/v1".into();
        assert_ne!(embedding_identity(&model), original);
        model.embedding_base_url = "http://localhost:1000/v1/".into();
        assert_eq!(embedding_identity(&model), original);
        model.embedding_model = "another-embedding-model".into();
        assert_ne!(embedding_identity(&model), original);
    }
}
