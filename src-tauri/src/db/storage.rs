use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use super::Database;
use crate::error::{AppError, AppResult};

const MIGRATION_KEY: &str = "legacy-main-split-v1";

const CONFIG_TABLES: &[&str] = &["model_configs", "mcp_servers", "ops_servers"];
const CONVERSATION_TABLES: &[&str] = &[
    // model_configs is an internal reference mirror for cross-database foreign keys.
    "model_configs",
    "conversations",
    "messages",
    "rag_files",
    "rag_chunks",
    "rag_embeddings",
    "rag_chunks_fts",
    "profile_state",
    "profile_settings",
    "profile_observations",
    "profile_extraction_batches",
    "profile_batch_observations",
    "user_profile_facts",
    "user_profile_fact_sources",
    "profile_fact_tombstones",
    "profile_batch_commits",
    "profile_foreground_leases",
    "profile_usage_attempts",
];
const KNOWLEDGE_TABLES: &[&str] = &[
    "items",
    "items_fts",
    "memories",
    "memories_fts",
    "memory_embeddings",
    "memory_entities",
    "memory_entities_fts",
    "memory_entity_links",
    "memory_relations",
];
const PROJECT_INDEX_TABLES: &[&str] = &[
    "code_index_runs",
    "code_entities",
    "code_relations",
    "code_chunks",
    "code_chunks_fts",
    "code_embeddings",
    "project_index_runs",
    "project_index_chunks",
    "project_index_chunks_fts",
    "project_index_embeddings",
];

const CONFIG_COPY_ORDER: &[&str] = &["model_configs", "mcp_servers", "ops_servers"];
const CONVERSATION_COPY_ORDER: &[&str] = &[
    "conversations",
    "messages",
    "rag_files",
    "rag_chunks",
    "rag_embeddings",
    "profile_state",
    "profile_settings",
    "profile_observations",
    "profile_extraction_batches",
    "profile_batch_observations",
    "user_profile_facts",
    "user_profile_fact_sources",
    "profile_fact_tombstones",
    "profile_batch_commits",
    "profile_foreground_leases",
    "profile_usage_attempts",
];
const KNOWLEDGE_COPY_ORDER: &[&str] = &[
    "items",
    "memories",
    "memory_entities",
    "memory_entity_links",
    "memory_relations",
    "memory_embeddings",
];
const PROJECT_COPY_ORDER: &[&str] = &[
    "code_index_runs",
    "code_entities",
    "code_relations",
    "code_chunks",
    "code_embeddings",
    "project_index_runs",
    "project_index_chunks",
    "project_index_embeddings",
];

pub(super) struct DatabasePaths {
    pub(super) legacy: PathBuf,
    pub(super) config: PathBuf,
    pub(super) conversations: PathBuf,
    pub(super) knowledge: PathBuf,
    pub(super) project_index: PathBuf,
}

impl DatabasePaths {
    pub(super) fn from_legacy_path(path: &Path) -> AppResult<Self> {
        if path == Path::new(":memory:") {
            return Ok(Self {
                legacy: path.to_path_buf(),
                config: path.to_path_buf(),
                conversations: path.to_path_buf(),
                knowledge: path.to_path_buf(),
                project_index: path.to_path_buf(),
            });
        }

        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                AppError::Message("database path must include a valid file name".into())
            })?;
        let parent = path.parent().unwrap_or_else(|| Path::new(""));
        let named = |suffix: &str| parent.join(format!("{stem}-{suffix}.sqlite3"));

        Ok(Self {
            legacy: path.to_path_buf(),
            config: named("config"),
            conversations: named("conversations"),
            knowledge: named("knowledge"),
            project_index: named("project-index"),
        })
    }
}

