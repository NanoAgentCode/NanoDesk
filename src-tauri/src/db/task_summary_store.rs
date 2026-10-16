use super::*;
use crate::task_summary::TaskSummary;
use rusqlite::OptionalExtension;

impl Database {
    pub(crate) fn append_response_with_task_summary(
        &self,
        draft: crate::models::MessageDraft,
        replace_id: Option<&str>,
        summary: Option<&TaskSummary>,
    ) -> AppResult<crate::models::Message> {
        self.with_savepoint("task_summary_response", || {
            // Regeneration without a valid update retains the last known task facts.
            let previous = if replace_id.is_some() && summary.is_none() {
                self.task_summary(&draft.conversation_id)?
            } else {
                None
            };
            let message = self.append_background_response(draft, replace_id)?;
            if let Some(summary) = summary.or(previous.as_ref()) {
                self.save_task_summary(&message.id, summary)?;
            }
            Ok(message)
        })
    }

    pub(crate) fn task_summary(&self, conversation_id: &str) -> AppResult<Option<TaskSummary>> {
        let json: Option<String> = self.conn.query_row(
            "SELECT s.payload FROM task_summaries s JOIN messages m ON m.id=s.message_id WHERE m.conversation_id=?1 ORDER BY s.rowid DESC LIMIT 1",
            [conversation_id], |row| row.get(0),
        ).optional()?;
        json.map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub(crate) fn save_task_summary(
        &self,
        message_id: &str,
        summary: &TaskSummary,
    ) -> AppResult<()> {
        let conversation_id: String = self.conn.query_row(
            "SELECT conversation_id FROM messages WHERE id=?1",
            [message_id],
            |row| row.get(0),
        )?;
        if self.task_summary(&conversation_id)?.as_ref() == Some(summary) {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO task_summaries(message_id,payload) VALUES(?1,?2)",
            params![message_id, serde_json::to_string(summary)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConversationDraft, MessageDraft};

    #[test]
    fn task_summary_is_scoped_persistent_and_survives_failed_regeneration() {
        let directory = std::env::temp_dir().join(format!("task-summary-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("test.sqlite3");
        let db = Database::open(path.clone()).unwrap();
        let create = || {
            db.create_conversation(ConversationDraft {
                title: None,
                model_config_id: None,
                project_path: None,
            })
            .unwrap()
        };
        let a = create();
        let b = create();
        let summary = TaskSummary {
            main_task: Some("设计摘要".into()),
            items: vec!["只设计，不实施".into()],
            constraints: vec!["未授权实施".into()],
            completed: vec![],
            pending: vec!["确认方案".into()],
            blocked: vec![],
            resume: "继续设计".into(),
            side_question: None,
        };
        let draft = || MessageDraft {
            conversation_id: a.id.clone(),
            role: "assistant".into(),
            content: "正常回答".into(),
            metadata: None,
        };
        let first = db
            .append_response_with_task_summary(draft(), None, Some(&summary))
            .unwrap();
        assert_eq!(db.task_summary(&a.id).unwrap(), Some(summary.clone()));
        assert_eq!(db.task_summary(&b.id).unwrap(), None);
        let replacement = db
            .append_response_with_task_summary(draft(), Some(&first.id), None)
            .unwrap();
        assert_eq!(db.task_summary(&a.id).unwrap(), Some(summary.clone()));
        drop(db);
        let db = Database::open(path).unwrap();
        assert_eq!(db.task_summary(&a.id).unwrap(), Some(summary));
        assert_eq!(db.list_messages(&a.id).unwrap().len(), 1);
        db.delete_messages(&[replacement.id]).unwrap();
        assert_eq!(db.task_summary(&a.id).unwrap(), None);
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
