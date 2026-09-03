use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::brand;
use crate::error::{AppError, AppResult};
use crate::llm::create_embeddings;
use crate::models::{
    CodeChunk, CodeEntity, CodeIndexRun, CodeIndexStats, CodeRelation, CodeSearchResult,
};
use crate::project_files::project_root;
use crate::AppState;

const MAX_CODE_FILES: usize = 800;
const MAX_CODE_FILE_BYTES: u64 = 768 * 1024;
const MAX_EMBEDDED_CODE_CHUNKS: usize = 400;
const CHUNK_LINES: usize = 90;
const CHUNK_OVERLAP: usize = 12;

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
pub async fn index_project_code(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<CodeIndexRun> {
    let root = project_root(&project_path)?;
    let canonical_project_path = root.to_string_lossy().to_string();
    let indexed = build_project_code_index(&root, &canonical_project_path)?;
    let embedding_config = {
        state
            .db
            .lock()
            .await
            .get_model_config("embedding-config")
            .ok()
    };
    let code_embeddings = if let Some(config) = embedding_config {
        if indexed.chunks.len() > MAX_EMBEDDED_CODE_CHUNKS {
            None
        } else {
            let embedding_model = if config.embedding_model.trim().is_empty() {
                "text-embedding-3-small".to_string()
            } else {
                config.embedding_model.trim().to_string()
            };
            let texts = indexed
                .chunks
                .iter()
                .map(|chunk| chunk.text.clone())
                .collect::<Vec<_>>();
            match create_embeddings(&config, texts).await {
                Ok(embeddings) if embeddings.len() == indexed.chunks.len() => {
                    Some((embeddings, embedding_model))
                }
                Ok(_) | Err(_) => None,
            }
        }
    } else {
        None
    };
    let embedding_refs = code_embeddings
        .as_ref()
        .map(|(embeddings, model)| (embeddings.as_slice(), model.as_str()));
    state.db.lock().await.replace_code_index(
        &canonical_project_path,
        indexed.file_count,
        &indexed.entities,
        &indexed.relations,
        &indexed.chunks,
        embedding_refs,
    )
}

#[tauri::command]
pub async fn get_code_index_stats(
    state: State<'_, AppState>,
    project_path: String,
) -> AppResult<CodeIndexStats> {
    let root = project_root(&project_path)?;
    let canonical_project_path = root.to_string_lossy().to_string();
    state
        .db
        .lock()
        .await
        .get_code_index_stats(&canonical_project_path)
}

#[tauri::command]
pub async fn search_code_index(
    state: State<'_, AppState>,
    project_path: String,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<CodeSearchResult>> {
    let root = project_root(&project_path)?;
    let canonical_project_path = root.to_string_lossy().to_string();
    let query_embedding = if !query.trim().is_empty() {
        let embedding_config = {
            state
                .db
                .lock()
                .await
                .get_model_config("embedding-config")
                .ok()
        };
        if let Some(config) = embedding_config {
            create_embeddings(&config, vec![query.clone()])
                .await
                .ok()
                .and_then(|embeddings| embeddings.into_iter().next())
        } else {
            None
        }
    } else {
        None
    };
    state.db.lock().await.search_code_index(
        &canonical_project_path,
        &query,
        query_embedding.as_deref(),
        limit.unwrap_or(8),
    )
}

pub(crate) struct ProjectCodeIndex {
    pub(crate) file_count: i64,
    pub(crate) entities: Vec<CodeEntity>,
    pub(crate) relations: Vec<CodeRelation>,
    pub(crate) chunks: Vec<CodeChunk>,
}

#[derive(Debug, Clone)]
struct PendingRelation {
    source_name: String,
    target_name: String,
    kind: String,
    file_path: String,
    line: i64,
}

pub(crate) fn build_project_code_index(
    root: &Path,
    project_path: &str,
) -> AppResult<ProjectCodeIndex> {
    let mut files = Vec::new();
    collect_code_files(root, root, &mut files)?;

    let now = Utc::now();
    let mut entities = Vec::new();
    let mut pending_relations = Vec::new();
    let mut chunks = Vec::new();

    for file_path in files.iter().take(MAX_CODE_FILES) {
        let relative_path = relative_file_path(root, file_path)?;
        let metadata = std::fs::metadata(file_path)
            .map_err(|err| AppError::Message(format!("读取代码文件信息失败: {err}")))?;
        if !metadata.is_file() || metadata.len() > MAX_CODE_FILE_BYTES {
            continue;
        }

        let Some(language) = language_for_path(file_path) else {
            continue;
        };
        let content = match std::fs::read_to_string(file_path) {
            Ok(content) => normalize_code_text(&content),
            Err(_) => continue,
        };
        if content.trim().is_empty() {
            continue;
        }

        let file_entities = extract_entities(project_path, &relative_path, language, &content, now);
        let file_relations =
            extract_pending_relations(&relative_path, language, &content, &file_entities);
        let file_chunks = chunk_code_file(project_path, &relative_path, language, &content, now);

        entities.extend(file_entities);
        pending_relations.extend(file_relations);
        chunks.extend(file_chunks);
    }

    let entity_by_name = build_entity_name_index(&entities);
    let relations = pending_relations
        .into_iter()
        .map(|relation| {
            let source_entity_id = entity_by_name.get(&relation.source_name).cloned();
            let target_entity_id = entity_by_name.get(&relation.target_name).cloned();
            CodeRelation {
                id: Uuid::new_v4().to_string(),
                project_path: project_path.to_string(),
                source_entity_id,
                source_name: relation.source_name,
                target_entity_id,
                target_name: relation.target_name,
                kind: relation.kind,
                file_path: relation.file_path,
                line: relation.line,
                created_at: now,
            }
        })
        .collect::<Vec<_>>();

    Ok(ProjectCodeIndex {
        file_count: files.len().min(MAX_CODE_FILES) as i64,
        entities,
        relations,
        chunks,
    })
}

fn collect_code_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> AppResult<()> {
    if files.len() >= MAX_CODE_FILES {
        return Ok(());
    }

    let entries = std::fs::read_dir(current)
        .map_err(|err| AppError::Message(format!("读取代码目录失败: {err}")))?;
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
            collect_code_files(root, &path, files)?;
        } else if language_for_path(&path).is_some() {
            let _ = root;
            files.push(path);
            if files.len() >= MAX_CODE_FILES {
                break;
            }
        }
    }
    Ok(())
}