impl Database {
    pub(super) fn initialize_split_storage(&self, paths: &DatabasePaths) -> AppResult<()> {
        Self::prepare_database(&self.config_conn, CONFIG_TABLES)?;
        Self::prepare_database(&self.conn, CONVERSATION_TABLES)?;
        Self::prepare_database(&self.knowledge_conn, KNOWLEDGE_TABLES)?;
        Self::prepare_database(&self.project_conn, PROJECT_INDEX_TABLES)?;

        if paths.legacy != Path::new(":memory:") && paths.legacy.exists() {
            Self::migrate_database(
                &self.config_conn,
                &paths.legacy,
                CONFIG_COPY_ORDER,
                MigrationKind::Config,
            )
            .map_err(|error| AppError::Message(format!("config migration failed: {error}")))?;
            self.sync_model_config_references()?;
            Self::migrate_database(
                &self.conn,
                &paths.legacy,
                CONVERSATION_COPY_ORDER,
                MigrationKind::Conversations,
            )
            .map_err(|error| {
                AppError::Message(format!("conversation migration failed: {error}"))
            })?;
            Self::migrate_database(
                &self.knowledge_conn,
                &paths.legacy,
                KNOWLEDGE_COPY_ORDER,
                MigrationKind::Knowledge,
            )
            .map_err(|error| AppError::Message(format!("knowledge migration failed: {error}")))?;
            Self::migrate_database(
                &self.project_conn,
                &paths.legacy,
                PROJECT_COPY_ORDER,
                MigrationKind::ProjectIndex,
            )
            .map_err(|error| {
                AppError::Message(format!("project index migration failed: {error}"))
            })?;
        }

        self.sync_model_config_references()?;
        self.rebuild_missing_memory_graphs()
            .map_err(|error| AppError::Message(format!("memory graph rebuild failed: {error}")))?;
        Ok(())
    }

