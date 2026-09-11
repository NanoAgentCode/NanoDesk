use std::path::{Path, PathBuf};

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::brand;
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::llm::create_embeddings;
use crate::models::{
    ProjectIndexChunk, ProjectIndexRun, ProjectIndexSearchResult, ProjectIndexStats,
};
use crate::project_files::project_root;
use crate::AppState;

pub(crate) const DOCUMENT_INDEXER: &str = "document";
const MAX_DOCUMENT_FILES: usize = 600;
const MAX_DOCUMENT_FILE_BYTES: u64 = 1024 * 1024;
const MAX_EMBEDDED_DOCUMENT_CHUNKS: usize = 500;
const CHUNK_LINES: usize = 80;
const CHUNK_OVERLAP: usize = 10;

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".idea",
    ".vscode",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "coverage",
    brand::PROJECT_DATA_DIRECTORY,
    ".codegraph",
];

#[tauri::command]
pub async fn index_project_documents(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<ProjectIndexRun> {
    let root = project_root(&project_path)?;
    let canonical_project_path = root.to_string_lossy().to_string();
    let embedding_config = state
        .db
        .lock()
        .await
        .get_model_config("embedding-config")
        .ok();
    let prepared =
        prepare_project_document_index(&root, &canonical_project_path, embedding_config.as_ref())
            .await?;
    let db = state.db.lock().await;
    persist_project_document_index(&db, &canonical_project_path, &prepared)
}

#[tauri::command]
pub async fn get_project_index_stats(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<ProjectIndexStats> {
    let root = project_root(&project_path)?;
    let canonical_project_path = root.to_string_lossy().to_string();
    state
        .db
        .lock()
        .await
        .get_project_index_stats(&canonical_project_path)
}

#[tauri::command]
pub async fn search_project_index(
    state: State<'_, AppState>,
    project_path: String,
    indexer: Option<String>,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<ProjectIndexSearchResult>> {
    let root = project_root(&project_path)?;
    let canonical_project_path = root.to_string_lossy().to_string();
    let query_embedding = build_query_embedding(&state, &query).await;
    state.db.lock().await.search_project_index(
        &canonical_project_path,
        indexer.as_deref(),
        &query,
        query_embedding.as_deref(),
        limit.unwrap_or(8),
    )
}

pub(crate) struct DocumentIndex {
    pub(crate) file_count: i64,
    pub(crate) chunks: Vec<ProjectIndexChunk>,
}

pub(crate) struct PreparedProjectDocumentIndex {
    index: DocumentIndex,
    embeddings: Option<(Vec<Vec<f32>>, String)>,
}

pub(crate) async fn prepare_project_document_index(
    root: &Path,
    project_path: &str,
    embedding_config: Option<&crate::models::ModelConfig>,
) -> AppResult<PreparedProjectDocumentIndex> {
    let index = build_document_index(root, project_path)?;
    let embeddings = build_optional_embeddings(embedding_config, &index.chunks).await;
    Ok(PreparedProjectDocumentIndex { index, embeddings })
}

pub(crate) fn persist_project_document_index(
    db: &Database,
    project_path: &str,
    prepared: &PreparedProjectDocumentIndex,
) -> AppResult<ProjectIndexRun> {
    let embedding_refs = prepared
        .embeddings
        .as_ref()
        .map(|(embeddings, model)| (embeddings.as_slice(), model.as_str()));
    db.replace_project_index(
        project_path,
        DOCUMENT_INDEXER,
        prepared.index.file_count,
        &prepared.index.chunks,
        embedding_refs,
    )
}

pub(crate) fn build_document_index(root: &Path, project_path: &str) -> AppResult<DocumentIndex> {
    let mut files = Vec::new();
    collect_document_files(root, &mut files)?;

    let now = Utc::now();
    let mut chunks = Vec::new();
    let mut indexed_files = 0i64;

    for file_path in files.iter().take(MAX_DOCUMENT_FILES) {
        let metadata = std::fs::metadata(file_path)
            .map_err(|err| AppError::Message(format!("读取文档文件信息失败: {err}")))?;
        if !metadata.is_file() || metadata.len() > MAX_DOCUMENT_FILE_BYTES {
            continue;
        }
        let content = match std::fs::read_to_string(file_path) {
            Ok(content) => normalize_text(&content),
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }
        indexed_files += 1;
        let relative_path = relative_file_path(root, file_path)?;
        chunks.extend(chunk_document(project_path, &relative_path, &content, now));
    }

    Ok(DocumentIndex {
        file_count: indexed_files,
        chunks,
    })
}

fn collect_document_files(current: &Path, files: &mut Vec<PathBuf>) -> AppResult<()> {
    if files.len() >= MAX_DOCUMENT_FILES {
        return Ok(());
    }
    let entries = std::fs::read_dir(current)
        .map_err(|err| AppError::Message(format!("读取文档目录失败: {err}")))?;
    for entry in entries {
        let entry = entry.map_err(|err| AppError::Message(format!("读取目录项失败: {err}")))?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if SKIP_DIRS
                .iter()
                .any(|skip| skip.eq_ignore_ascii_case(&file_name))
            {
                continue;
            }
            collect_document_files(&path, files)?;
        } else if is_supported_document_path(&path) {
            files.push(path);
            if files.len() >= MAX_DOCUMENT_FILES {
                break;
            }
        }
    }
    Ok(())
}

fn is_supported_document_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some(
            "md" | "mdx"
                | "txt"
                | "log"
                | "csv"
                | "tsv"
                | "json"
                | "jsonl"
                | "yaml"
                | "yml"
                | "toml"
                | "xml"
                | "html"
                | "htm"
                | "ini"
                | "conf"
                | "properties"
        )
    )
}

