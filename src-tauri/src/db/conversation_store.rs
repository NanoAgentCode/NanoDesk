use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use super::{clean_or_default, ensure_affected, serialize_metadata, Database};
use crate::error::{AppError, AppResult};
use crate::models::{
    Conversation, ConversationDraft, Message, MessageDraft, UsageAnalysis, UsageModelCount,
    UsageModelTokens, UsageTokenTrendPoint,
};
use std::collections::BTreeMap;

impl Database {
    pub fn get_usage_analysis(&self) -> AppResult<UsageAnalysis> {
        let conversation_count =
            self.conn
                .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))?;
        let message_count = self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;

        let mut model_statement = self.conn.prepare(
            "SELECT c.model_config_id, COUNT(*)
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             WHERE m.role = 'assistant'
             GROUP BY c.model_config_id
             ORDER BY COUNT(*) DESC",
        )?;
        let model_rows = model_statement
            .query_map([], |row| {
                Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let model_usage = model_rows
            .into_iter()
            .map(|(model_config_id, count)| {
                let model_name = model_config_id
                    .as_deref()
                    .and_then(|id| self.get_model_config(id).ok())
                    .map(|config| config.name)
                    .unwrap_or_else(|| "未指定模型".to_string());
                UsageModelCount {
                    model_config_id,
                    model_name,
                    count,
                }
            })
            .collect();

        let cutoff_date = (Utc::now().date_naive() - chrono::Duration::days(29)).to_string();
        let mut trend = BTreeMap::<String, (i64, i64, BTreeMap<Option<String>, i64>)>::new();
        let mut message_statement = self.conn.prepare(
            "SELECT m.role, m.content, substr(m.created_at, 1, 10), c.model_config_id
             FROM messages m
             JOIN conversations c ON c.id = m.conversation_id
             ORDER BY m.created_at",
        )?;
        let rows = message_statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut prompt_tokens = 0;
        let mut completion_tokens = 0;
        for row in rows {
            let (role, content, date, model_config_id) = row?;
            let tokens = estimate_usage_tokens(&content);
            if role == "assistant" {
                completion_tokens += tokens;
            } else {
                prompt_tokens += tokens;
            }
            if date >= cutoff_date {
                let entry = trend.entry(date).or_default();
                if role == "assistant" {
                    entry.1 += tokens;
                } else {
                    entry.0 += tokens;
                }
                *entry.2.entry(model_config_id).or_default() += tokens;
            }
        }
        let token_trend = trend
            .into_iter()
            .map(|(date, (prompt_tokens, completion_tokens, model_tokens))| {
                let models = model_tokens
                    .into_iter()
                    .map(|(model_config_id, tokens)| UsageModelTokens {
                        model_name: model_config_id
                            .as_deref()
                            .and_then(|id| self.get_model_config(id).ok())
                            .map(|config| config.name)
                            .unwrap_or_else(|| "未指定模型".to_string()),
                        model_config_id,
                        tokens,
                    })
                    .collect();
                UsageTokenTrendPoint {
                    date,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens: prompt_tokens + completion_tokens,
                    models,
                }
            })
            .collect();

        Ok(UsageAnalysis {
            conversation_count,
            message_count,
            model_usage,
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            token_trend,
            latency_call_count: 0,
            average_latency_ms: 0.0,
            p95_latency_ms: 0,
        })
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

fn estimate_usage_tokens(text: &str) -> i64 {
    let mut ascii_chars = 0usize;
    let mut non_ascii_chars = 0usize;
    for character in text.chars() {
        if character.is_ascii() {
            ascii_chars += 1;
        } else {
            non_ascii_chars += 1;
        }
    }
    ((ascii_chars + 3) / 4 + non_ascii_chars) as i64
}

#[cfg(test)]
mod usage_tests {
    use super::{estimate_usage_tokens, Database};
    use crate::models::{ConversationDraft, MessageDraft};

    #[test]
    fn estimates_ascii_and_non_ascii_tokens() {
        assert_eq!(estimate_usage_tokens("abcdefgh"), 2);
        assert_eq!(estimate_usage_tokens("你好ab"), 3);
    }

    #[test]
    fn aggregates_conversations_messages_models_and_token_trend() {
        let path = std::env::temp_dir()
            .join(format!("nanodesk-usage-{}", uuid::Uuid::new_v4()))
            .join("nanodesk.sqlite3");
        std::fs::create_dir_all(path.parent().expect("temporary path should have a parent"))
            .expect("temporary directory should be created");
        let db = Database::open(path).expect("database should open");
        let conversation = db
            .create_conversation(ConversationDraft {
                title: Some("Usage test".to_string()),
                model_config_id: None,
                project_path: None,
            })
            .expect("conversation should be created");
        for (role, content) in [("user", "abcdefgh"), ("assistant", "你好ab")] {
            db.append_message(MessageDraft {
                conversation_id: conversation.id.clone(),
                role: role.to_string(),
                content: content.to_string(),
                metadata: None,
            })
            .expect("message should be saved");
        }

        let analysis = db.get_usage_analysis().expect("usage should load");

        assert_eq!(analysis.conversation_count, 1);
        assert_eq!(analysis.message_count, 2);
        assert_eq!(analysis.prompt_tokens, 2);
        assert_eq!(analysis.completion_tokens, 3);
        assert_eq!(analysis.total_tokens, 5);
        assert_eq!(analysis.model_usage[0].model_name, "未指定模型");
        assert_eq!(analysis.model_usage[0].count, 1);
        assert_eq!(analysis.token_trend.len(), 1);
        assert_eq!(analysis.token_trend[0].total_tokens, 5);
        assert_eq!(analysis.token_trend[0].models[0].model_name, "未指定模型");
        assert_eq!(analysis.token_trend[0].models[0].tokens, 5);
    }
}