fn language_for_path(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().to_lowercase().as_str() {
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" => Some("javascript"),
        "jsx" => Some("jsx"),
        "rs" => Some("rust"),
        _ => None,
    }
}

fn relative_file_path(root: &Path, file_path: &Path) -> AppResult<String> {
    let relative = file_path
        .strip_prefix(root)
        .map_err(|err| AppError::Message(format!("解析代码文件相对路径失败: {err}")))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn normalize_code_text(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
}

fn extract_entities(
    project_path: &str,
    file_path: &str,
    language: &str,
    content: &str,
    now: chrono::DateTime<Utc>,
) -> Vec<CodeEntity> {
    let mut entities = Vec::new();
    let lines = content.lines().collect::<Vec<_>>();
    let mut next_rust_command = false;

    for (index, line) in lines.iter().enumerate() {
        let line_number = index as i64 + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if language == "rust" && trimmed == "#[tauri::command]" {
            next_rust_command = true;
            continue;
        }

        if let Some((kind, name)) = extract_entity_name(language, trimmed, next_rust_command) {
            next_rust_command = false;
            entities.push(CodeEntity {
                id: Uuid::new_v4().to_string(),
                project_path: project_path.to_string(),
                file_path: file_path.to_string(),
                name,
                kind,
                language: language.to_string(),
                start_line: line_number,
                end_line: estimate_entity_end_line(&lines, index),
                signature: trimmed.to_string(),
                created_at: now,
            });
        } else if !trimmed.starts_with("#[") {
            next_rust_command = false;
        }
    }

    entities
}

fn extract_entity_name(
    language: &str,
    trimmed: &str,
    rust_command: bool,
) -> Option<(String, String)> {
    if language == "rust" {
        if let Some(name) = name_after_keyword(trimmed, "pub async fn ")
            .or_else(|| name_after_keyword(trimmed, "async fn "))
            .or_else(|| name_after_keyword(trimmed, "pub fn "))
            .or_else(|| name_after_keyword(trimmed, "fn "))
        {
            return Some((
                if rust_command {
                    "tauri_command"
                } else {
                    "function"
                }
                .to_string(),
                name,
            ));
        }
        if let Some(name) = name_after_keyword(trimmed, "pub struct ")
            .or_else(|| name_after_keyword(trimmed, "struct "))
        {
            return Some(("struct".to_string(), name));
        }
        if let Some(name) = name_after_keyword(trimmed, "pub enum ")
            .or_else(|| name_after_keyword(trimmed, "enum "))
        {
            return Some(("enum".to_string(), name));
        }
        return None;
    }

    if let Some(name) = name_after_keyword(trimmed, "export default function ")
        .or_else(|| name_after_keyword(trimmed, "export function "))
        .or_else(|| name_after_keyword(trimmed, "function "))
    {
        return Some(("function".to_string(), name));
    }
    if let Some(name) = name_after_keyword(trimmed, "export interface ")
        .or_else(|| name_after_keyword(trimmed, "interface "))
    {
        return Some(("interface".to_string(), name));
    }
    if let Some(name) =
        name_after_keyword(trimmed, "export type ").or_else(|| name_after_keyword(trimmed, "type "))
    {
        return Some(("type".to_string(), name));
    }
    if let Some(name) = name_after_keyword(trimmed, "export class ")
        .or_else(|| name_after_keyword(trimmed, "class "))
    {
        return Some(("class".to_string(), name));
    }
    if let Some(name) = const_function_name(trimmed) {
        let kind = if name.chars().next().map(char::is_uppercase).unwrap_or(false) {
            "component"
        } else {
            "function"
        };
        return Some((kind.to_string(), name));
    }

    None
}

fn name_after_keyword(line: &str, keyword: &str) -> Option<String> {
    let rest = line.strip_prefix(keyword)?;
    read_identifier(rest)
}

fn const_function_name(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("export const ")
        .or_else(|| line.strip_prefix("const "))
        .or_else(|| line.strip_prefix("let "))?;
    if !(line.contains("=>") || line.contains("function")) {
        return None;
    }
    read_identifier(rest)
}

fn read_identifier(value: &str) -> Option<String> {
    let name = value
        .trim_start()
        .chars()
        .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
        .collect::<String>();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn estimate_entity_end_line(lines: &[&str], start_index: usize) -> i64 {
    let mut depth = 0i64;
    let mut seen_open = false;
    for (offset, line) in lines.iter().enumerate().skip(start_index) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                seen_open = true;
            } else if ch == '}' && depth > 0 {
                depth -= 1;
            }
        }
        if seen_open && depth == 0 {
            return offset as i64 + 1;
        }
    }
    start_index as i64 + 1
}

