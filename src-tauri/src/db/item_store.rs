use chrono::Utc;
use rusqlite::params;
use uuid::Uuid;

use super::{build_fts_prefix_query, clean_or_default, ensure_affected, Database};
use crate::error::{AppError, AppResult};
use crate::models::{Item, ItemDraft, ItemPatch};

impl Database {
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

        let mut stmt = self.knowledge_conn.prepare(sql)?;
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

        let mut stmt = self.knowledge_conn.prepare(
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
        self.knowledge_conn
            .execute("DELETE FROM items_fts WHERE id = ?1", params![id])?;
        let affected = self
            .knowledge_conn
            .execute("DELETE FROM items WHERE id = ?1", params![id])?;
        ensure_affected(affected, "item not found")?;
        Ok(())
    }
}
