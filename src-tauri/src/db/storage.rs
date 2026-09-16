use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use super::Database;
use crate::error::{AppError, AppResult};

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

pub(super) struct DatabasePaths {
    pub(super) config: PathBuf,
    pub(super) conversations: PathBuf,
    pub(super) knowledge: PathBuf,
    pub(super) project_index: PathBuf,
}

impl DatabasePaths {
    pub(super) fn from_base_path(path: &Path) -> AppResult<Self> {
        if path == Path::new(":memory:") {
            return Ok(Self {
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
            config: named("config"),
            conversations: named("conversations"),
            knowledge: named("knowledge"),
            project_index: named("project-index"),
        })
    }
}

impl Database {
    pub(super) fn initialize_split_storage(&self) -> AppResult<()> {
        Self::prepare_database(&self.config_conn, CONFIG_TABLES)?;
        Self::prepare_database(&self.conn, CONVERSATION_TABLES)?;
        Self::prepare_database(&self.knowledge_conn, KNOWLEDGE_TABLES)?;
        Self::prepare_database(&self.project_conn, PROJECT_INDEX_TABLES)?;

        rebuild_legacy_search_indexes_once(&self.conn, rebuild_conversation_search_indexes)?;
        rebuild_legacy_search_indexes_once(&self.knowledge_conn, rebuild_knowledge_search_indexes)?;
        rebuild_legacy_search_indexes_once(&self.project_conn, rebuild_project_search_indexes)?;

        self.sync_model_config_references()?;
        self.rebuild_missing_memory_graphs()
            .map_err(|error| AppError::Message(format!("memory graph rebuild failed: {error}")))?;
        Ok(())
    }

    fn prepare_database(conn: &Connection, retained_tables: &[&str]) -> AppResult<()> {
        Self::init_full_schema(conn)?;
        Self::remove_unowned_tables(conn, retained_tables)?;
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

fn rebuild_conversation_search_indexes(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "DELETE FROM rag_chunks_fts;
         INSERT INTO rag_chunks_fts (chunk_id, conversation_id, file_id, file_name, text)
         SELECT c.id, c.conversation_id, c.file_id, f.name, c.text
         FROM rag_chunks c JOIN rag_files f ON f.id = c.file_id;",
    )?;
    Ok(())
}

fn rebuild_knowledge_search_indexes(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "DELETE FROM items_fts;
         INSERT INTO items_fts (id, title, body, tags)
         SELECT id, title, body, tags_json FROM items;
         DELETE FROM memories_fts;
         INSERT INTO memories_fts (id, title, content, tags)
         SELECT id, title, content, tags_json FROM memories;
         DELETE FROM memory_entities_fts;
         INSERT INTO memory_entities_fts (id, name)
         SELECT id, name FROM memory_entities;",
    )?;
    Ok(())
}

fn rebuild_project_search_indexes(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "DELETE FROM code_chunks_fts;
         INSERT INTO code_chunks_fts (chunk_id, project_path, file_path, language, text)
         SELECT id, project_path, file_path, language, text FROM code_chunks;
         DELETE FROM project_index_chunks_fts;
         INSERT INTO project_index_chunks_fts
            (chunk_id, project_path, indexer, file_path, title, text)
         SELECT id, project_path, indexer, file_path, title, text FROM project_index_chunks;",
    )?;
    Ok(())
}