    fn prepare_database(conn: &Connection, retained_tables: &[&str]) -> AppResult<()> {
        Self::init_full_schema(conn)?;
        Self::remove_unowned_tables(conn, retained_tables)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS storage_migrations (
                key TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    fn remove_unowned_tables(conn: &Connection, retained_tables: &[&str]) -> AppResult<()> {
        let retained = retained_tables.iter().copied().collect::<HashSet<_>>();
        let mut stmt = conn.prepare(
            "SELECT name, type FROM pragma_table_list
             WHERE schema = 'main' AND type IN ('table', 'virtual') AND name NOT LIKE 'sqlite_%'",
        )?;
        let entries = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        for (name, _) in entries {
            let is_memory_vector = retained.contains("memory_embeddings")
                && name
                    .strip_prefix("memory_vectors_")
                    .and_then(|value| value.parse::<usize>().ok())
                    .is_some();
            if !retained.contains(name.as_str())
                && !is_memory_vector
                && name != "storage_migrations"
            {
                conn.execute_batch(&format!(
                    "DROP TABLE IF EXISTS {};",
                    quote_identifier(&name)
                ))?;
            }
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(())
    }

    fn migrate_database(
        conn: &Connection,
        legacy_path: &Path,
        tables: &[&str],
        kind: MigrationKind,
    ) -> AppResult<()> {
        let migrated = conn
            .query_row(
                "SELECT 1 FROM storage_migrations WHERE key = ?1",
                params![MIGRATION_KEY],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if migrated {
            return Ok(());
        }

        conn.execute(
            "ATTACH DATABASE ?1 AS legacy",
            params![legacy_path.to_string_lossy().as_ref()],
        )?;
        let result = (|| -> AppResult<()> {
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            for table in tables {
                let replace = matches!(*table, "profile_state" | "profile_settings");
                copy_table(conn, table, replace)?;
            }
            if kind == MigrationKind::Knowledge {
                copy_memory_vector_tables(conn)?;
            }
            rebuild_search_indexes(conn, kind)?;
            conn.execute(
                "INSERT INTO storage_migrations (key, applied_at) VALUES (?1, ?2)",
                params![MIGRATION_KEY, Utc::now().to_rfc3339()],
            )?;
            conn.execute_batch("COMMIT;")?;
            Ok(())
        })();

        if result.is_err() {
            let _ = conn.execute_batch("ROLLBACK;");
        }
        let detach_result = conn.execute_batch("DETACH DATABASE legacy;");
        result?;
        detach_result?;
        Ok(())
    }

    pub(super) fn sync_model_config_references(&self) -> AppResult<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE;")?;
        let result = (|| -> AppResult<()> {
            let mut stmt = self.config_conn.prepare("SELECT id FROM model_configs")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut config_ids = HashSet::new();
            let now = Utc::now().to_rfc3339();
            for id in rows {
                let id = id?;
                config_ids.insert(id.clone());
                self.conn.execute(
                    "INSERT INTO model_configs
                        (id, name, provider, base_url, model, api_key, created_at, updated_at)
                     VALUES (?1, '', '', '', '', '', ?2, ?2)
                     ON CONFLICT(id) DO UPDATE SET
                        name = '', provider = '', base_url = '', model = '', api_key = '',
                        temperature = 0.4, max_tokens = NULL, context_window = 32768,
                        top_p = NULL, reasoning_effort = '', embedding_provider = '',
                        embedding_base_url = '', embedding_model = '', embedding_api_key = '',
                        updated_at = excluded.updated_at",
                    params![id, now],
                )?;
            }
            drop(stmt);
            let mut stmt = self.conn.prepare("SELECT id FROM model_configs")?;
            let mirror_ids = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);
            for id in mirror_ids {
                if !config_ids.contains(&id) {
                    self.conn
                        .execute("DELETE FROM model_configs WHERE id = ?1", params![id])?;
                }
            }
            self.conn.execute_batch("COMMIT;")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self.conn.execute_batch("ROLLBACK;");
        }
        result
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MigrationKind {
    Config,
    Conversations,
    Knowledge,
    ProjectIndex,
}

fn copy_table(conn: &Connection, table: &str, replace: bool) -> AppResult<()> {
    let source_exists = conn
        .query_row(
            "SELECT 1 FROM legacy.sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !source_exists {
        return Ok(());
    }

    let target_columns = table_columns(conn, "main", table)?;
    let source_columns = table_columns(conn, "legacy", table)?;
    let source = source_columns.into_iter().collect::<HashSet<_>>();
    let columns = target_columns
        .into_iter()
        .filter(|column| source.contains(column))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Ok(());
    }
    let names = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect::<Vec<_>>()
        .join(", ");
    let conflict = if replace { "REPLACE" } else { "IGNORE" };
    conn.execute_batch(&format!(
        "INSERT OR {conflict} INTO main.{} ({names}) SELECT {names} FROM legacy.{};",
        quote_identifier(table),
        quote_identifier(table)
    ))?;
    Ok(())
}

fn table_columns(conn: &Connection, schema: &str, table: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(&format!(
        "PRAGMA {}.table_info({})",
        quote_identifier(schema),
        quote_identifier(table)
    ))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns)
}

fn copy_memory_vector_tables(conn: &Connection) -> AppResult<()> {
    let mut stmt = conn.prepare(
        "SELECT name FROM legacy.sqlite_master
         WHERE type = 'table'
           AND name GLOB 'memory_vectors_[0-9]*'
           AND sql LIKE 'CREATE VIRTUAL TABLE%'",
    )?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    for name in names {
        let dimensions = name
            .strip_prefix("memory_vectors_")
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| (1..=65_536).contains(value))
            .ok_or_else(|| AppError::Message(format!("invalid legacy vector table: {name}")))?;
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS {} USING vec0(embedding float[{dimensions}]);
             INSERT OR REPLACE INTO main.{} (rowid, embedding)
             SELECT rowid, embedding FROM legacy.{};",
            quote_identifier(&name),
            quote_identifier(&name),
            quote_identifier(&name)
        ))?;
    }
    Ok(())
}

