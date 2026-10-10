use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use sqlite_vec::sqlite3_vec_init;
use std::collections::HashMap;
use std::ffi::{c_char, c_int};
use std::sync::Once;

use crate::error::{AppError, AppResult};
use crate::models::{
    CodeIndexRun, CodeSearchResult, Conversation, Item, McpServerConfig, Memory, Message,
    MessageMetadata, ModelConfig, OpsServer, RagFile,
};

mod code_index_store;
mod config_store;
mod conversation_store;
mod item_store;
mod memory_store;
pub(crate) mod profile_store;
mod project_index_store;
mod rag_store;
mod storage;
mod schema;

pub(crate) use rag_store::RagFileReplacement;

pub struct Database {
    /// Conversation, RAG, and user-profile data.
    conn: Connection,
    config_conn: Connection,
    knowledge_conn: Connection,
    project_conn: Connection,
}

pub(crate) fn register_sqlite_vec_extension() {
    static REGISTER_SQLITE_VEC: Once = Once::new();
    REGISTER_SQLITE_VEC.call_once(|| unsafe {
        type SqliteExtensionEntry = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> c_int;
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            SqliteExtensionEntry,
        >(sqlite3_vec_init as *const ())));
    });
}

impl Database {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        register_sqlite_vec_extension();
        let paths = storage::DatabasePaths::from_base_path(&path)?;
        let conn = Connection::open(&paths.conversations)?;
        let config_conn = Connection::open(&paths.config)?;
        let knowledge_conn = Connection::open(&paths.knowledge)?;
        let project_conn = Connection::open(&paths.project_index)?;
        let db = Self {
            conn,
            config_conn,
            knowledge_conn,
            project_conn,
        };
        db.initialize_split_storage()?;
        Ok(db)
    }

    fn with_savepoint<T>(
        &self,
        name: &str,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        self.conn.execute_batch(&format!("SAVEPOINT {name}"))?;
        match operation() {
            Ok(value) => {
                self.conn.execute_batch(&format!("RELEASE {name}"))?;
                Ok(value)
            }
            Err(error) => {
                let _ = self
                    .conn
                    .execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name};"));
                Err(error)
            }
        }
    }

    fn with_knowledge_savepoint<T>(
        &self,
        name: &str,
        operation: impl FnOnce() -> AppResult<T>,
    ) -> AppResult<T> {
        self.knowledge_conn
            .execute_batch(&format!("SAVEPOINT {name}"))?;
        match operation() {
            Ok(value) => {
                self.knowledge_conn
                    .execute_batch(&format!("RELEASE {name}"))?;
                Ok(value)
            }
            Err(error) => {
                let _ = self
                    .knowledge_conn
                    .execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name};"));
                Err(error)
            }
        }
    }

    fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<Item> {
        let tags_json: String = row.get(5)?;
        let created_at: String = row.get(6)?;
        let updated_at: String = row.get(7)?;

        Ok(Item {
            id: row.get(0)?,
            kind: row.get(1)?,
            title: row.get(2)?,
            body: row.get(3)?,
            status: row.get(4)?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            created_at: parse_time_for_row(&created_at)?,
            updated_at: parse_time_for_row(&updated_at)?,
        })
    }

    fn row_to_model_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelConfig> {
        let routing_tasks_json: String = row.get(17)?;
        let created_at: String = row.get(22)?;
        let updated_at: String = row.get(23)?;

        Ok(ModelConfig {
            id: row.get(0)?,
            name: row.get(1)?,
            provider: row.get(2)?,
            base_url: row.get(3)?,
            model: row.get(4)?,
            api_key: row.get(5)?,
            temperature: row.get(6)?,
            max_tokens: row.get(7)?,
            context_window: row.get(8)?,
            top_p: row.get(9)?,
            reasoning_effort: row.get(10)?,
            model_kind: row.get(11)?,
            routing_group: row.get(12)?,
            routing_enabled: row.get::<_, i64>(13)? != 0,
            routing_cost: row.get(14)?,
            routing_quality: row.get(15)?,
            routing_speed: row.get(16)?,
            routing_tasks: serde_json::from_str(&routing_tasks_json).unwrap_or_default(),
            embedding_provider: row.get(18)?,
            embedding_base_url: row.get(19)?,
            embedding_model: row.get(20)?,
            embedding_api_key: row.get(21)?,
            created_at: parse_time_for_row(&created_at)?,
            updated_at: parse_time_for_row(&updated_at)?,
        })
    }

    fn row_to_mcp_server(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpServerConfig> {
        let enabled: i64 = row.get(9)?;
        let created_at: String = row.get(10)?;
        let updated_at: String = row.get(11)?;

        Ok(McpServerConfig {
            id: row.get(0)?,
            name: row.get(1)?,
            transport: row.get(2)?,
            command: row.get(3)?,
            args_json: row.get(4)?,
            env_json: row.get(5)?,
            url: row.get(6)?,
            headers_json: row.get(7)?,
            working_dir: row.get(8)?,
            enabled: enabled == 1,
            created_at: parse_time_for_row(&created_at)?,
            updated_at: parse_time_for_row(&updated_at)?,
        })
    }

    fn row_to_ops_server(row: &rusqlite::Row<'_>) -> rusqlite::Result<OpsServer> {
        let created_at: String = row.get(9)?;
        let updated_at: String = row.get(10)?;

        Ok(OpsServer {
            id: row.get(0)?,
            name: row.get(1)?,
            host: row.get(2)?,
            port: row.get(3)?,
            username: row.get(4)?,
            auth_method: row.get(5)?,
            key_path: row.get(6)?,
            password: row.get(7)?,
            remote_dir: row.get(8)?,
            created_at: parse_time_for_row(&created_at)?,
            updated_at: parse_time_for_row(&updated_at)?,
        })
    }

    fn row_to_conversation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Conversation> {
        let archived: i64 = row.get(4)?;
        let archived_at: Option<String> = row.get(5)?;
        let created_at: String = row.get(6)?;
        let updated_at: String = row.get(7)?;

        Ok(Conversation {
            id: row.get(0)?,
            title: row.get(1)?,
            model_config_id: row.get(2)?,
            project_path: row.get(3)?,
            archived: archived == 1,
            archived_at: archived_at
                .map(|value| parse_time_for_row(&value))
                .transpose()?,
            created_at: parse_time_for_row(&created_at)?,
            updated_at: parse_time_for_row(&updated_at)?,
        })
    }

    fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
        let metadata_json: Option<String> = row.get(4)?;
        let created_at: String = row.get(5)?;

        Ok(Message {
            id: row.get(0)?,
            conversation_id: row.get(1)?,
            role: row.get(2)?,
            content: row.get(3)?,
            metadata: deserialize_metadata(metadata_json),
            created_at: parse_time_for_row(&created_at)?,
        })
    }

    fn row_to_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
        let tags_json: String = row.get(3)?;
        let enabled: i64 = row.get(4)?;
        let created_at: String = row.get(5)?;
        let updated_at: String = row.get(6)?;

        Ok(Memory {
            id: row.get(0)?,
            title: row.get(1)?,
            content: row.get(2)?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            enabled: enabled == 1,
            created_at: parse_time_for_row(&created_at)?,
            updated_at: parse_time_for_row(&updated_at)?,
        })
    }
}