fn chunk_document(
    project_path: &str,
    file_path: &str,
    content: &str,
    now: chrono::DateTime<Utc>,
) -> Vec<ProjectIndexChunk> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut index = 0i64;
    while start < lines.len() {
        let end = (start + CHUNK_LINES).min(lines.len());
        let text = lines[start..end].join("\n").trim().to_string();
        if !text.is_empty() {
            chunks.push(ProjectIndexChunk {
                id: Uuid::new_v4().to_string(),
                project_path: project_path.to_string(),
                indexer: DOCUMENT_INDEXER.to_string(),
                file_path: file_path.to_string(),
                title: document_title(file_path, &text),
                chunk_index: index,
                start_line: start as i64 + 1,
                end_line: end as i64,
                content_hash: content_hash(file_path, &text),
                token_count: estimate_token_count(&text),
                text,
                created_at: now,
            });
            index += 1;
        }
        if end >= lines.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP);
        if start >= end {
            start = end;
        }
    }
    chunks
}

async fn build_optional_embeddings(
    config: Option<&crate::models::ModelConfig>,
    chunks: &[ProjectIndexChunk],
) -> Option<(Vec<Vec<f32>>, String)> {
    if chunks.is_empty() || chunks.len() > MAX_EMBEDDED_DOCUMENT_CHUNKS {
        return None;
    }
    let config = config?;
    let embedding_model = if config.embedding_model.trim().is_empty() {
        "text-embedding-3-small".to_string()
    } else {
        config.embedding_model.trim().to_string()
    };
    let texts = chunks
        .iter()
        .map(|chunk| chunk.text.clone())
        .collect::<Vec<_>>();
    match create_embeddings(config, texts).await {
        Ok(embeddings) if embeddings.len() == chunks.len() => Some((embeddings, embedding_model)),
        Ok(_) | Err(_) => None,
    }
}

async fn build_query_embedding(state: &State<'_, AppState>, query: &str) -> Option<Vec<f32>> {
    if query.trim().is_empty() {
        return None;
    }
    let config = state
        .db
        .lock()
        .await
        .get_model_config("embedding-config")
        .ok()?;
    create_embeddings(&config, vec![query.to_string()])
        .await
        .ok()
        .and_then(|embeddings| embeddings.into_iter().next())
}

fn relative_file_path(root: &Path, file_path: &Path) -> AppResult<String> {
    let relative = file_path
        .strip_prefix(root)
        .map_err(|err| AppError::Message(format!("解析文档相对路径失败: {err}")))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn normalize_text(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

fn document_title(file_path: &str, text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|title| !title.is_empty() && title.chars().count() <= 80)
        .unwrap_or_else(|| file_path.to_string())
}

fn content_hash(name: &str, content: &str) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn estimate_token_count(text: &str) -> i64 {
    let chinese_chars = text
        .chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .count();
    let non_chinese = text
        .chars()
        .map(|ch| {
            if ('\u{4e00}'..='\u{9fff}').contains(&ch) {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    let words = non_chinese.split_whitespace().count();
    chinese_chars as i64 + ((words as f64) * 1.3).ceil() as i64
}