fn rebuild_legacy_search_indexes_once(
    conn: &Connection,
    rebuild: fn(&Connection) -> AppResult<()>,
) -> AppResult<()> {
    let has_migration_table = conn
        .query_row(
            "SELECT 1 FROM sqlite_master
             WHERE type = 'table' AND name = 'storage_migrations'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_migration_table {
        return Ok(());
    }

    let legacy_imported = conn
        .query_row(
            "SELECT 1 FROM storage_migrations WHERE key = ?1",
            params![crate::legacy_migration::MIGRATION_KEY],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    let already_rebuilt = conn
        .query_row(
            "SELECT 1 FROM storage_migrations WHERE key = ?1",
            params![crate::legacy_migration::SEARCH_INDEX_REBUILD_KEY],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !legacy_imported || already_rebuilt {
        return Ok(());
    }

    rebuild(conn)?;
    conn.execute(
        "INSERT INTO storage_migrations (key, applied_at) VALUES (?1, ?2)",
        params![
            crate::legacy_migration::SEARCH_INDEX_REBUILD_KEY,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::models::{ConversationDraft, ModelConfigDraft};
    use rusqlite::Connection;

    fn temp_database_path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("nano-desk-{label}-{}", uuid::Uuid::new_v4()))
            .join("nanodesk.sqlite3")
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

    fn increment_rebuild_count(conn: &Connection) -> AppResult<()> {
        conn.execute("UPDATE rebuild_count SET value = value + 1", [])?;
        Ok(())
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
            model_kind: "chat".to_string(),
            routing_group: "默认组".to_string(),
            routing_enabled: true,
            routing_cost: 3,
            routing_quality: 3,
            routing_speed: 3,
            routing_tasks: Vec::new(),
            embedding_provider: "openai-compatible".to_string(),
            embedding_base_url: String::new(),
            embedding_model: "test-embedding".to_string(),
            embedding_api_key: String::new(),
        }
    }

    #[test]
    fn fresh_storage_is_split_by_domain() {
        let base_path = temp_database_path("fresh-split");
        fs::create_dir_all(base_path.parent().expect("path should have parent"))
            .expect("temporary directory should be created");
        let paths = DatabasePaths::from_base_path(&base_path).expect("paths should resolve");

        let db = Database::open(base_path.clone()).expect("database should open");

        assert!(paths.config.exists());
        assert!(paths.conversations.exists());
        assert!(paths.knowledge.exists());
        assert!(paths.project_index.exists());
        assert!(!base_path.exists());
        assert!(table_exists(&db.config_conn, "model_configs"));
        assert!(!table_exists(&db.config_conn, "conversations"));
        assert!(table_exists(&db.conn, "conversations"));
        assert!(!table_exists(&db.conn, "memories"));
        assert!(table_exists(&db.knowledge_conn, "memories"));
        assert!(!table_exists(&db.knowledge_conn, "code_chunks"));
        assert!(table_exists(&db.project_conn, "code_chunks"));
        assert!(!table_exists(&db.project_conn, "model_configs"));

        drop(db);
        fs::remove_dir_all(base_path.parent().expect("path should have parent"))
            .expect("temporary directory should be removed");
    }

    #[test]
    fn legacy_search_indexes_are_rebuilt_only_once() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE storage_migrations (
                key TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
             );
             INSERT INTO storage_migrations (key, applied_at)
             VALUES ('legacy-brand-import-v1', '2026-01-01T00:00:00Z');
             CREATE TABLE rebuild_count (value INTEGER NOT NULL);
             INSERT INTO rebuild_count VALUES (0);",
        )
        .unwrap();

        rebuild_legacy_search_indexes_once(&conn, increment_rebuild_count).unwrap();
        rebuild_legacy_search_indexes_once(&conn, increment_rebuild_count).unwrap();

        assert_eq!(
            conn.query_row("SELECT value FROM rebuild_count", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn old_base_database_is_ignored() {
        let base_path = temp_database_path("discard-old-data");
        fs::create_dir_all(base_path.parent().expect("path should have parent"))
            .expect("temporary directory should be created");
        Connection::open(&base_path)
            .expect("old base database should be created")
            .execute("CREATE TABLE old_data (value TEXT NOT NULL)", [])
            .expect("old data should be inserted");

        let db = Database::open(base_path.clone()).expect("new storage should open");
        assert!(base_path.exists(), "old base file is left untouched");
        assert!(db
            .list_model_configs()
            .expect("models should load")
            .is_empty());
        assert!(!table_exists(&db.config_conn, "old_data"));
        assert!(!table_exists(&db.conn, "old_data"));
        assert!(!table_exists(&db.knowledge_conn, "old_data"));
        assert!(!table_exists(&db.project_conn, "old_data"));

        drop(db);
        fs::remove_dir_all(base_path.parent().expect("path should have parent"))
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