fn parse_time(value: &str) -> AppResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

#[derive(Debug)]
struct MemoryGraphEntity {
    kind: &'static str,
    name: String,
    weight: f64,
}

fn extract_memory_entities(memory: &Memory) -> Vec<MemoryGraphEntity> {
    let mut entities = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |kind: &'static str, name: &str, weight: f64| {
        let name = name.trim().trim_matches(['#', '`', '"', '\'', '，', '。']);
        let normalized = normalize_entity_name(name);
        if normalized.chars().count() < 2 || !seen.insert((kind, normalized)) {
            return;
        }
        entities.push(MemoryGraphEntity {
            kind,
            name: name.chars().take(96).collect(),
            weight,
        });
    };

    push("topic", &memory.title, 1.0);
    for tag in &memory.tags {
        push("tag", tag, 1.2);
    }
    for token in memory.content.split_whitespace() {
        if token.starts_with('#') {
            push("tag", token, 1.1);
        }
    }
    for (index, segment) in memory.content.split('`').enumerate() {
        if index % 2 == 1 && segment.chars().count() <= 96 {
            push("concept", segment, 0.9);
        }
    }

    entities.truncate(16);
    entities
}

fn normalize_entity_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_lowercase()
}

fn user_profile_dimension_label(dimension: &str) -> &str {
    match dimension {
        "response-language" => "回答语言",
        "response-length" => "回答长度",
        "response-format" => "输出格式",
        "response-tone" => "表达语气",
        "response-style" => "回答方式",
        "profile-name" => "姓名",
        "profile-role" => "角色",
        "profile-workspace" => "工作目录",
        "profile-environment" => "工作环境",
        "tooling" => "常用技术",
        "project" => "长期项目",
        "workflow" => "工作方式",
        "interest" => "关注领域",
        "preference-general" => "其他偏好",
        "profile-general" => "其他画像",
        _ => "画像事实",
    }
}