fn rebuild_search_indexes(conn: &Connection, kind: MigrationKind) -> AppResult<()> {
    match kind {
        MigrationKind::Config => {}
        MigrationKind::Conversations => conn.execute_batch(
            "DELETE FROM rag_chunks_fts;
             INSERT INTO rag_chunks_fts (chunk_id, conversation_id, file_id, file_name, text)
             SELECT c.id, c.conversation_id, c.file_id, f.name, c.text
             FROM rag_chunks c JOIN rag_files f ON f.id = c.file_id;",
        )?,
        MigrationKind::Knowledge => conn.execute_batch(
            "DELETE FROM items_fts;
             INSERT INTO items_fts (id, title, body, tags)
             SELECT id, title, body, tags_json FROM items;
             DELETE FROM memories_fts;
             INSERT INTO memories_fts (id, title, content, tags)
             SELECT id, title, content, tags_json FROM memories;
             DELETE FROM memory_entities_fts;
             INSERT INTO memory_entities_fts (id, name)
             SELECT id, name FROM memory_entities;",
        )?,
        MigrationKind::ProjectIndex => conn.execute_batch(
            "DELETE FROM code_chunks_fts;
             INSERT INTO code_chunks_fts (chunk_id, project_path, file_path, language, text)
             SELECT id, project_path, file_path, language, text FROM code_chunks;
             DELETE FROM project_index_chunks_fts;
             INSERT INTO project_index_chunks_fts
                (chunk_id, project_path, indexer, file_path, title, text)
             SELECT id, project_path, indexer, file_path, title, text FROM project_index_chunks;",
        )?,
    }
    Ok(())
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::Connection;
    use zerocopy::IntoBytes;

    use super::*;
    use crate::models::{ConversationDraft, ModelConfigDraft};

    fn temp_database_path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("nano-desk-{label}-{}", uuid::Uuid::new_v4()))
            .join("nano-agent.sqlite3")
    }

    fn table_exists(conn: &Connection, name: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![name],
            |_| Ok(()),
        )
        .optional()
        .expect("table lookup should succeed")
        .is_some()
    }

    fn model_draft(id: &str, name: &str) -> ModelConfigDraft {
        ModelConfigDraft {
            id: Some(id.to_string()),
            name: name.to_string(),
            provider: "openai-compatible".to_string(),
            base_url: "http://localhost".to_string(),
            model: "test-model".to_string(),
            api_key: "test-secret".to_string(),
            temperature: 0.4,
            max_tokens: None,
            context_window: 32_768,
            top_p: None,
            reasoning_effort: String::new(),
            embedding_provider: "openai-compatible".to_string(),
            embedding_base_url: String::new(),
            embedding_model: "test-embedding".to_string(),
            embedding_api_key: String::new(),
        }
    }

    #[test]
    fn fresh_storage_is_split_by_domain() {
        let legacy_path = temp_database_path("fresh-split");
        fs::create_dir_all(legacy_path.parent().expect("path should have parent"))
            .expect("temporary directory should be created");
        let paths = DatabasePaths::from_legacy_path(&legacy_path).expect("paths should resolve");

        let db = Database::open(legacy_path.clone()).expect("database should open");

        assert!(paths.config.exists());
        assert!(paths.conversations.exists());
        assert!(paths.knowledge.exists());
        assert!(paths.project_index.exists());
        assert!(!legacy_path.exists());
        assert!(table_exists(&db.config_conn, "model_configs"));
        assert!(!table_exists(&db.config_conn, "conversations"));
        assert!(table_exists(&db.conn, "conversations"));
        assert!(!table_exists(&db.conn, "memories"));
        assert!(table_exists(&db.knowledge_conn, "memories"));
        assert!(!table_exists(&db.knowledge_conn, "code_chunks"));
        assert!(table_exists(&db.project_conn, "code_chunks"));
        assert!(!table_exists(&db.project_conn, "model_configs"));

        drop(db);
        fs::remove_dir_all(legacy_path.parent().expect("path should have parent"))
            .expect("temporary directory should be removed");
    }

    #[test]
    fn legacy_database_is_copied_once_and_preserved() {
        // Register sqlite-vec before constructing a representative legacy database.
        drop(Database::open(PathBuf::from(":memory:")).expect("in-memory database should open"));
        let legacy_path = temp_database_path("legacy-split");
        fs::create_dir_all(legacy_path.parent().expect("path should have parent"))
            .expect("temporary directory should be created");
        let paths = DatabasePaths::from_legacy_path(&legacy_path).expect("paths should resolve");
        {
            let conn = Connection::open(&legacy_path).expect("legacy database should open");
            Database::init_full_schema(&conn).expect("legacy schema should initialize");
            conn.execute_batch(
                "INSERT INTO model_configs
                    (id, name, provider, base_url, model, api_key, created_at, updated_at)
                 VALUES ('model-1', 'Legacy model', 'openai-compatible', 'http://localhost',
                         'legacy', 'legacy-secret', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                 INSERT INTO mcp_servers (id, name, command, created_at, updated_at)
                 VALUES ('mcp-1', 'Legacy MCP', 'legacy-mcp',
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                 INSERT INTO conversations
                    (id, title, model_config_id, created_at, updated_at)
                 VALUES ('conversation-1', 'Legacy conversation', 'model-1',
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                 INSERT INTO messages (id, conversation_id, role, content, created_at)
                 VALUES ('message-1', 'conversation-1', 'user', 'legacy message',
                         '2026-01-01T00:00:00Z');
                 INSERT INTO rag_files
                    (id, conversation_id, name, mime, size, content_hash, chunk_count,
                     status, created_at)
                 VALUES ('file-1', 'conversation-1', 'legacy.txt', 'text/plain', 14,
                         'file-hash', 1, 'ready', '2026-01-01T00:00:00Z');
                 INSERT INTO rag_chunks
                    (id, file_id, conversation_id, chunk_index, text, token_count, created_at)
                 VALUES ('chunk-1', 'file-1', 'conversation-1', 0, 'legacy content', 2,
                         '2026-01-01T00:00:00Z');
                 UPDATE profile_settings SET enabled = 1, model_config_id = 'model-1';
                 INSERT INTO items
                    (id, kind, title, body, status, tags_json, created_at, updated_at)
                 VALUES ('item-1', 'note', 'Legacy item', 'legacy item body', 'active', '[]',
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                 INSERT INTO memories
                    (id, title, content, tags_json, enabled, created_at, updated_at)
                 VALUES ('memory-1', 'Legacy memory', 'legacy memory body', '[]', 1,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                 INSERT INTO code_index_runs
                    (id, project_path, status, file_count, entity_count, relation_count,
                     chunk_count, created_at, updated_at)
                 VALUES ('run-1', 'D:/legacy', 'ready', 1, 0, 0, 0,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
                 CREATE VIRTUAL TABLE memory_vectors_2 USING vec0(embedding float[2]);",
            )
            .expect("legacy data should be inserted");
            conn.execute(
                "INSERT INTO memory_vectors_2 (rowid, embedding) VALUES (1, ?1)",
                params![[1.0_f32, 0.0_f32].as_bytes()],
            )
            .expect("legacy vector should be inserted");
            conn.execute(
                "INSERT INTO memory_embeddings
                    (vector_rowid, memory_id, model, dimensions, content_hash, indexed_at)
                 VALUES (1, 'memory-1', 'legacy-embedding', 2, 'hash', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("legacy embedding metadata should be inserted");
        }

        let db = Database::open(legacy_path.clone()).expect("legacy migration should succeed");
        assert!(legacy_path.exists(), "legacy database must be preserved");
        assert_eq!(
            db.list_model_configs().expect("models should load").len(),
            1
        );
        assert_eq!(
            db.list_mcp_servers()
                .expect("MCP servers should load")
                .len(),
            1
        );
        assert_eq!(
            db.list_conversations(None)
                .expect("conversations should load")
                .len(),
            1
        );
        assert_eq!(db.list_items(None).expect("items should load").len(), 1);
        assert_eq!(
            db.conn
                .query_row("SELECT COUNT(*) FROM rag_chunks_fts", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("RAG search index should be rebuilt"),
            1
        );
        assert_eq!(
            db.conn
                .query_row(
                    "SELECT enabled, model_config_id FROM profile_settings WHERE id = 1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .expect("profile settings should migrate"),
            (1, Some("model-1".to_string()))
        );
        assert_eq!(db.list_memories().expect("memories should load").len(), 1);
        assert_eq!(
            db.project_conn
                .query_row("SELECT COUNT(*) FROM code_index_runs", [], |row| row
                    .get::<_, i64>(0))
                .expect("project rows should be readable"),
            1
        );
        assert_eq!(
            db.knowledge_conn
                .query_row("SELECT COUNT(*) FROM memory_vectors_2", [], |row| row
                    .get::<_, i64>(0))
                .expect("vectors should be readable"),
            1
        );
        drop(db);

        assert_eq!(
            Connection::open(&legacy_path)
                .expect("legacy database should remain readable")
                .query_row(
                    "SELECT api_key FROM model_configs WHERE id = 'model-1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("legacy data should remain unchanged"),
            "legacy-secret"
        );

        let reopened = Database::open(legacy_path.clone()).expect("second open should succeed");
        assert_eq!(
            reopened
                .list_memories()
                .expect("memories should load")
                .len(),
            1
        );
        assert_eq!(
            reopened
                .knowledge_conn
                .query_row("SELECT COUNT(*) FROM memory_vectors_2", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("vectors should survive reopening"),
            1
        );
        assert_eq!(
            reopened
                .config_conn
                .query_row("SELECT COUNT(*) FROM storage_migrations", [], |row| row
                    .get::<_, i64>(0))
                .expect("migration marker should be readable"),
            1
        );
        drop(reopened);

        for path in [
            &paths.config,
            &paths.conversations,
            &paths.knowledge,
            &paths.project_index,
        ] {
            assert!(path.exists());
        }
        fs::remove_dir_all(legacy_path.parent().expect("path should have parent"))
            .expect("temporary directory should be removed");
    }

    #[test]
    fn model_reference_mirror_preserves_updates_and_clears_deletes() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        db.save_model_config(model_draft("model-1", "First"))
            .expect("model should save");
        assert_eq!(
            db.conn
                .query_row(
                    "SELECT api_key FROM model_configs WHERE id = 'model-1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("reference mirror should load"),
            ""
        );
        let conversation = db
            .create_conversation(ConversationDraft {
                title: Some("Reference test".to_string()),
                model_config_id: Some("model-1".to_string()),
                project_path: None,
            })
            .expect("conversation should save");

        db.save_model_config(model_draft("model-1", "Updated"))
            .expect("model update should save");
        let reference_after_update: Option<String> = db
            .conn
            .query_row(
                "SELECT model_config_id FROM conversations WHERE id = ?1",
                params![conversation.id],
                |row| row.get(0),
            )
            .expect("conversation reference should load");
        assert_eq!(reference_after_update.as_deref(), Some("model-1"));

        db.delete_model_config("model-1")
            .expect("model should delete");
        let reference_after_delete: Option<String> = db
            .conn
            .query_row(
                "SELECT model_config_id FROM conversations WHERE id = ?1",
                params![conversation.id],
                |row| row.get(0),
            )
            .expect("conversation reference should load");
        assert_eq!(reference_after_delete, None);
    }
}
