use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::params;
#[cfg(test)]
use rusqlite::OptionalExtension;

use super::Database;
use crate::error::{AppError, AppResult};

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
        Self::init_config_schema(&self.config_conn)?;
        Self::init_conversation_schema(&self.conn)?;
        Self::init_knowledge_schema(&self.knowledge_conn)?;
        Self::init_project_index_schema(&self.project_conn)?;

        self.sync_model_config_references()?;
        self.rebuild_missing_memory_graphs()
            .map_err(|error| AppError::Message(format!("memory graph rebuild failed: {error}")))?;
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::models::{ConversationDraft, MessageDraft, ModelConfigDraft, ModelSupplierDraft};
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
    fn persists_asr_model_purpose() {
        let path = temp_database_path("asr-model-purpose");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = Database::open(path).unwrap();
        let mut draft = model_draft("asr-model", "ASR");
        draft.model = "Qwen/Qwen3-ASR-1.7B".into();
        draft.model_kind = "asr".into();
        let saved = db.save_model_config(draft).unwrap();
        assert_eq!(saved.model_kind, "asr");
        assert_eq!(db.get_model_config("asr-model").unwrap().model_kind, "asr");
    }

    #[test]
    fn reopening_current_storage_does_not_rebuild_schema_or_invent_suppliers() {
        let base_path = temp_database_path("reopen-current");
        fs::create_dir_all(base_path.parent().unwrap()).unwrap();
        let db = Database::open(base_path.clone()).unwrap();
        db.save_model_config(model_draft("model-1", "Standalone")).unwrap();
        let conversation = db.create_conversation(ConversationDraft {
            title: Some("Keep history".into()),
            model_config_id: Some("model-1".into()),
            project_path: None,
        }).unwrap();
        db.append_message(MessageDraft {
            conversation_id: conversation.id.clone(), role: "user".into(),
            content: "preserved".into(), metadata: None,
        }).unwrap();
        let versions = |db: &Database| {
            [&db.config_conn, &db.conn, &db.knowledge_conn, &db.project_conn]
                .map(|conn| conn.query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0)).unwrap())
        };
        let before = versions(&db);
        drop(db);
        let db = Database::open(base_path.clone()).unwrap();
        assert_eq!(versions(&db), before, "opening current storage must not recreate tables");
        assert!(db.list_model_suppliers().unwrap().is_empty());
        assert_eq!(db.get_model_config("model-1").unwrap().name, "Standalone");
        assert_eq!(db.list_messages(&conversation.id).unwrap()[0].content, "preserved");
        drop(db);
        fs::remove_dir_all(base_path.parent().unwrap()).unwrap();
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
        assert!(table_exists(&db.config_conn, "model_suppliers"));
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

    #[test]
    fn deleting_supplier_cascades_models_but_preserves_chat_history() {
        let db = Database::open(PathBuf::from(":memory:")).expect("database should open");
        let supplier = db
            .save_model_supplier(ModelSupplierDraft {
                id: Some("supplier-1".to_string()),
                name: "Primary".to_string(),
                provider: "openai-compatible".to_string(),
                base_url: "http://localhost".to_string(),
                api_key: "test-secret".to_string(),
            })
            .expect("supplier should save");
        db.save_model_config(model_draft("model-1", "Primary model"))
            .expect("associated model should save");
        let mut unrelated = model_draft("model-2", "Unrelated model");
        unrelated.base_url = "http://other-host".to_string();
        db.save_model_config(unrelated)
            .expect("unrelated model should save");

        let conversation = db
            .create_conversation(ConversationDraft {
                title: Some("Preserved chat".to_string()),
                model_config_id: Some("model-1".to_string()),
                project_path: None,
            })
            .expect("conversation should save");
        db.append_message(MessageDraft {
            conversation_id: conversation.id.clone(),
            role: "user".to_string(),
            content: "keep this message".to_string(),
            metadata: None,
        })
        .expect("message should save");

        db.delete_model_supplier(&supplier.id)
            .expect("supplier should delete");

        assert!(db.list_model_suppliers().unwrap().is_empty());
        let remaining_models = db.list_model_configs().unwrap();
        assert_eq!(remaining_models.len(), 1);
        assert_eq!(remaining_models[0].id, "model-2");
        let model_reference: Option<String> = db
            .conn
            .query_row(
                "SELECT model_config_id FROM conversations WHERE id=?1",
                params![conversation.id],
                |row| row.get(0),
            )
            .expect("conversation should remain");
        assert_eq!(model_reference, None);
        let messages = db.list_messages(&conversation.id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "keep this message");
    }
}
