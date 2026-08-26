use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use sqlite_vec::sqlite3_vec_init;
use std::collections::HashMap;
use std::ffi::{c_char, c_int};
use std::sync::Once;
use uuid::Uuid;
use zerocopy::IntoBytes;

use crate::error::{AppError, AppResult};
use crate::models::{
    CodeChunk, CodeEntity, CodeIndexRun, CodeIndexStats, CodeRelation, CodeSearchResult,
    Conversation, ConversationDraft, Item, ItemDraft, ItemPatch, McpServerConfig, McpServerDraft,
    Memory, MemoryDraft, MemoryPatch, Message, MessageDraft, MessageMetadata, ModelConfig,
    ModelConfigDraft, OpsServer, OpsServerDraft, RagChunkMatch, RagFile,
};

pub(crate) mod profile_store;
mod project_index_store;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: PathBuf) -> AppResult<Self> {
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
            >(
                sqlite3_vec_init as *const ()
            )));
        });
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> AppResult<()> {
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                status TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
                id UNINDEXED,
                title,
                body,
                tags
            );

            CREATE TABLE IF NOT EXISTS model_configs (
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

            CREATE TABLE IF NOT EXISTS mcp_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                transport TEXT NOT NULL DEFAULT 'stdio',
                command TEXT NOT NULL,
                args_json TEXT NOT NULL DEFAULT '[]',
                env_json TEXT NOT NULL DEFAULT '{}',
                url TEXT NOT NULL DEFAULT '',
                headers_json TEXT NOT NULL DEFAULT '{}',
                working_dir TEXT NOT NULL DEFAULT '',
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ops_servers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                username TEXT NOT NULL,
                auth_method TEXT NOT NULL DEFAULT 'key',
                key_path TEXT NOT NULL DEFAULT '',
                password TEXT NOT NULL DEFAULT '',
                remote_dir TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model_config_id TEXT,
                project_path TEXT,
                archived INTEGER NOT NULL DEFAULT 0,
                archived_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation_created
                ON messages(conversation_id, created_at);

            CREATE TABLE IF NOT EXISTS rag_files (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                name TEXT NOT NULL,
                mime TEXT NOT NULL,
                size INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                chunk_count INTEGER NOT NULL,
                status TEXT NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS rag_chunks (
                id TEXT PRIMARY KEY,
                file_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                text TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                metadata_json TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY (file_id) REFERENCES rag_files(id) ON DELETE CASCADE,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS rag_embeddings (
                chunk_id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dim INTEGER NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES rag_chunks(id) ON DELETE CASCADE,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS rag_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                conversation_id UNINDEXED,
                file_id UNINDEXED,
                file_name,
                text
            );

            CREATE TABLE IF NOT EXISTS code_index_runs (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                status TEXT NOT NULL,
                file_count INTEGER NOT NULL,
                entity_count INTEGER NOT NULL,
                relation_count INTEGER NOT NULL,
                chunk_count INTEGER NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS code_entities (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                language TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                signature TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS code_relations (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                source_entity_id TEXT,
                source_name TEXT NOT NULL,
                target_entity_id TEXT,
                target_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS code_chunks (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                file_path TEXT NOT NULL,
                language TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                text TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS code_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                project_path UNINDEXED,
                file_path,
                language,
                text
            );

            CREATE TABLE IF NOT EXISTS code_embeddings (
                chunk_id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dim INTEGER NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES code_chunks(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS project_index_runs (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                indexer TEXT NOT NULL,
                status TEXT NOT NULL,
                file_count INTEGER NOT NULL,
                chunk_count INTEGER NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS project_index_chunks (
                id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                indexer TEXT NOT NULL,
                file_path TEXT NOT NULL,
                title TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                start_line INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                text TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                token_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS project_index_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                project_path UNINDEXED,
                indexer UNINDEXED,
                file_path,
                title,
                text
            );

            CREATE TABLE IF NOT EXISTS project_index_embeddings (
                chunk_id TEXT PRIMARY KEY,
                project_path TEXT NOT NULL,
                indexer TEXT NOT NULL,
                embedding BLOB NOT NULL,
                dim INTEGER NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (chunk_id) REFERENCES project_index_chunks(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_rag_files_conversation
                ON rag_files(conversation_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_rag_chunks_conversation
                ON rag_chunks(conversation_id, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_rag_embeddings_conversation
                ON rag_embeddings(conversation_id);

            CREATE INDEX IF NOT EXISTS idx_code_index_runs_project
                ON code_index_runs(project_path, updated_at);
            CREATE INDEX IF NOT EXISTS idx_code_entities_project_name
                ON code_entities(project_path, name);
            CREATE INDEX IF NOT EXISTS idx_code_relations_project_source
                ON code_relations(project_path, source_name);
            CREATE INDEX IF NOT EXISTS idx_code_relations_project_target
                ON code_relations(project_path, target_name);
            CREATE INDEX IF NOT EXISTS idx_code_chunks_project_file
                ON code_chunks(project_path, file_path, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_code_embeddings_project
                ON code_embeddings(project_path);
            CREATE INDEX IF NOT EXISTS idx_project_index_runs_project
                ON project_index_runs(project_path, indexer, updated_at);
            CREATE INDEX IF NOT EXISTS idx_project_index_chunks_project
                ON project_index_chunks(project_path, indexer, file_path, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_project_index_embeddings_project
                ON project_index_embeddings(project_path, indexer);

            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id UNINDEXED,
                title,
                content,
                tags
            );

            CREATE TABLE IF NOT EXISTS memory_embeddings (
                vector_rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id TEXT NOT NULL UNIQUE,
                model TEXT NOT NULL,
                dimensions INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                indexed_at TEXT NOT NULL,
                FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS memory_entities (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                normalized_name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(kind, normalized_name)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS memory_entities_fts USING fts5(
                id UNINDEXED,
                name
            );

            CREATE TABLE IF NOT EXISTS memory_entity_links (
                memory_id TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 1,
                PRIMARY KEY (memory_id, entity_id),
                FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE,
                FOREIGN KEY (entity_id) REFERENCES memory_entities(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS memory_relations (
                source_entity_id TEXT NOT NULL,
                target_entity_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                weight REAL NOT NULL DEFAULT 1,
                evidence_memory_id TEXT NOT NULL,
                PRIMARY KEY (source_entity_id, target_entity_id, kind, evidence_memory_id),
                FOREIGN KEY (source_entity_id) REFERENCES memory_entities(id) ON DELETE CASCADE,
                FOREIGN KEY (target_entity_id) REFERENCES memory_entities(id) ON DELETE CASCADE,
                FOREIGN KEY (evidence_memory_id) REFERENCES memories(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS profile_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                profile_generation INTEGER NOT NULL DEFAULT 1,
                next_event_revision INTEGER NOT NULL DEFAULT 0,
                skipped_observation_count INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profile_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                enabled INTEGER NOT NULL DEFAULT 0,
                model_config_id TEXT,
                character_threshold INTEGER NOT NULL DEFAULT 3000,
                idle_seconds INTEGER NOT NULL DEFAULT 1800,
                max_wait_seconds INTEGER NOT NULL DEFAULT 86400,
                long_input_threshold INTEGER NOT NULL DEFAULT 8000,
                rolling_hour_attempt_limit INTEGER NOT NULL DEFAULT 2,
                rolling_day_attempt_limit INTEGER NOT NULL DEFAULT 8,
                rolling_day_candidate_character_limit INTEGER NOT NULL DEFAULT 30000,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS profile_observations (
                id TEXT PRIMARY KEY,
                source_message_id TEXT NOT NULL UNIQUE,
                conversation_id TEXT NOT NULL,
                raw_character_count INTEGER NOT NULL,
                candidate_character_count INTEGER NOT NULL DEFAULT 0,
                candidate_hash TEXT NOT NULL DEFAULT '',
                candidate_kind TEXT NOT NULL DEFAULT 'implicit',
                input_kind TEXT NOT NULL DEFAULT 'normal',
                cleaner_version TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                skip_reason TEXT,
                profile_generation INTEGER NOT NULL,
                observation_revision INTEGER NOT NULL UNIQUE,
                preprocess_lease_owner TEXT,
                preprocess_lease_expires_at TEXT,
                preprocess_lease_epoch INTEGER NOT NULL DEFAULT 0,
                observed_at TEXT NOT NULL,
                processed_at TEXT,
                FOREIGN KEY (source_message_id) REFERENCES messages(id) ON DELETE CASCADE,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS profile_extraction_batches (
                id TEXT PRIMARY KEY,
                trigger_kind TEXT NOT NULL,
                model_config_id TEXT,
                model_provider_snapshot TEXT,
                model_base_url_snapshot TEXT,
                model_name_snapshot TEXT,
                model_config_hash TEXT,
                profile_generation INTEGER NOT NULL,
                status TEXT NOT NULL,
                observation_count INTEGER NOT NULL,
                input_character_count INTEGER NOT NULL,
                estimated_input_tokens INTEGER NOT NULL DEFAULT 0,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                available_at TEXT NOT NULL,
                lease_owner TEXT,
                lease_expires_at TEXT,
                lease_epoch INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS profile_batch_observations (
                batch_id TEXT NOT NULL,
                observation_id TEXT NOT NULL,
                batch_index INTEGER NOT NULL,
                PRIMARY KEY (batch_id, observation_id),
                FOREIGN KEY (batch_id) REFERENCES profile_extraction_batches(id) ON DELETE CASCADE,
                FOREIGN KEY (observation_id) REFERENCES profile_observations(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS user_profile_facts (
                id TEXT PRIMARY KEY,
                dimension TEXT NOT NULL,
                normalized_value TEXT NOT NULL,
                display_value TEXT NOT NULL,
                category TEXT NOT NULL,
                global INTEGER NOT NULL,
                confidence REAL NOT NULL,
                extractor_model_config_id TEXT NOT NULL,
                last_observation_revision INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE (dimension, normalized_value)
            );

            CREATE TABLE IF NOT EXISTS user_profile_fact_sources (
                fact_id TEXT NOT NULL,
                observation_id TEXT,
                source_message_id TEXT,
                created_at TEXT NOT NULL,
                PRIMARY KEY (fact_id, observation_id),
                FOREIGN KEY (fact_id) REFERENCES user_profile_facts(id) ON DELETE CASCADE,
                FOREIGN KEY (observation_id) REFERENCES profile_observations(id) ON DELETE SET NULL,
                FOREIGN KEY (source_message_id) REFERENCES messages(id) ON DELETE SET NULL
            );

            CREATE TABLE IF NOT EXISTS profile_fact_tombstones (
                dimension TEXT NOT NULL,
                normalized_value TEXT NOT NULL,
                delete_revision INTEGER NOT NULL,
                deleted_at TEXT NOT NULL,
                PRIMARY KEY (dimension, normalized_value)
            );

            CREATE TABLE IF NOT EXISTS profile_batch_commits (
                batch_id TEXT PRIMARY KEY,
                lease_epoch INTEGER NOT NULL,
                applied_at TEXT NOT NULL,
                FOREIGN KEY (batch_id) REFERENCES profile_extraction_batches(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS profile_foreground_leases (
                owner TEXT PRIMARY KEY,
                expires_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS profile_usage_attempts (
                id TEXT PRIMARY KEY,
                batch_id TEXT,
                started_at_utc TEXT NOT NULL,
                candidate_character_count INTEGER NOT NULL,
                estimated_input_tokens INTEGER NOT NULL,
                actual_input_tokens INTEGER NOT NULL DEFAULT 0,
                actual_output_tokens INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (batch_id) REFERENCES profile_extraction_batches(id) ON DELETE SET NULL
            );

            CREATE INDEX IF NOT EXISTS idx_memory_embeddings_model
                ON memory_embeddings(model, dimensions);
            CREATE INDEX IF NOT EXISTS idx_memory_entity_links_entity
                ON memory_entity_links(entity_id, memory_id);
            CREATE INDEX IF NOT EXISTS idx_memory_relations_source
                ON memory_relations(source_entity_id, target_entity_id);
            CREATE INDEX IF NOT EXISTS idx_memory_relations_target
                ON memory_relations(target_entity_id, source_entity_id);
            CREATE INDEX IF NOT EXISTS idx_profile_observations_status_revision
                ON profile_observations(status, profile_generation, observation_revision);
            CREATE INDEX IF NOT EXISTS idx_profile_batches_status_available
                ON profile_extraction_batches(status, available_at);
            CREATE INDEX IF NOT EXISTS idx_profile_usage_started
                ON profile_usage_attempts(started_at_utc);
            CREATE INDEX IF NOT EXISTS idx_profile_fact_dimension
                ON user_profile_facts(dimension, updated_at);
            ",
        )?;
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR IGNORE INTO profile_state (id, profile_generation, next_event_revision, updated_at) VALUES (1, 1, 0, ?1)",
            params![now],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO profile_settings
                (id, enabled, model_config_id, character_threshold, idle_seconds,
                 max_wait_seconds, long_input_threshold, rolling_hour_attempt_limit,
                 rolling_day_attempt_limit, rolling_day_candidate_character_limit, updated_at)
             VALUES (1, 0, NULL, 3000, 1800, 86400, 8000, 2, 8, 30000, ?1)",
            params![now],
        )?;
        self.ensure_column("conversations", "project_path", "TEXT")?;
        self.ensure_column("conversations", "archived", "INTEGER NOT NULL DEFAULT 0")?;
        self.ensure_column("conversations", "archived_at", "TEXT")?;
        self.ensure_column("messages", "metadata_json", "TEXT")?;
        self.ensure_column(
            "profile_state",
            "skipped_observation_count",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        self.ensure_column(
            "model_configs",
            "embedding_provider",
            "TEXT NOT NULL DEFAULT 'openai-compatible'",
        )?;
        self.ensure_column(
            "model_configs",
            "embedding_base_url",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        self.ensure_column(
            "model_configs",
            "embedding_model",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        self.ensure_column(
            "model_configs",
            "embedding_api_key",
            "TEXT NOT NULL DEFAULT ''",
        )?;
        self.ensure_column("mcp_servers", "transport", "TEXT NOT NULL DEFAULT 'stdio'")?;
        self.ensure_column("mcp_servers", "args_json", "TEXT NOT NULL DEFAULT '[]'")?;
        self.ensure_column("mcp_servers", "env_json", "TEXT NOT NULL DEFAULT '{}'")?;
        self.ensure_column("mcp_servers", "url", "TEXT NOT NULL DEFAULT ''")?;
        self.ensure_column("mcp_servers", "headers_json", "TEXT NOT NULL DEFAULT '{}'")?;
        self.ensure_column("mcp_servers", "working_dir", "TEXT NOT NULL DEFAULT ''")?;
        self.ensure_column("mcp_servers", "enabled", "INTEGER NOT NULL DEFAULT 1")?;
        self.ensure_column("ops_servers", "auth_method", "TEXT NOT NULL DEFAULT 'key'")?;
        self.ensure_column("ops_servers", "key_path", "TEXT NOT NULL DEFAULT ''")?;
        self.ensure_column("ops_servers", "password", "TEXT NOT NULL DEFAULT ''")?;
        self.ensure_column("ops_servers", "remote_dir", "TEXT NOT NULL DEFAULT ''")?;
        self.rebuild_missing_memory_graphs()?;
        Ok(())
    }

    fn ensure_column(&self, table: &str, column: &str, definition: &str) -> AppResult<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;

        if !columns.iter().any(|name| name == column) {
            self.conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )?;
        }

        Ok(())
    }

    pub fn list_items(&self, kind: Option<&str>) -> AppResult<Vec<Item>> {
        let sql = match kind {
            Some(_) => {
                "SELECT id, kind, title, body, status, tags_json, created_at, updated_at
                 FROM items WHERE kind = ?1 ORDER BY updated_at DESC"
            }
            None => {
                "SELECT id, kind, title, body, status, tags_json, created_at, updated_at
                 FROM items ORDER BY updated_at DESC"
            }
        };

        let mut stmt = self.conn.prepare(sql)?;
        let rows: Result<Vec<_>, _> = match kind {
            Some(kind) => stmt.query_map([kind], Self::row_to_item)?.collect(),
            None => stmt.query_map([], Self::row_to_item)?.collect(),
        };

        rows.map_err(AppError::from)
    }

    pub fn search_items(&self, query: &str) -> AppResult<Vec<Item>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_items(None);
        }

        let Some(fts_query) = build_fts_prefix_query(trimmed) else {
            return Ok(Vec::new());
        };

        let mut stmt = self.conn.prepare(
            "
            SELECT i.id, i.kind, i.title, i.body, i.status, i.tags_json, i.created_at, i.updated_at
            FROM items_fts f
            JOIN items i ON i.id = f.id
            WHERE items_fts MATCH ?1
            ORDER BY rank
            LIMIT 100
            ",
        )?;

        let rows = stmt
            .query_map([fts_query], Self::row_to_item)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn create_item(&self, draft: ItemDraft) -> AppResult<Item> {
        let now = Utc::now();
        let item = Item {
            id: Uuid::new_v4().to_string(),
            kind: clean_or_default(draft.kind, "note"),
            title: clean_or_default(draft.title, "未命名"),
            body: draft.body,
            status: draft.status.unwrap_or_else(|| "active".to_string()),
            tags: draft.tags,
            created_at: now,
            updated_at: now,
        };

        self.upsert_item(&item)?;
        Ok(item)
    }

    pub fn update_item(&self, patch: ItemPatch) -> AppResult<Item> {
        let current = self
            .get_item(&patch.id)?
            .ok_or_else(|| AppError::Message("item not found".to_string()))?;

        let item = Item {
            id: current.id,
            kind: patch.kind.unwrap_or(current.kind),
            title: patch.title.unwrap_or(current.title),
            body: patch.body.unwrap_or(current.body),
            status: patch.status.unwrap_or(current.status),
            tags: patch.tags.unwrap_or(current.tags),
            created_at: current.created_at,
            updated_at: Utc::now(),
        };

        self.upsert_item(&item)?;
        Ok(item)
    }

    pub fn delete_item(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM items_fts WHERE id = ?1", params![id])?;
        let affected = self
            .conn
            .execute("DELETE FROM items WHERE id = ?1", params![id])?;
        ensure_affected(affected, "item not found")?;
        Ok(())
    }

    pub fn list_model_configs(&self) -> AppResult<Vec<ModelConfig>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, provider, base_url, model, api_key,
                   embedding_provider, embedding_base_url, embedding_model, embedding_api_key,
                   created_at, updated_at
            FROM model_configs
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_model_config)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn get_model_config(&self, id: &str) -> AppResult<ModelConfig> {
        self.conn
            .query_row(
                "
                SELECT id, name, provider, base_url, model, api_key,
                       embedding_provider, embedding_base_url, embedding_model, embedding_api_key,
                       created_at, updated_at
                FROM model_configs WHERE id = ?1
                ",
                params![id],
                Self::row_to_model_config,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("model config not found".to_string()))
    }

    pub fn save_model_config(&self, draft: ModelConfigDraft) -> AppResult<ModelConfig> {
        let now = Utc::now();
        let id = draft.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = self
            .conn
            .query_row(
                "SELECT created_at FROM model_configs WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| parse_time(&value))
            .transpose()?
            .unwrap_or(now);

        let config = ModelConfig {
            id,
            name: clean_or_default(draft.name, "默认模型"),
            provider: clean_or_default(draft.provider, "openai-compatible"),
            base_url: clean_or_default(draft.base_url, "https://api.openai.com/v1"),
            model: clean_or_default(draft.model, "gpt-4o-mini"),
            api_key: draft.api_key,
            embedding_provider: clean_or_default(draft.embedding_provider, "openai-compatible"),
            embedding_base_url: clean_optional_string(draft.embedding_base_url),
            embedding_model: clean_or_default(draft.embedding_model, "text-embedding-3-small"),
            embedding_api_key: clean_optional_string(draft.embedding_api_key),
            created_at,
            updated_at: now,
        };

        self.conn.execute(
            "
            INSERT INTO model_configs
                (id, name, provider, base_url, model, api_key,
                 embedding_provider, embedding_base_url, embedding_model, embedding_api_key,
                 created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                base_url = excluded.base_url,
                model = excluded.model,
                api_key = excluded.api_key,
                embedding_provider = excluded.embedding_provider,
                embedding_base_url = excluded.embedding_base_url,
                embedding_model = excluded.embedding_model,
                embedding_api_key = excluded.embedding_api_key,
                updated_at = excluded.updated_at
            ",
            params![
                config.id,
                config.name,
                config.provider,
                config.base_url,
                config.model,
                config.api_key,
                config.embedding_provider,
                config.embedding_base_url,
                config.embedding_model,
                config.embedding_api_key,
                config.created_at.to_rfc3339(),
                config.updated_at.to_rfc3339()
            ],
        )?;

        Ok(config)
    }

    pub fn delete_model_config(&self, id: &str) -> AppResult<()> {
        let affected = self
            .conn
            .execute("DELETE FROM model_configs WHERE id = ?1", params![id])?;
        ensure_affected(affected, "model config not found")?;
        Ok(())
    }

    pub fn list_mcp_servers(&self) -> AppResult<Vec<McpServerConfig>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, transport, command, args_json, env_json, url, headers_json,
                   working_dir, enabled, created_at, updated_at
            FROM mcp_servers
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_mcp_server)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn get_mcp_server(&self, id: &str) -> AppResult<McpServerConfig> {
        self.conn
            .query_row(
                "
                SELECT id, name, transport, command, args_json, env_json, url, headers_json,
                       working_dir, enabled, created_at, updated_at
                FROM mcp_servers WHERE id = ?1
                ",
                params![id],
                Self::row_to_mcp_server,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("mcp server not found".to_string()))
    }

    pub fn save_mcp_server(&self, draft: McpServerDraft) -> AppResult<McpServerConfig> {
        let now = Utc::now();
        let id = draft.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = self
            .conn
            .query_row(
                "SELECT created_at FROM mcp_servers WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| parse_time(&value))
            .transpose()?
            .unwrap_or(now);

        validate_json_array_or_empty(&draft.args_json, "args_json")?;
        validate_json_object_or_empty(&draft.env_json, "env_json")?;
        validate_json_object_or_empty(&draft.headers_json, "headers_json")?;

        let server = McpServerConfig {
            id,
            name: clean_or_default(draft.name, "MCP Server"),
            transport: clean_mcp_transport(draft.transport)?,
            command: clean_optional_string(draft.command),
            args_json: clean_or_default(draft.args_json, "[]"),
            env_json: clean_or_default(draft.env_json, "{}"),
            url: clean_optional_string(draft.url),
            headers_json: clean_or_default(draft.headers_json, "{}"),
            working_dir: clean_optional_string(draft.working_dir),
            enabled: draft.enabled,
            created_at,
            updated_at: now,
        };
        if server.transport == "stdio" && server.command.is_empty() {
            return Err(AppError::Message(
                "mcp server command is required".to_string(),
            ));
        }
        if server.transport != "stdio" && server.url.is_empty() {
            return Err(AppError::Message("mcp server url is required".to_string()));
        }

        self.conn.execute(
            "
            INSERT INTO mcp_servers
                (id, name, transport, command, args_json, env_json, url, headers_json,
                 working_dir, enabled, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                transport = excluded.transport,
                command = excluded.command,
                args_json = excluded.args_json,
                env_json = excluded.env_json,
                url = excluded.url,
                headers_json = excluded.headers_json,
                working_dir = excluded.working_dir,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            ",
            params![
                server.id,
                server.name,
                server.transport,
                server.command,
                server.args_json,
                server.env_json,
                server.url,
                server.headers_json,
                server.working_dir,
                if server.enabled { 1 } else { 0 },
                server.created_at.to_rfc3339(),
                server.updated_at.to_rfc3339()
            ],
        )?;

        Ok(server)
    }

    pub fn delete_mcp_server(&self, id: &str) -> AppResult<()> {
        let affected = self
            .conn
            .execute("DELETE FROM mcp_servers WHERE id = ?1", params![id])?;
        ensure_affected(affected, "mcp server not found")?;
        Ok(())
    }

    pub fn list_ops_servers(&self) -> AppResult<Vec<OpsServer>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, host, port, username, auth_method, key_path, password,
                   remote_dir, created_at, updated_at
            FROM ops_servers
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_ops_server)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn get_ops_server(&self, id: &str) -> AppResult<OpsServer> {
        self.conn
            .query_row(
                "
                SELECT id, name, host, port, username, auth_method, key_path, password,
                       remote_dir, created_at, updated_at
                FROM ops_servers WHERE id = ?1
                ",
                params![id],
                Self::row_to_ops_server,
            )
            .optional()?
            .ok_or_else(|| AppError::Message("server not found".to_string()))
    }

    pub fn save_ops_server(&self, draft: OpsServerDraft) -> AppResult<OpsServer> {
        let now = Utc::now();
        let id = draft.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let created_at = self
            .conn
            .query_row(
                "SELECT created_at FROM ops_servers WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|value| parse_time(&value))
            .transpose()?
            .unwrap_or(now);

        let port = draft.port.unwrap_or(22).clamp(1, 65535);
        let server = OpsServer {
            id,
            name: clean_or_default(draft.name, "未命名服务器"),
            host: clean_optional_string(draft.host),
            port,
            username: clean_optional_string(draft.username),
            auth_method: clean_ops_auth_method(draft.auth_method)?,
            key_path: clean_optional_string(draft.key_path),
            password: draft.password,
            remote_dir: clean_optional_string(draft.remote_dir),
            created_at,
            updated_at: now,
        };

        if server.host.is_empty() {
            return Err(AppError::Message("服务器地址不能为空".to_string()));
        }
        if server.username.is_empty() {
            return Err(AppError::Message("用户名不能为空".to_string()));
        }

        self.conn.execute(
            "
            INSERT INTO ops_servers
                (id, name, host, port, username, auth_method, key_path, password,
                 remote_dir, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                port = excluded.port,
                username = excluded.username,
                auth_method = excluded.auth_method,
                key_path = excluded.key_path,
                password = excluded.password,
                remote_dir = excluded.remote_dir,
                updated_at = excluded.updated_at
            ",
            params![
                server.id,
                server.name,
                server.host,
                server.port,
                server.username,
                server.auth_method,
                server.key_path,
                server.password,
                server.remote_dir,
                server.created_at.to_rfc3339(),
                server.updated_at.to_rfc3339()
            ],
        )?;

        Ok(server)
    }

    pub fn delete_ops_server(&self, id: &str) -> AppResult<()> {
        let affected = self
            .conn
            .execute("DELETE FROM ops_servers WHERE id = ?1", params![id])?;
        ensure_affected(affected, "server not found")?;
        Ok(())
    }

    pub fn list_conversations(&self, project_path: Option<&str>) -> AppResult<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, model_config_id, project_path, archived, archived_at, created_at, updated_at
            FROM conversations
            WHERE archived = 0
              AND (
                (?1 IS NULL AND project_path IS NULL)
                OR project_path = ?1
              )
            ORDER BY updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([project_path], Self::row_to_conversation)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn list_archived_conversations(
        &self,
        project_path: Option<&str>,
    ) -> AppResult<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, model_config_id, project_path, archived, archived_at, created_at, updated_at
            FROM conversations
            WHERE archived = 1
              AND (
                (?1 IS NULL AND project_path IS NULL)
                OR project_path = ?1
              )
            ORDER BY COALESCE(archived_at, updated_at) DESC
            ",
        )?;

        let rows = stmt
            .query_map([project_path], Self::row_to_conversation)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn create_conversation(&self, draft: ConversationDraft) -> AppResult<Conversation> {
        let now = Utc::now();
        let conversation = Conversation {
            id: Uuid::new_v4().to_string(),
            title: clean_or_default(draft.title.unwrap_or_default(), "New chat"),
            model_config_id: draft.model_config_id,
            project_path: draft.project_path,
            archived: false,
            archived_at: None,
            created_at: now,
            updated_at: now,
        };

        self.conn.execute(
            "
            INSERT INTO conversations
                (id, title, model_config_id, project_path, archived, archived_at, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                conversation.id,
                conversation.title,
                conversation.model_config_id,
                conversation.project_path,
                if conversation.archived { 1 } else { 0 },
                conversation.archived_at.map(|time| time.to_rfc3339()),
                conversation.created_at.to_rfc3339(),
                conversation.updated_at.to_rfc3339()
            ],
        )?;

        Ok(conversation)
    }

    pub fn archive_conversation(&self, id: &str, archived: bool) -> AppResult<()> {
        let now = Utc::now();
        let affected = self.conn.execute(
            "
            UPDATE conversations
            SET archived = ?2,
                archived_at = ?3,
                updated_at = ?4
            WHERE id = ?1
            ",
            params![
                id,
                if archived { 1 } else { 0 },
                if archived {
                    Some(now.to_rfc3339())
                } else {
                    None
                },
                now.to_rfc3339()
            ],
        )?;
        ensure_affected(affected, "conversation not found")?;
        Ok(())
    }

    pub fn rename_conversation(&self, id: &str, title: &str) -> AppResult<()> {
        let now = Utc::now();
        let affected = self.conn.execute(
            "
            UPDATE conversations
            SET title = ?2,
                updated_at = ?3
            WHERE id = ?1
            ",
            params![id, title, now.to_rfc3339()],
        )?;
        ensure_affected(affected, "conversation not found")?;
        Ok(())
    }

    pub fn update_conversation_model(
        &self,
        id: &str,
        model_config_id: Option<&str>,
    ) -> AppResult<()> {
        let now = Utc::now();
        let affected = self.conn.execute(
            "
            UPDATE conversations
            SET model_config_id = ?2,
                updated_at = ?3
            WHERE id = ?1
            ",
            params![id, model_config_id, now.to_rfc3339()],
        )?;
        ensure_affected(affected, "conversation not found")?;
        Ok(())
    }

    pub fn delete_conversation(&self, id: &str) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM rag_chunks_fts WHERE conversation_id = ?1",
            params![id],
        )?;
        let affected = self
            .conn
            .execute("DELETE FROM conversations WHERE id = ?1", params![id])?;
        ensure_affected(affected, "conversation not found")?;
        Ok(())
    }

    pub fn list_messages(&self, conversation_id: &str) -> AppResult<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, conversation_id, role, content, metadata_json, created_at
            FROM messages
            WHERE conversation_id = ?1
            ORDER BY created_at ASC
            ",
        )?;

        let rows = stmt
            .query_map([conversation_id], Self::row_to_message)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn append_message(&self, draft: MessageDraft) -> AppResult<Message> {
        let now = Utc::now();
        let message = Message {
            id: Uuid::new_v4().to_string(),
            conversation_id: draft.conversation_id,
            role: clean_or_default(draft.role, "user"),
            content: draft.content,
            metadata: draft.metadata,
            created_at: now,
        };

        self.with_savepoint("message_append", || {
            self.conn.execute(
                "
                INSERT INTO messages (id, conversation_id, role, content, metadata_json, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    message.id,
                    message.conversation_id,
                    message.role,
                    message.content,
                    serialize_metadata(&message.metadata)?,
                    message.created_at.to_rfc3339()
                ],
            )?;

            self.archive_conversation(&message.conversation_id, false)?;

            let title = message
                .content
                .chars()
                .take(30)
                .collect::<String>()
                .trim()
                .to_string();
            if message.role == "user" && !title.is_empty() {
                self.conn.execute(
                    "
                    UPDATE conversations
                    SET title = CASE WHEN title = 'New chat' THEN ?2 ELSE title END,
                        updated_at = ?3
                    WHERE id = ?1
                    ",
                    params![message.conversation_id, title, now.to_rfc3339()],
                )?;
            } else {
                self.conn.execute(
                    "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
                    params![message.conversation_id, now.to_rfc3339()],
                )?;
            }

            self.collect_profile_observation(&message)
        })?;

        Ok(message)
    }

    fn collect_profile_observation(&self, message: &Message) -> AppResult<()> {
        if message.role != "user" {
            return Ok(());
        }
        let enabled = self.conn.query_row(
            "SELECT enabled FROM profile_settings WHERE id = 1",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        if !enabled {
            return Ok(());
        }

        let now = message.created_at.to_rfc3339();
        self.conn.execute(
            "UPDATE profile_state
             SET next_event_revision = next_event_revision + 1, updated_at = ?1
             WHERE id = 1",
            params![now],
        )?;
        let (generation, revision) = self.conn.query_row(
            "SELECT profile_generation, next_event_revision FROM profile_state WHERE id = 1",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO profile_observations
                (id, source_message_id, conversation_id, raw_character_count, status,
                 profile_generation, observation_revision, observed_at)
             VALUES (?1, ?2, ?3, ?4, 'pending_preprocess', ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                message.id,
                message.conversation_id,
                message.content.len() as i64,
                generation,
                revision,
                now
            ],
        )?;
        Ok(())
    }

    pub fn delete_messages(&self, ids: &[String]) -> AppResult<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("DELETE FROM messages WHERE id IN ({})", placeholders);
        let mut stmt = self.conn.prepare(&query)?;
        let params = rusqlite::params_from_iter(ids);
        let affected = stmt.execute(params)?;
        if affected != ids.len() {
            return Err(AppError::Message("message not found".to_string()));
        }
        Ok(())
    }

    pub fn list_rag_files(&self, conversation_id: &str) -> AppResult<Vec<RagFile>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, conversation_id, name, mime, size, content_hash, chunk_count,
                   status, error, created_at
            FROM rag_files
            WHERE conversation_id = ?1
            ORDER BY created_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([conversation_id], row_to_rag_file)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn get_rag_file(&self, id: &str) -> AppResult<RagFile> {
        self.conn
            .query_row(
                "
                SELECT id, conversation_id, name, mime, size, content_hash, chunk_count,
                       status, error, created_at
                FROM rag_files
                WHERE id = ?1
                ",
                params![id],
                row_to_rag_file,
            )
            .map_err(AppError::from)
    }

    pub fn delete_rag_file(&self, id: &str) -> AppResult<()> {
        let file = self.get_rag_file(id)?;
        self.conn
            .execute("DELETE FROM rag_chunks_fts WHERE file_id = ?1", params![id])?;
        let affected = self
            .conn
            .execute("DELETE FROM rag_files WHERE id = ?1", params![id])?;
        ensure_affected(affected, "rag file not found")?;
        let _ = file;
        Ok(())
    }

    pub fn replace_rag_file(
        &self,
        conversation_id: &str,
        name: &str,
        mime: &str,
        size: i64,
        content_hash: &str,
        chunks: &[String],
        embeddings: &[Vec<f32>],
        embedding_model: &str,
    ) -> AppResult<RagFile> {
        if chunks.is_empty() {
            return Err(AppError::Message("文件没有可索引文本".to_string()));
        }
        if chunks.len() != embeddings.len() {
            return Err(AppError::Message(
                "chunk 与 embedding 数量不一致".to_string(),
            ));
        }

        let existing_id: Option<String> = self
            .conn
            .query_row(
                "
                SELECT id FROM rag_files
                WHERE conversation_id = ?1 AND content_hash = ?2
                ",
                params![conversation_id, content_hash],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(existing_id) = existing_id {
            self.delete_rag_file(&existing_id)?;
        }

        let now = Utc::now();
        let file = RagFile {
            id: Uuid::new_v4().to_string(),
            conversation_id: conversation_id.to_string(),
            name: clean_or_default(name.to_string(), "uploaded.txt"),
            mime: clean_optional_string(mime.to_string()),
            size,
            content_hash: content_hash.to_string(),
            chunk_count: chunks.len() as i64,
            status: "ready".to_string(),
            error: None,
            created_at: now,
        };

        self.conn.execute(
            "
            INSERT INTO rag_files
                (id, conversation_id, name, mime, size, content_hash, chunk_count,
                 status, error, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                file.id,
                file.conversation_id,
                file.name,
                file.mime,
                file.size,
                file.content_hash,
                file.chunk_count,
                file.status,
                file.error,
                file.created_at.to_rfc3339()
            ],
        )?;

        for (index, (text, embedding)) in chunks.iter().zip(embeddings.iter()).enumerate() {
            let chunk_id = Uuid::new_v4().to_string();
            self.conn.execute(
                "
                INSERT INTO rag_chunks
                    (id, file_id, conversation_id, chunk_index, text, token_count,
                     metadata_json, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    chunk_id,
                    file.id,
                    file.conversation_id,
                    index as i64,
                    text,
                    estimate_token_count(text),
                    serde_json::json!({ "file_name": file.name }).to_string(),
                    now.to_rfc3339()
                ],
            )?;
            self.conn.execute(
                "
                INSERT INTO rag_embeddings
                    (chunk_id, conversation_id, embedding, dim, model, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ",
                params![
                    chunk_id,
                    file.conversation_id,
                    encode_embedding(embedding),
                    embedding.len() as i64,
                    embedding_model,
                    now.to_rfc3339()
                ],
            )?;
            self.conn.execute(
                "
                INSERT INTO rag_chunks_fts
                    (chunk_id, conversation_id, file_id, file_name, text)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![chunk_id, file.conversation_id, file.id, file.name, text],
            )?;
        }

        Ok(file)
    }

    pub fn search_rag_chunks(
        &self,
        conversation_id: &str,
        query_embedding: &[f32],
        limit: i64,
    ) -> AppResult<Vec<RagChunkMatch>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT chunks.id, chunks.file_id, files.name, chunks.chunk_index, chunks.text,
                   embeddings.embedding
            FROM rag_chunks chunks
            JOIN rag_files files ON files.id = chunks.file_id
            JOIN rag_embeddings embeddings ON embeddings.chunk_id = chunks.id
            WHERE chunks.conversation_id = ?1
            ",
        )?;

        let mut rows = stmt.query(params![conversation_id])?;
        let mut matches = Vec::new();
        while let Some(row) = rows.next()? {
            let embedding_blob: Vec<u8> = row.get(5)?;
            let embedding = decode_embedding(&embedding_blob)?;
            let score = cosine_similarity(query_embedding, &embedding);
            matches.push(RagChunkMatch {
                chunk_id: row.get(0)?,
                file_id: row.get(1)?,
                file_name: row.get(2)?,
                chunk_index: row.get(3)?,
                text: row.get(4)?,
                score,
            });
        }

        matches.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(limit.clamp(1, 20) as usize);
        Ok(matches)
    }

    pub fn replace_code_index(
        &self,
        project_path: &str,
        file_count: i64,
        entities: &[CodeEntity],
        relations: &[CodeRelation],
        chunks: &[CodeChunk],
        chunk_embeddings: Option<(&[Vec<f32>], &str)>,
    ) -> AppResult<CodeIndexRun> {
        let now = Utc::now();
        if let Some((embeddings, _)) = chunk_embeddings {
            if embeddings.len() != chunks.len() {
                return Err(AppError::Message(
                    "代码 chunk 与 embedding 数量不一致".to_string(),
                ));
            }
        }
        self.conn.execute(
            "DELETE FROM code_chunks_fts WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_embeddings WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_relations WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_entities WHERE project_path = ?1",
            params![project_path],
        )?;
        self.conn.execute(
            "DELETE FROM code_chunks WHERE project_path = ?1",
            params![project_path],
        )?;

        for entity in entities {
            self.conn.execute(
                "
                INSERT INTO code_entities
                    (id, project_path, file_path, name, kind, language, start_line,
                     end_line, signature, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    &entity.id,
                    project_path,
                    &entity.file_path,
                    &entity.name,
                    &entity.kind,
                    &entity.language,
                    entity.start_line,
                    entity.end_line,
                    &entity.signature,
                    now.to_rfc3339()
                ],
            )?;
        }

        for relation in relations {
            self.conn.execute(
                "
                INSERT INTO code_relations
                    (id, project_path, source_entity_id, source_name, target_entity_id,
                     target_name, kind, file_path, line, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ",
                params![
                    &relation.id,
                    project_path,
                    &relation.source_entity_id,
                    &relation.source_name,
                    &relation.target_entity_id,
                    &relation.target_name,
                    &relation.kind,
                    &relation.file_path,
                    relation.line,
                    now.to_rfc3339()
                ],
            )?;
        }

        for (chunk_index, chunk) in chunks.iter().enumerate() {
            self.conn.execute(
                "
                INSERT INTO code_chunks
                    (id, project_path, file_path, language, chunk_index, start_line,
                     end_line, text, content_hash, token_count, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                params![
                    &chunk.id,
                    project_path,
                    &chunk.file_path,
                    &chunk.language,
                    chunk.chunk_index,
                    chunk.start_line,
                    chunk.end_line,
                    &chunk.text,
                    &chunk.content_hash,
                    chunk.token_count,
                    now.to_rfc3339()
                ],
            )?;
            self.conn.execute(
                "
                INSERT INTO code_chunks_fts
                    (chunk_id, project_path, file_path, language, text)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    &chunk.id,
                    project_path,
                    &chunk.file_path,
                    &chunk.language,
                    &chunk.text
                ],
            )?;
            if let Some((embeddings, embedding_model)) = chunk_embeddings {
                let embedding = &embeddings[chunk_index];
                self.conn.execute(
                    "
                    INSERT INTO code_embeddings
                        (chunk_id, project_path, embedding, dim, model, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ",
                    params![
                        &chunk.id,
                        project_path,
                        encode_embedding(embedding),
                        embedding.len() as i64,
                        embedding_model,
                        now.to_rfc3339()
                    ],
                )?;
            }
        }

        let run = CodeIndexRun {
            id: Uuid::new_v4().to_string(),
            project_path: project_path.to_string(),
            status: "ready".to_string(),
            file_count,
            entity_count: entities.len() as i64,
            relation_count: relations.len() as i64,
            chunk_count: chunks.len() as i64,
            error: None,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "
            INSERT INTO code_index_runs
                (id, project_path, status, file_count, entity_count, relation_count,
                 chunk_count, error, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ",
            params![
                &run.id,
                &run.project_path,
                &run.status,
                run.file_count,
                run.entity_count,
                run.relation_count,
                run.chunk_count,
                &run.error,
                run.created_at.to_rfc3339(),
                run.updated_at.to_rfc3339()
            ],
        )?;
        Ok(run)
    }

    pub fn get_code_index_stats(&self, project_path: &str) -> AppResult<CodeIndexStats> {
        let latest_run = self
            .conn
            .query_row(
                "
                SELECT id, project_path, status, file_count, entity_count, relation_count,
                       chunk_count, error, created_at, updated_at
                FROM code_index_runs
                WHERE project_path = ?1
                ORDER BY updated_at DESC
                LIMIT 1
                ",
                params![project_path],
                row_to_code_index_run,
            )
            .optional()?;

        Ok(CodeIndexStats {
            project_path: project_path.to_string(),
            latest_run,
        })
    }

    pub fn search_code_index(
        &self,
        project_path: &str,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: i64,
    ) -> AppResult<Vec<CodeSearchResult>> {
        let terms = code_search_terms(query);
        if terms.is_empty() && query_embedding.is_none() {
            return Ok(Vec::new());
        }

        let like_patterns = terms
            .iter()
            .take(4)
            .map(|term| format!("%{}%", term))
            .collect::<Vec<_>>();
        let limit = limit.clamp(1, 20);
        let mut results = Vec::new();

        if let Some(query_embedding) = query_embedding {
            let mut stmt = self.conn.prepare(
                "
                SELECT chunks.file_path, chunks.language, chunks.start_line, chunks.end_line,
                       chunks.text, embeddings.embedding
                FROM code_chunks chunks
                JOIN code_embeddings embeddings ON embeddings.chunk_id = chunks.id
                WHERE chunks.project_path = ?1
                ",
            )?;
            let mut rows = stmt.query(params![project_path])?;
            let mut vector_matches = Vec::new();
            while let Some(row) = rows.next()? {
                let embedding_blob: Vec<u8> = row.get(5)?;
                let embedding = decode_embedding(&embedding_blob)?;
                let score = cosine_similarity(query_embedding, &embedding);
                vector_matches.push(CodeSearchResult {
                    file_path: row.get(0)?,
                    kind: "semantic_chunk".to_string(),
                    name: "语义代码片段".to_string(),
                    language: row.get(1)?,
                    start_line: row.get(2)?,
                    end_line: row.get(3)?,
                    snippet: trim_code_snippet(&row.get::<_, String>(4)?),
                    score,
                });
            }
            vector_matches.sort_by(|left, right| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            results.extend(vector_matches.into_iter().take((limit / 2).max(1) as usize));
        }

        if !terms.is_empty() {
            let mut entity_stmt = self.conn.prepare(
                "
            SELECT file_path, kind, name, language, start_line, end_line, signature
            FROM code_entities
            WHERE project_path = ?1
              AND (
                name LIKE ?2 OR signature LIKE ?2 OR file_path LIKE ?2
                OR name LIKE ?3 OR signature LIKE ?3 OR file_path LIKE ?3
                OR name LIKE ?4 OR signature LIKE ?4 OR file_path LIKE ?4
                OR name LIKE ?5 OR signature LIKE ?5 OR file_path LIKE ?5
              )
            ORDER BY
              CASE WHEN name LIKE ?2 THEN 0 ELSE 1 END,
              file_path,
              start_line
            LIMIT ?6
            ",
            )?;
            let mut entity_rows = entity_stmt.query(params![
                project_path,
                like_patterns.get(0).map(String::as_str).unwrap_or(""),
                like_patterns.get(1).map(String::as_str).unwrap_or(""),
                like_patterns.get(2).map(String::as_str).unwrap_or(""),
                like_patterns.get(3).map(String::as_str).unwrap_or(""),
                limit
            ])?;
            while let Some(row) = entity_rows.next()? {
                let signature: String = row.get(6)?;
                results.push(CodeSearchResult {
                    file_path: row.get(0)?,
                    kind: row.get(1)?,
                    name: row.get(2)?,
                    language: row.get(3)?,
                    start_line: row.get(4)?,
                    end_line: row.get(5)?,
                    snippet: signature,
                    score: 1.0,
                });
            }

            let remaining = limit.saturating_sub(results.len() as i64);
            if remaining > 0 {
                let mut chunk_stmt = self.conn.prepare(
                    "
                SELECT file_path, language, start_line, end_line, text
                FROM code_chunks
                WHERE project_path = ?1
                  AND (
                    text LIKE ?2 OR file_path LIKE ?2
                    OR text LIKE ?3 OR file_path LIKE ?3
                    OR text LIKE ?4 OR file_path LIKE ?4
                    OR text LIKE ?5 OR file_path LIKE ?5
                  )
                ORDER BY file_path, chunk_index
                LIMIT ?6
                ",
                )?;
                let mut chunk_rows = chunk_stmt.query(params![
                    project_path,
                    like_patterns.get(0).map(String::as_str).unwrap_or(""),
                    like_patterns.get(1).map(String::as_str).unwrap_or(""),
                    like_patterns.get(2).map(String::as_str).unwrap_or(""),
                    like_patterns.get(3).map(String::as_str).unwrap_or(""),
                    remaining
                ])?;
                while let Some(row) = chunk_rows.next()? {
                    let text: String = row.get(4)?;
                    results.push(CodeSearchResult {
                        file_path: row.get(0)?,
                        kind: "chunk".to_string(),
                        name: "代码片段".to_string(),
                        language: row.get(1)?,
                        start_line: row.get(2)?,
                        end_line: row.get(3)?,
                        snippet: trim_code_snippet(&text),
                        score: 0.6,
                    });
                }
            }
        }

        dedupe_code_search_results(&mut results);
        results.truncate(limit as usize);
        Ok(results)
    }

    pub fn list_memories(&self) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories
            ORDER BY enabled DESC, updated_at DESC
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn list_enabled_memories(&self) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories
            WHERE enabled = 1
            ORDER BY updated_at DESC
            LIMIT 30
            ",
        )?;

        let rows = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn list_relevant_memories(
        &self,
        query: &str,
        limit: Option<i64>,
    ) -> AppResult<Vec<Memory>> {
        let limit = limit.unwrap_or(8).clamp(1, 30) as usize;
        let trimmed = query.trim();
        if trimmed.is_empty() {
            let mut memories = self.list_enabled_memories()?;
            memories.truncate(limit);
            return Ok(memories);
        }

        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories
            WHERE enabled = 1
            ORDER BY updated_at DESC
            ",
        )?;
        let memories = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        let tokens = memory_query_tokens(trimmed);
        let query_lower = trimmed.to_lowercase();
        let mut scored = memories
            .into_iter()
            .filter_map(|memory| {
                let score = score_memory_relevance(&memory, &query_lower, &tokens);
                (score > 0).then_some((score, memory))
            })
            .collect::<Vec<_>>();

        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.updated_at.cmp(&left.1.updated_at))
        });
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(_, memory)| memory).collect())
    }

    pub fn list_memories_missing_embedding(
        &self,
        model: &str,
        limit: i64,
    ) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT m.id, m.title, m.content, m.tags_json, m.enabled, m.created_at, m.updated_at
            FROM memories m
            WHERE m.enabled = 1
            ORDER BY m.updated_at DESC
            ",
        )?;

        let candidates = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut missing = candidates
            .into_iter()
            .filter(|memory| {
                self.memory_embedding_is_stale(memory, model)
                    .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        missing.truncate(limit.clamp(1, 512) as usize);
        Ok(missing)
    }

    pub fn memory_needs_embedding(&self, memory: &Memory, model: &str) -> AppResult<bool> {
        self.memory_embedding_is_stale(memory, model)
    }

    pub fn upsert_memory_embedding(
        &self,
        memory: &Memory,
        model: &str,
        embedding: &[f32],
    ) -> AppResult<()> {
        if embedding.is_empty() || embedding.len() > 65_536 {
            return Err(AppError::Message(
                "memory embedding dimensions are invalid".to_string(),
            ));
        }

        let dimensions = embedding.len() as i64;
        let table = memory_vector_table(dimensions)?;
        self.conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {table} USING vec0(embedding float[{dimensions}]);"
        ))?;

        let existing = self
            .conn
            .query_row(
                "SELECT vector_rowid, dimensions FROM memory_embeddings WHERE memory_id = ?1",
                params![memory.id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let vector_rowid = existing.map(|value| value.0).unwrap_or_else(|| {
            self.conn
                .query_row(
                    "SELECT COALESCE(MAX(vector_rowid), 0) + 1 FROM memory_embeddings",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(1)
        });

        self.conn.execute_batch("SAVEPOINT memory_vector_upsert")?;
        let result = (|| -> AppResult<()> {
            if let Some((_, old_dimensions)) = existing {
                let old_table = memory_vector_table(old_dimensions)?;
                self.conn.execute(
                    &format!("DELETE FROM {old_table} WHERE rowid = ?1"),
                    params![vector_rowid],
                )?;
            }
            self.conn.execute(
                &format!("INSERT INTO {table}(rowid, embedding) VALUES (?1, ?2)"),
                params![vector_rowid, embedding.as_bytes()],
            )?;
            self.conn.execute(
                "
                INSERT INTO memory_embeddings
                    (vector_rowid, memory_id, model, dimensions, content_hash, indexed_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(memory_id) DO UPDATE SET
                    vector_rowid = excluded.vector_rowid,
                    model = excluded.model,
                    dimensions = excluded.dimensions,
                    content_hash = excluded.content_hash,
                    indexed_at = excluded.indexed_at
                ",
                params![
                    vector_rowid,
                    memory.id,
                    model,
                    dimensions,
                    memory_content_hash(memory),
                    Utc::now().to_rfc3339()
                ],
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.conn.execute_batch("RELEASE memory_vector_upsert")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.conn.execute_batch(
                    "ROLLBACK TO memory_vector_upsert; RELEASE memory_vector_upsert;",
                );
                Err(error)
            }
        }
    }

    pub fn search_hybrid_memories(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        query_embedding_model: Option<&str>,
        limit: i64,
    ) -> AppResult<Vec<Memory>> {
        let limit = limit.clamp(1, 30) as usize;
        let candidate_limit = (limit * 6).clamp(24, 180);
        let mut scores = HashMap::<String, f64>::new();

        let lexical = self.list_relevant_memories(query, Some(candidate_limit as i64))?;
        add_rrf_scores(
            &mut scores,
            lexical.iter().map(|memory| memory.id.as_str()),
            1.0,
        );

        if let Some(embedding) = query_embedding.filter(|value| !value.is_empty()) {
            let vector_ids = self.search_memory_vectors(
                embedding,
                query_embedding_model,
                candidate_limit as i64,
            )?;
            add_rrf_scores(&mut scores, vector_ids.iter().map(String::as_str), 1.15);
        }

        let graph_ids = self.search_memory_graph(query, candidate_limit as i64)?;
        add_rrf_scores(&mut scores, graph_ids.iter().map(String::as_str), 0.9);

        let mut ranked = scores.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });

        let always_limit = ((limit + 2) / 3).clamp(1, 3);
        let mut memories = self.list_always_personalization_memories(always_limit)?;
        let mut selected_ids = memories
            .iter()
            .map(|memory| memory.id.clone())
            .collect::<std::collections::HashSet<_>>();
        for (id, _) in ranked {
            if memories.len() >= limit {
                break;
            }
            if selected_ids.contains(&id) {
                continue;
            }
            if let Some(memory) = self.get_memory(&id)? {
                if memory.enabled {
                    selected_ids.insert(memory.id.clone());
                    memories.push(memory);
                }
            }
        }
        Ok(memories)
    }

    fn list_always_personalization_memories(&self, limit: usize) -> AppResult<Vec<Memory>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT m.id, m.title, m.content, m.tags_json, m.enabled, m.created_at, m.updated_at
            FROM memory_entities e
            JOIN memory_entity_links l ON l.entity_id = e.id
            JOIN memories m ON m.id = l.memory_id
            WHERE e.kind = 'tag'
              AND e.normalized_name = 'personalization:always'
              AND m.enabled = 1
            ORDER BY m.updated_at DESC
            LIMIT ?1
            ",
        )?;
        let rows = stmt
            .query_map([limit as i64], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn memory_embedding_is_stale(&self, memory: &Memory, model: &str) -> AppResult<bool> {
        let indexed = self
            .conn
            .query_row(
                "SELECT model, content_hash FROM memory_embeddings WHERE memory_id = ?1",
                params![memory.id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(match indexed {
            Some((indexed_model, content_hash)) => {
                indexed_model != model || content_hash != memory_content_hash(memory)
            }
            None => true,
        })
    }

    fn search_memory_vectors(
        &self,
        query_embedding: &[f32],
        model: Option<&str>,
        limit: i64,
    ) -> AppResult<Vec<String>> {
        let dimensions = query_embedding.len() as i64;
        let table = memory_vector_table(dimensions)?;
        let exists = self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(Vec::new());
        }

        let sql = format!(
            "
            SELECT e.memory_id
            FROM {table} v
            JOIN memory_embeddings e ON e.vector_rowid = v.rowid
            JOIN memories m ON m.id = e.memory_id
            WHERE v.embedding MATCH ?1
              AND k = ?2
              AND (?3 IS NULL OR e.model = ?3)
              AND m.enabled = 1
            ORDER BY v.distance
            "
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let ids = stmt
            .query_map(
                params![query_embedding.as_bytes(), limit.clamp(1, 180), model],
                |row| row.get(0),
            )?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    fn search_memory_graph(&self, query: &str, limit: i64) -> AppResult<Vec<String>> {
        let tokens = memory_query_tokens(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let graph_query = tokens
            .iter()
            .take(12)
            .map(|token| format!("\"{}\"*", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut seed_stmt = self.conn.prepare(
            "
            SELECT e.id
            FROM memory_entities_fts f
            JOIN memory_entities e ON e.id = f.id
            WHERE memory_entities_fts MATCH ?1
            LIMIT 24
            ",
        )?;
        let seed_ids = seed_stmt
            .query_map([graph_query], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if seed_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut scores = HashMap::<String, f64>::new();
        let mut direct_stmt = self
            .conn
            .prepare("SELECT memory_id, weight FROM memory_entity_links WHERE entity_id = ?1")?;
        let mut neighbor_stmt = self.conn.prepare(
            "
            SELECT CASE WHEN source_entity_id = ?1 THEN target_entity_id ELSE source_entity_id END,
                   weight
            FROM memory_relations
            WHERE source_entity_id = ?1 OR target_entity_id = ?1
            LIMIT 64
            ",
        )?;
        for seed_id in seed_ids {
            let direct = direct_stmt
                .query_map([&seed_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (memory_id, weight) in direct {
                *scores.entry(memory_id).or_default() += weight;
            }

            let neighbors = neighbor_stmt
                .query_map([&seed_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (neighbor_id, relation_weight) in neighbors {
                let linked = direct_stmt
                    .query_map([neighbor_id], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                for (memory_id, link_weight) in linked {
                    *scores.entry(memory_id).or_default() += relation_weight * link_weight * 0.45;
                }
            }
        }

        let mut ranked = scores.into_iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked.truncate(limit.clamp(1, 180) as usize);
        Ok(ranked.into_iter().map(|(id, _)| id).collect())
    }

    fn rebuild_missing_memory_graphs(&self) -> AppResult<()> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, title, content, tags_json, enabled, created_at, updated_at
            FROM memories m
            WHERE NOT EXISTS (
                SELECT 1 FROM memory_entity_links l WHERE l.memory_id = m.id
            )
            ",
        )?;
        let memories = stmt
            .query_map([], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for memory in memories {
            self.sync_memory_graph(&memory)?;
        }
        Ok(())
    }

    fn sync_memory_graph(&self, memory: &Memory) -> AppResult<()> {
        self.conn.execute(
            "DELETE FROM memory_relations WHERE evidence_memory_id = ?1",
            params![memory.id],
        )?;
        self.conn.execute(
            "DELETE FROM memory_entity_links WHERE memory_id = ?1",
            params![memory.id],
        )?;

        let entities = extract_memory_entities(memory);
        let mut entity_ids = Vec::with_capacity(entities.len());
        for entity in entities {
            let normalized = normalize_entity_name(&entity.name);
            if normalized.is_empty() {
                continue;
            }
            let existing_id = self
                .conn
                .query_row(
                    "SELECT id FROM memory_entities WHERE kind = ?1 AND normalized_name = ?2",
                    params![entity.kind, normalized],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let entity_id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());
            let inserted = self.conn.execute(
                "
                INSERT OR IGNORE INTO memory_entities
                    (id, kind, name, normalized_name, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    entity_id,
                    entity.kind,
                    entity.name,
                    normalized,
                    Utc::now().to_rfc3339()
                ],
            )?;
            if inserted > 0 {
                self.conn.execute(
                    "INSERT INTO memory_entities_fts (id, name) VALUES (?1, ?2)",
                    params![entity_id, entity.name],
                )?;
            }
            self.conn.execute(
                "INSERT OR REPLACE INTO memory_entity_links (memory_id, entity_id, weight) VALUES (?1, ?2, ?3)",
                params![memory.id, entity_id, entity.weight],
            )?;
            entity_ids.push((entity_id, entity.weight));
        }

        for left in 0..entity_ids.len() {
            for right in (left + 1)..entity_ids.len() {
                let (source_id, source_weight) = &entity_ids[left];
                let (target_id, target_weight) = &entity_ids[right];
                self.conn.execute(
                    "
                    INSERT OR REPLACE INTO memory_relations
                        (source_entity_id, target_entity_id, kind, weight, evidence_memory_id)
                    VALUES (?1, ?2, 'co_occurs', ?3, ?4)
                    ",
                    params![
                        source_id,
                        target_id,
                        source_weight.min(*target_weight),
                        memory.id
                    ],
                )?;
            }
        }
        self.remove_orphan_memory_entities()
    }

    fn remove_orphan_memory_entities(&self) -> AppResult<()> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id FROM memory_entities e
            WHERE NOT EXISTS (SELECT 1 FROM memory_entity_links l WHERE l.entity_id = e.id)
              AND NOT EXISTS (
                  SELECT 1 FROM memory_relations r
                  WHERE r.source_entity_id = e.id OR r.target_entity_id = e.id
              )
            ",
        )?;
        let ids = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        for id in ids {
            self.conn
                .execute("DELETE FROM memory_entities_fts WHERE id = ?1", params![id])?;
            self.conn
                .execute("DELETE FROM memory_entities WHERE id = ?1", params![id])?;
        }
        Ok(())
    }

    pub fn search_memories(&self, query: &str) -> AppResult<Vec<Memory>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_memories();
        }

        let Some(fts_query) = build_fts_prefix_query(trimmed) else {
            return Ok(Vec::new());
        };

        let mut stmt = self.conn.prepare(
            "
            SELECT m.id, m.title, m.content, m.tags_json, m.enabled, m.created_at, m.updated_at
            FROM memories_fts f
            JOIN memories m ON m.id = f.id
            WHERE memories_fts MATCH ?1
            ORDER BY rank
            LIMIT 100
            ",
        )?;

        let rows = stmt
            .query_map([fts_query], Self::row_to_memory)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;

        Ok(rows)
    }

    pub fn create_memory(&self, draft: MemoryDraft) -> AppResult<Memory> {
        let now = Utc::now();
        let memory = Memory {
            id: Uuid::new_v4().to_string(),
            title: clean_or_default(draft.title, "New memory"),
            content: draft.content,
            tags: draft.tags,
            enabled: draft.enabled.unwrap_or(true),
            created_at: now,
            updated_at: now,
        };

        self.upsert_memory(&memory)?;
        Ok(memory)
    }

    pub fn update_memory(&self, patch: MemoryPatch) -> AppResult<Memory> {
        let current = self
            .get_memory(&patch.id)?
            .ok_or_else(|| AppError::Message("memory not found".to_string()))?;

        let memory = Memory {
            id: current.id,
            title: patch.title.unwrap_or(current.title),
            content: patch.content.unwrap_or(current.content),
            tags: patch.tags.unwrap_or(current.tags),
            enabled: patch.enabled.unwrap_or(current.enabled),
            created_at: current.created_at,
            updated_at: Utc::now(),
        };

        self.upsert_memory(&memory)?;
        Ok(memory)
    }

    pub fn delete_memory(&self, id: &str) -> AppResult<()> {
        self.with_savepoint("memory_delete", || {
            if let Some((vector_rowid, dimensions)) = self
                .conn
                .query_row(
                    "SELECT vector_rowid, dimensions FROM memory_embeddings WHERE memory_id = ?1",
                    params![id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?
            {
                let table = memory_vector_table(dimensions)?;
                self.conn.execute(
                    &format!("DELETE FROM {table} WHERE rowid = ?1"),
                    params![vector_rowid],
                )?;
            }
            self.conn
                .execute("DELETE FROM memories_fts WHERE id = ?1", params![id])?;
            let affected = self
                .conn
                .execute("DELETE FROM memories WHERE id = ?1", params![id])?;
            ensure_affected(affected, "memory not found")?;
            self.remove_orphan_memory_entities()
        })
    }

    fn get_item(&self, id: &str) -> AppResult<Option<Item>> {
        self.conn
            .query_row(
                "
                SELECT id, kind, title, body, status, tags_json, created_at, updated_at
                FROM items WHERE id = ?1
                ",
                params![id],
                Self::row_to_item,
            )
            .optional()
            .map_err(AppError::from)
    }

    fn get_memory(&self, id: &str) -> AppResult<Option<Memory>> {
        self.conn
            .query_row(
                "
                SELECT id, title, content, tags_json, enabled, created_at, updated_at
                FROM memories WHERE id = ?1
                ",
                params![id],
                Self::row_to_memory,
            )
            .optional()
            .map_err(AppError::from)
    }

    fn upsert_item(&self, item: &Item) -> AppResult<()> {
        let tags_json = serde_json::to_string(&item.tags)?;
        self.conn.execute(
            "
            INSERT INTO items (id, kind, title, body, status, tags_json, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                title = excluded.title,
                body = excluded.body,
                status = excluded.status,
                tags_json = excluded.tags_json,
                updated_at = excluded.updated_at
            ",
            params![
                item.id,
                item.kind,
                item.title,
                item.body,
                item.status,
                tags_json,
                item.created_at.to_rfc3339(),
                item.updated_at.to_rfc3339()
            ],
        )?;

        self.conn
            .execute("DELETE FROM items_fts WHERE id = ?1", params![item.id])?;
        self.conn.execute(
            "INSERT INTO items_fts (id, title, body, tags) VALUES (?1, ?2, ?3, ?4)",
            params![item.id, item.title, item.body, item.tags.join(" ")],
        )?;
        Ok(())
    }

    fn upsert_memory(&self, memory: &Memory) -> AppResult<()> {
        self.with_savepoint("memory_record_upsert", || self.upsert_memory_inner(memory))
    }

    fn upsert_memory_inner(&self, memory: &Memory) -> AppResult<()> {
        let tags_json = serde_json::to_string(&memory.tags)?;
        self.conn.execute(
            "
            INSERT INTO memories (id, title, content, tags_json, enabled, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                content = excluded.content,
                tags_json = excluded.tags_json,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at
            ",
            params![
                memory.id,
                memory.title,
                memory.content,
                tags_json,
                if memory.enabled { 1 } else { 0 },
                memory.created_at.to_rfc3339(),
                memory.updated_at.to_rfc3339()
            ],
        )?;

        self.conn
            .execute("DELETE FROM memories_fts WHERE id = ?1", params![memory.id])?;
        self.conn.execute(
            "INSERT INTO memories_fts (id, title, content, tags) VALUES (?1, ?2, ?3, ?4)",
            params![
                memory.id,
                memory.title,
                memory.content,
                memory.tags.join(" ")
            ],
        )?;
        self.sync_memory_graph(memory)
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
        let created_at: String = row.get(10)?;
        let updated_at: String = row.get(11)?;

        Ok(ModelConfig {
            id: row.get(0)?,
            name: row.get(1)?,
            provider: row.get(2)?,
            base_url: row.get(3)?,
            model: row.get(4)?,
            api_key: row.get(5)?,
            embedding_provider: row.get(6)?,
            embedding_base_url: row.get(7)?,
            embedding_model: row.get(8)?,
            embedding_api_key: row.get(9)?,
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
    if bytes.len() % 4 != 0 {
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
}