fn memory_content_hash(memory: &Memory) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    memory.title.hash(&mut hasher);
    memory.content.hash(&mut hasher);
    memory.tags.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn memory_vector_table(dimensions: i64) -> AppResult<String> {
    if !(1..=65_536).contains(&dimensions) {
        return Err(AppError::Message(
            "memory embedding dimensions are invalid".to_string(),
        ));
    }
    Ok(format!("memory_vectors_{dimensions}"))
}

fn add_rrf_scores<'a>(
    scores: &mut HashMap<String, f64>,
    ids: impl Iterator<Item = &'a str>,
    weight: f64,
) {
    const RRF_K: f64 = 60.0;
    for (rank, id) in ids.enumerate() {
        *scores.entry(id.to_string()).or_default() += weight / (RRF_K + rank as f64 + 1.0);
    }
}

fn parse_time_for_row(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
        })
}

fn clean_or_default(value: String, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn clean_optional_string(value: String) -> String {
    value.trim().to_string()
}

fn build_fts_prefix_query(query: &str) -> Option<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in query.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
            continue;
        }

        push_fts_token(&mut tokens, &current);
        current.clear();
    }

    push_fts_token(&mut tokens, &current);
    tokens.sort();
    tokens.dedup();
    tokens.truncate(32);

    if tokens.is_empty() {
        return None;
    }

    Some(
        tokens
            .into_iter()
            .map(|token| format!("\"{}\"*", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn push_fts_token(tokens: &mut Vec<String>, token: &str) {
    let trimmed = token.trim();
    if trimmed.chars().count() < 2 {
        return;
    }

    tokens.push(trimmed.chars().take(64).collect());
}

fn memory_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in query.chars() {
        if ch.is_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
            continue;
        }

        push_memory_token(&mut tokens, &current);
        current.clear();
    }

    push_memory_token(&mut tokens, &current);

    tokens.sort();
    tokens.dedup();
    tokens.truncate(24);
    tokens
}

fn push_memory_token(tokens: &mut Vec<String>, token: &str) {
    let char_count = token.chars().count();
    if char_count < 2 {
        return;
    }

    tokens.push(token.to_string());
    if token.is_ascii() || char_count < 4 {
        return;
    }

    let chars = token.chars().collect::<Vec<_>>();
    for size in [2usize, 3] {
        if chars.len() < size {
            continue;
        }
        for window in chars.windows(size).take(16) {
            tokens.push(window.iter().collect());
        }
    }
}

fn score_memory_relevance(memory: &Memory, query_lower: &str, tokens: &[String]) -> i32 {
    let title = memory.title.to_lowercase();
    let content = memory.content.to_lowercase();
    let tags = memory.tags.join(" ").to_lowercase();
    let mut score = 0;

    if title.contains(query_lower) {
        score += 16;
    }
    if tags.contains(query_lower) {
        score += 12;
    }
    if content.contains(query_lower) {
        score += 8;
    }
    for token in tokens {
        if token.chars().count() < 2 {
            continue;
        }
        if title.contains(token) {
            score += 6;
        }
        if tags.contains(token) {
            score += 5;
        }
        if content.contains(token) {
            score += 2;
        }
    }

    let is_personalization = tags.contains("personalization")
        || tags.contains("preference")
        || tags.contains("profile")
        || title.contains("用户画像")
        || title.contains("工作方式");
    if is_personalization && score > 0 {
        score += 6;
    }
    if is_personalization
        && (query_lower.contains("我的偏好")
            || query_lower.contains("关于我")
            || query_lower.contains("用户画像")
            || query_lower.contains("my preference")
            || query_lower.contains("about me"))
    {
        score += 12;
    }

    score
}

fn validate_json_array_or_empty(value: &str, name: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let parsed: serde_json::Value = serde_json::from_str(trimmed)?;
    if parsed.is_array() {
        Ok(())
    } else {
        Err(AppError::Message(format!("{name} must be a JSON array")))
    }
}

fn validate_json_object_or_empty(value: &str, name: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let parsed: serde_json::Value = serde_json::from_str(trimmed)?;
    if parsed.is_object() {
        Ok(())
    } else {
        Err(AppError::Message(format!("{name} must be a JSON object")))
    }
}

fn clean_mcp_transport(value: String) -> AppResult<String> {
    let transport = clean_or_default(value, "stdio");
    match transport.as_str() {
        "stdio" | "sse" | "streamable_http" => Ok(transport),
        _ => Err(AppError::Message(
            "mcp transport must be stdio, sse, or streamable_http".to_string(),
        )),
    }
}

fn clean_ops_auth_method(value: String) -> AppResult<String> {
    let auth_method = clean_or_default(value, "key");
    match auth_method.as_str() {
        "key" | "agent" | "password" => Ok(auth_method),
        _ => Err(AppError::Message(
            "auth_method must be key, agent, or password".to_string(),
        )),
    }
}

fn row_to_rag_file(row: &rusqlite::Row<'_>) -> rusqlite::Result<RagFile> {
    let created_at: String = row.get(9)?;
    Ok(RagFile {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        name: row.get(2)?,
        mime: row.get(3)?,
        size: row.get(4)?,
        content_hash: row.get(5)?,
        chunk_count: row.get(6)?,
        status: row.get(7)?,
        error: row.get(8)?,
        created_at: parse_time_for_row(&created_at)?,
    })
}

fn row_to_code_index_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<CodeIndexRun> {
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;
    Ok(CodeIndexRun {
        id: row.get(0)?,
        project_path: row.get(1)?,
        status: row.get(2)?,
        file_count: row.get(3)?,
        entity_count: row.get(4)?,
        relation_count: row.get(5)?,
        chunk_count: row.get(6)?,
        error: row.get(7)?,
        created_at: parse_time_for_row(&created_at)?,
        updated_at: parse_time_for_row(&updated_at)?,
    })
}

fn code_search_terms(query: &str) -> Vec<String> {
    query
        .split(|ch: char| {
            !(ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/' || ch == '.')
        })
        .map(str::trim)
        .filter(|term| term.chars().count() >= 2)
        .take(8)
        .map(ToString::to_string)
        .collect()
}

fn trim_code_snippet(text: &str) -> String {
    const MAX_CHARS: usize = 900;
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_CHARS {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX_CHARS).collect::<String>() + "\n..."
}

fn dedupe_code_search_results(results: &mut Vec<CodeSearchResult>) {
    let mut seen = std::collections::HashSet::new();
    results.retain(|result| {
        seen.insert(format!(
            "{}:{}:{}:{}",
            result.file_path, result.start_line, result.end_line, result.kind
        ))
    });
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

fn encode_embedding(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>()
}

fn decode_embedding(bytes: &[u8]) -> AppResult<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return Err(AppError::Message("invalid embedding blob".to_string()));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || left.len() != right.len() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for (left_value, right_value) in left.iter().zip(right.iter()) {
        dot += left_value * right_value;
        left_norm += left_value * left_value;
        right_norm += right_value * right_value;
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn serialize_metadata(metadata: &Option<MessageMetadata>) -> AppResult<Option<String>> {
    metadata
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(AppError::from)
}

fn deserialize_metadata(metadata_json: Option<String>) -> Option<MessageMetadata> {
    metadata_json.and_then(|json| serde_json::from_str(&json).ok())
}

fn ensure_affected(affected: usize, message: &str) -> AppResult<()> {
    if affected == 0 {
        return Err(AppError::Message(message.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        ContextSummaryMetadata, ConversationDraft, MemoryDraft, MemoryPatch, MessageDraft,
        MessageMetadata, ModelConfigDraft,
    };

    #[test]
    fn model_generation_parameters_are_persisted() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let saved = db
            .save_model_config(ModelConfigDraft {
                id: Some("configured-model".to_string()),
                name: "Configured model".to_string(),
                provider: "openai-compatible".to_string(),
                base_url: "http://localhost:11434/v1".to_string(),
                model: "local-model".to_string(),
                api_key: String::new(),
                temperature: 0.65,
                max_tokens: Some(3072),
                context_window: 65_536,
                top_p: Some(0.9),
                reasoning_effort: "high".to_string(),
                model_kind: "chat".to_string(),
                routing_group: "高质量组".to_string(),
                routing_enabled: true,
                routing_cost: 5,
                routing_quality: 5,
                routing_speed: 2,
                routing_tasks: vec!["coding".to_string(), "reasoning".to_string()],
                embedding_provider: String::new(),
                embedding_base_url: String::new(),
                embedding_model: String::new(),
                embedding_api_key: String::new(),
            })
            .expect("model should save");
        let loaded = db.get_model_config(&saved.id).expect("model should load");

        assert_eq!(loaded.temperature, 0.65);
        assert_eq!(loaded.max_tokens, Some(3072));
        assert_eq!(loaded.context_window, 65_536);
        assert_eq!(loaded.top_p, Some(0.9));
        assert_eq!(loaded.routing_group, "高质量组");
        assert_eq!(loaded.routing_tasks, vec!["coding", "reasoning"]);
        assert_eq!(loaded.reasoning_effort, "high");
        assert_eq!(loaded.model_kind, "chat");
    }

    #[test]
    fn existing_model_configs_receive_generation_parameter_defaults() {
        let path = std::env::temp_dir().join(format!(
            "nanodesk-model-config-migration-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let split_paths = storage::DatabasePaths::from_base_path(&path)
            .expect("split database paths should resolve");
        {
            let conn = Connection::open(&split_paths.config)
                .expect("existing config database should open");
            conn.execute_batch(
                "
                CREATE TABLE model_configs (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    provider TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    model TEXT NOT NULL,
                    api_key TEXT NOT NULL,
                    embedding_provider TEXT NOT NULL DEFAULT 'openai-compatible',
                    embedding_base_url TEXT NOT NULL DEFAULT '',
                    embedding_model TEXT NOT NULL DEFAULT '',
                    embedding_api_key TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO model_configs
                    (id, name, provider, base_url, model, api_key, created_at, updated_at)
                VALUES
                    ('existing', 'Existing', 'openai-compatible', 'http://localhost:11434/v1',
                     'existing-model', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                INSERT INTO model_configs
                    (id, name, provider, base_url, model, api_key, created_at, updated_at)
                VALUES
                    ('embedding-config', 'Embedding', 'openai-compatible', 'http://localhost:11434/v1',
                     'bge-m3', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                ",
            )
            .expect("existing schema should be created");
        }

        let db = Database::open(path.clone()).expect("database migration should succeed");
        let loaded = db
            .get_model_config("existing")
            .expect("existing model should remain readable");
        assert_eq!(loaded.temperature, 0.4);
        assert_eq!(loaded.max_tokens, None);
        assert_eq!(loaded.context_window, 32_768);
        assert_eq!(loaded.top_p, None);
        assert_eq!(loaded.reasoning_effort, "");
        assert_eq!(loaded.model_kind, "chat");
        assert_eq!(loaded.routing_group, "默认组");
        assert!(loaded.routing_enabled);
        assert_eq!(loaded.routing_cost, 3);
        assert_eq!(loaded.routing_quality, 3);
        assert_eq!(loaded.routing_speed, 3);
        assert!(loaded.routing_tasks.is_empty());
        assert_eq!(
            db.get_model_config("embedding-config").unwrap().model_kind,
            "embedding"
        );

        drop(db);
        for split_path in [
            split_paths.config,
            split_paths.conversations,
            split_paths.knowledge,
            split_paths.project_index,
        ] {
            std::fs::remove_file(split_path).expect("split database should be removed");
        }
    }

    #[test]
    fn message_metadata_round_trips_without_removing_original_messages() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let conversation = db
            .create_conversation(ConversationDraft {
                title: Some("Summary test".to_string()),
                model_config_id: None,
                project_path: None,
            })
            .expect("conversation should be created");
        let first = db
            .append_message(MessageDraft {
                conversation_id: conversation.id.clone(),
                role: "user".to_string(),
                content: "first".to_string(),
                metadata: None,
            })
            .expect("first message should persist");
        db.append_message(MessageDraft {
            conversation_id: conversation.id.clone(),
            role: "assistant".to_string(),
            content: "second".to_string(),
            metadata: Some(MessageMetadata {
                web_search: None,
                exclude_from_profile: None,
                context_summary: None,
                generation_status: Some("interrupted".to_string()),
                assistant_reasoning: None,
            }),
        })
        .expect("second message should persist");
        db.append_message(MessageDraft {
            conversation_id: conversation.id.clone(),
            role: "system".to_string(),
            content: "structured summary".to_string(),
            metadata: Some(MessageMetadata {
                web_search: None,
                exclude_from_profile: None,
                context_summary: Some(ContextSummaryMetadata {
                    version: 1,
                    covered_through_message_id: first.id.clone(),
                    covered_message_count: 1,
                }),
                generation_status: None,
                assistant_reasoning: None,
            }),
        })
        .expect("summary should persist");

        let messages = db
            .list_messages(&conversation.id)
            .expect("messages should load");
        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages[1]
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.generation_status.as_deref()),
            Some("interrupted")
        );
        let metadata = messages[2]
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.context_summary.as_ref())
            .expect("summary metadata should load");
        assert_eq!(metadata.covered_through_message_id, first.id);
        assert_eq!(metadata.covered_message_count, 1);
    }

    #[test]
    fn fts_prefix_query_handles_punctuation_heavy_input() {
        let query = build_fts_prefix_query(r#"MCP: "stdio" path=C:\tmp\foo"#).unwrap();

        assert!(query.contains("\"mcp\"*"));
        assert!(query.contains("\"stdio\"*"));
        assert!(query.contains("\"path\"*"));
        assert!(query.contains("\"tmp\"*"));
        assert!(query.contains("\"foo\"*"));
        assert!(!query.contains("\"c\"*"));
    }

    #[test]
    fn fts_prefix_query_keeps_chinese_terms() {
        let query = build_fts_prefix_query("记忆：项目上下文、偏好").unwrap();

        assert!(query.contains("\"记忆\"*"));
        assert!(query.contains("\"项目上下文\"*"));
        assert!(query.contains("\"偏好\"*"));
    }

    #[test]
    fn fts_prefix_query_ignores_non_searchable_input() {
        assert_eq!(build_fts_prefix_query(":: -- /"), None);
    }

    #[test]
    fn memory_graph_expands_recall_through_shared_entities() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let rust = db
            .create_memory(MemoryDraft {
                title: "Rust 开发偏好".to_string(),
                content: "优先使用明确的错误类型".to_string(),
                tags: vec!["rust".to_string()],
                enabled: Some(true),
            })
            .expect("memory should be created");
        let cargo = db
            .create_memory(MemoryDraft {
                title: "Cargo 缓存".to_string(),
                content: "依赖下载使用缓存".to_string(),
                tags: vec!["rust".to_string(), "cargo".to_string()],
                enabled: Some(true),
            })
            .expect("memory should be created");

        let results = db
            .search_hybrid_memories("cargo", None, None, 10)
            .expect("hybrid search should succeed");
        let ids = results
            .iter()
            .map(|memory| memory.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&cargo.id.as_str()));
        assert!(ids.contains(&rust.id.as_str()));
    }

    #[test]
    fn memory_vectors_rank_nearest_embedding_first() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let apple = db
            .create_memory(MemoryDraft {
                title: "Apple".to_string(),
                content: "fruit".to_string(),
                tags: vec!["food".to_string()],
                enabled: Some(true),
            })
            .expect("memory should be created");
        let car = db
            .create_memory(MemoryDraft {
                title: "Car".to_string(),
                content: "vehicle".to_string(),
                tags: vec!["transport".to_string()],
                enabled: Some(true),
            })
            .expect("memory should be created");
        db.upsert_memory_embedding(&apple, "test-model", &[1.0, 0.0])
            .expect("apple embedding should be indexed");
        db.upsert_memory_embedding(&car, "test-model", &[0.0, 1.0])
            .expect("car embedding should be indexed");

        let results = db
            .search_hybrid_memories("unmatched", Some(&[0.9, 0.1]), Some("test-model"), 2)
            .expect("vector search should succeed");

        assert_eq!(
            results.first().map(|memory| memory.id.as_str()),
            Some(apple.id.as_str())
        );
    }

    #[test]
    fn updating_memory_invalidates_its_embedding_hash() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let memory = db
            .create_memory(MemoryDraft {
                title: "Preference".to_string(),
                content: "dark theme".to_string(),
                tags: vec!["ui".to_string()],
                enabled: Some(true),
            })
            .expect("memory should be created");
        db.upsert_memory_embedding(&memory, "test-model", &[1.0, 0.0])
            .expect("embedding should be indexed");
        assert!(!db
            .memory_embedding_is_stale(&memory, "test-model")
            .expect("embedding state should load"));

        let updated = db
            .update_memory(MemoryPatch {
                id: memory.id,
                title: None,
                content: Some("light theme".to_string()),
                tags: None,
                enabled: None,
            })
            .expect("memory should update");

        assert!(db
            .memory_embedding_is_stale(&updated, "test-model")
            .expect("embedding state should load"));
    }

    #[test]
    fn global_personalization_keeps_a_recall_slot_without_polluting_contextual_profile() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let global = db
            .create_memory(MemoryDraft {
                title: "回答语言".to_string(),
                content: "默认使用中文回答".to_string(),
                tags: vec![
                    "personalization".to_string(),
                    "preference".to_string(),
                    "personalization:response-language".to_string(),
                    "personalization:always".to_string(),
                ],
                enabled: Some(true),
            })
            .expect("global preference should be created");
        let contextual = db
            .create_memory(MemoryDraft {
                title: "Rust 项目".to_string(),
                content: "我主要维护 Rust 桌面应用".to_string(),
                tags: vec!["personalization".to_string(), "profile".to_string()],
                enabled: Some(true),
            })
            .expect("contextual profile should be created");

        let unrelated = db
            .search_hybrid_memories("今天的天气", None, None, 8)
            .expect("hybrid search should succeed");
        assert_eq!(
            unrelated
                .iter()
                .map(|memory| &memory.id)
                .collect::<Vec<_>>(),
            vec![&global.id]
        );

        let rust_query = db
            .search_hybrid_memories("Rust 桌面应用", None, None, 8)
            .expect("hybrid search should succeed");
        let ids = rust_query
            .iter()
            .map(|memory| memory.id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&global.id.as_str()));
        assert!(ids.contains(&contextual.id.as_str()));
    }

    #[test]
    fn legacy_personalization_memories_do_not_populate_the_new_profile() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        db.create_memory(MemoryDraft {
            title: "回答语言".to_string(),
            content: "默认使用中文回答".to_string(),
            tags: vec![
                "personalization".to_string(),
                "preference".to_string(),
                "personalization:response-language".to_string(),
                "personalization:always".to_string(),
            ],
            enabled: Some(true),
        })
        .expect("language preference should be created");

        let profile = db.get_user_profile().expect("profile should load");

        assert_eq!(profile.global_preference_count, 0);
        assert_eq!(profile.profile_fact_count, 0);
        assert!(profile.facts.is_empty());
    }

    #[test]
    fn project_paths_can_be_recovered_from_persisted_conversations() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        for project_path in [Some("D:/workspace/one"), Some("D:/workspace/two"), None] {
            db.create_conversation(ConversationDraft {
                title: None,
                model_config_id: None,
                project_path: project_path.map(str::to_string),
            })
            .expect("conversation should be created");
        }

        assert_eq!(
            db.list_conversation_project_paths().unwrap(),
            vec!["D:/workspace/one", "D:/workspace/two"]
        );
    }
}
