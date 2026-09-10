use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use super::{clean_or_default, ensure_affected, serialize_metadata, Database};
use crate::error::{AppError, AppResult};
use crate::models::{Conversation, ConversationDraft, Message, MessageDraft};

impl Database {
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

    pub fn list_conversation_project_paths(&self) -> AppResult<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT DISTINCT project_path
             FROM conversations
             WHERE project_path IS NOT NULL AND TRIM(project_path) <> ''
             ORDER BY project_path COLLATE NOCASE",
        )?;
        let paths = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::from)?;
        Ok(paths)
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
        if message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.exclude_from_profile)
            .unwrap_or(false)
        {
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
}
