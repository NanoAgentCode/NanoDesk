use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
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

pub(crate) use rag_store::RagFileReplacement;

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
    use crate::models::{MemoryDraft, MemoryPatch};

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