fn extract_pending_relations(
    file_path: &str,
    language: &str,
    content: &str,
    entities: &[CodeEntity],
) -> Vec<PendingRelation> {
    let entity_names = entities
        .iter()
        .map(|entity| entity.name.clone())
        .collect::<HashSet<_>>();
    let mut relations = Vec::new();

    for (index, line) in content.lines().enumerate() {
        let line_number = index as i64 + 1;
        let trimmed = line.trim();
        let source_name = entity_for_line(entities, line_number)
            .map(|entity| entity.name.clone())
            .unwrap_or_else(|| file_path.to_string());

        if let Some(target) = extract_import_target(language, trimmed) {
            relations.push(PendingRelation {
                source_name: file_path.to_string(),
                target_name: target,
                kind: "imports".to_string(),
                file_path: file_path.to_string(),
                line: line_number,
            });
        }
        if let Some(target) = extract_invoke_target(trimmed) {
            relations.push(PendingRelation {
                source_name: source_name.clone(),
                target_name: target,
                kind: "invokes".to_string(),
                file_path: file_path.to_string(),
                line: line_number,
            });
        }

        for target in &entity_names {
            if *target == source_name {
                continue;
            }
            if contains_call_to(trimmed, target) {
                relations.push(PendingRelation {
                    source_name: source_name.clone(),
                    target_name: target.clone(),
                    kind: "calls".to_string(),
                    file_path: file_path.to_string(),
                    line: line_number,
                });
            }
        }
    }

    relations
}

fn entity_for_line(entities: &[CodeEntity], line: i64) -> Option<&CodeEntity> {
    entities
        .iter()
        .filter(|entity| entity.start_line <= line && entity.end_line >= line)
        .max_by_key(|entity| entity.start_line)
}

fn extract_import_target(language: &str, line: &str) -> Option<String> {
    if language == "rust" {
        return line
            .strip_prefix("use ")
            .map(|value| value.trim_end_matches(';').trim().to_string())
            .filter(|value| !value.is_empty());
    }
    if !line.starts_with("import ") {
        return None;
    }
    let marker = " from ";
    if let Some(index) = line.find(marker) {
        return quoted_value(&line[index + marker.len()..]);
    }
    quoted_value(line)
}

fn extract_invoke_target(line: &str) -> Option<String> {
    let index = line.find("invoke<").or_else(|| line.find("invoke("))?;
    quoted_value(&line[index..])
}

fn quoted_value(value: &str) -> Option<String> {
    let start = value.find(['"', '\''])?;
    let quote = value[start..].chars().next()?;
    let rest = &value[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string()).filter(|target| !target.is_empty())
}

fn contains_call_to(line: &str, name: &str) -> bool {
    if name.len() < 3 {
        return false;
    }
    let Some(index) = line.find(name) else {
        return false;
    };
    let before = line[..index].chars().last();
    let after = line[index + name.len()..].trim_start().chars().next();
    let boundary_before = before
        .map(|ch| !(ch.is_alphanumeric() || ch == '_' || ch == '.'))
        .unwrap_or(true);
    boundary_before && after == Some('(')
}

fn chunk_code_file(
    project_path: &str,
    file_path: &str,
    language: &str,
    content: &str,
    now: chrono::DateTime<Utc>,
) -> Vec<CodeChunk> {
    let lines = content.lines().collect::<Vec<_>>();
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut index = 0i64;

    while start < lines.len() {
        let end = (start + CHUNK_LINES).min(lines.len());
        let text = lines[start..end].join("\n").trim().to_string();
        if !text.is_empty() {
            chunks.push(CodeChunk {
                id: Uuid::new_v4().to_string(),
                project_path: project_path.to_string(),
                file_path: file_path.to_string(),
                language: language.to_string(),
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

fn build_entity_name_index(entities: &[CodeEntity]) -> HashMap<String, String> {
    let mut index = HashMap::new();
    for entity in entities {
        index
            .entry(entity.name.clone())
            .or_insert_with(|| entity.id.clone());
    }
    index
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
