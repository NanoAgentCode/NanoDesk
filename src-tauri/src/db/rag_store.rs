use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use super::{
    clean_optional_string, clean_or_default, cosine_similarity, decode_embedding, encode_embedding,
    ensure_affected, estimate_token_count, row_to_rag_file, Database,
};
use crate::error::{AppError, AppResult};
use crate::models::{RagChunkMatch, RagFile};

pub(crate) struct RagFileReplacement<'a> {
    pub conversation_id: &'a str,
    pub name: &'a str,
    pub mime: &'a str,
    pub size: i64,
    pub content_hash: &'a str,
    pub chunks: &'a [String],
    pub embeddings: &'a [Vec<f32>],
    pub embedding_model: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_summary_cache_is_initialized_with_document_storage() {
        let db = Database::open(std::path::PathBuf::from(":memory:")).unwrap();
        let exists: i64 = db.conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='document_summary_nodes'", [], |row| row.get(0)).unwrap();
        assert_eq!(exists, 1, "文档概要必须持久化，不能只保存在某次任务中");
    }

    #[test]
    fn document_summary_nodes_survive_reopen_and_are_versioned_and_cascade_deleted() {
        let root = std::env::temp_dir().join(format!("document-summary-cache-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("data.sqlite3");
        let db = Database::open(path.clone()).unwrap();
        let conversation = db.create_conversation(crate::models::ConversationDraft { title: None, model_config_id: None, project_path: None }).unwrap();
        let file = db.replace_rag_file(RagFileReplacement { conversation_id: &conversation.id, name: "test.txt", mime: "text/plain", size: 3, content_hash: "version-1", chunks: &["raw text".into()], embeddings: &[Vec::new()], embedding_model: "fulltext" }).unwrap();
        db.save_document_summary_node(&file.id, "version-1", "model-A/leaf", 0, "叶节点事实").unwrap();
        db.save_document_summary_node(&file.id, "version-1", "model-A/parent", 1, "父节点概要").unwrap();
        db.save_document_summary_node(&file.id, "version-2", "model-A/leaf", 0, "错误版本").unwrap();
        drop(db);
        let db = Database::open(path).unwrap();
        assert_eq!(db.document_summary_node(&file.id, "version-1", "model-A/leaf").unwrap().as_deref(), Some("叶节点事实"));
        assert_eq!(db.document_summary_node(&file.id, "version-1", "model-A/parent").unwrap().as_deref(), Some("父节点概要"));
        assert!(db.document_summary_node(&file.id, "version-2", "model-A/leaf").unwrap().is_none());
        assert!(db.document_summary_node(&file.id, "version-1", "model-B/leaf").unwrap().is_none());
        assert!(db.document_summary_node("other-file", "version-1", "model-A/leaf").unwrap().is_none());
        db.delete_rag_file(&file.id).unwrap();
        db.save_document_summary_node(&file.id, "version-1", "late", 0, "删除后才返回的结果").unwrap();
        assert_eq!(db.conn.query_row("SELECT COUNT(*) FROM document_summary_nodes", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_replacement_rolls_back_file_chunks_vectors_and_fts_together() {
        let db = Database::open(std::path::PathBuf::from(":memory:")).unwrap();
        let conversation = db.create_conversation(crate::models::ConversationDraft { title: None, model_config_id: None, project_path: None }).unwrap();
        let old = db.replace_rag_file(RagFileReplacement { conversation_id: &conversation.id, name: "test.txt", mime: "text/plain", size: 3, content_hash: "same", chunks: &["old complete text".into()], embeddings: &[vec![1.0, 0.0]], embedding_model: "qa" }).unwrap();
        db.conn.execute_batch("CREATE TRIGGER reject_second BEFORE INSERT ON rag_chunks WHEN NEW.chunk_index=1 BEGIN SELECT RAISE(ABORT, 'test insertion failure'); END;").unwrap();
        let result = db.replace_rag_file(RagFileReplacement { conversation_id: &conversation.id, name: "test.txt", mime: "text/plain", size: 6, content_hash: "same", chunks: &["new first".into(), "new second".into()], embeddings: &[vec![1.0, 0.0], vec![0.0, 1.0]], embedding_model: "qa" });
        assert!(result.is_err());
        assert_eq!(db.list_rag_files(&conversation.id).unwrap()[0].id, old.id);
        assert_eq!(db.load_document_chunks(&conversation.id).unwrap()[0].text, "old complete text");
        assert_eq!(db.search_rag_chunks(&conversation.id, &[1.0, 0.0], 6).unwrap()[0].text, "old complete text");
        let fts: i64 = db.conn.query_row("SELECT COUNT(*) FROM rag_chunks_fts WHERE text='old complete text'", [], |row| row.get(0)).unwrap();
        assert_eq!(fts, 1);
    }
}

impl Database {
    pub(crate) fn document_summary_node(&self, file_id: &str, content_hash: &str, key: &str) -> AppResult<Option<String>> {
        Ok(self.conn.query_row("SELECT nodes.summary FROM document_summary_nodes nodes JOIN rag_files files ON files.id=nodes.file_id WHERE nodes.file_id=?1 AND nodes.content_hash=?2 AND files.content_hash=?2 AND nodes.cache_key=?3", params![file_id, content_hash, key], |row| row.get(0)).optional()?)
    }

    pub(crate) fn save_document_summary_node(&self, file_id: &str, content_hash: &str, key: &str, level: u32, summary: &str) -> AppResult<()> {
        // If the document was removed/replaced during generation, discard the
        // result rather than resurrecting it or attaching it to another version.
        self.conn.execute("INSERT INTO document_summary_nodes(file_id,content_hash,cache_key,level,summary,created_at) SELECT id,content_hash,?3,?4,?5,?6 FROM rag_files WHERE id=?1 AND content_hash=?2 ON CONFLICT(file_id,content_hash,cache_key) DO UPDATE SET summary=excluded.summary", params![file_id,content_hash,key,level,summary,Utc::now().to_rfc3339()])?;
        Ok(())
    }

    pub fn rag_embedding_model(&self, file_id: &str) -> AppResult<Option<String>> {
        Ok(self.conn.query_row("SELECT embeddings.model FROM rag_embeddings embeddings JOIN rag_chunks chunks ON chunks.id = embeddings.chunk_id WHERE chunks.file_id = ?1 LIMIT 1", [file_id], |row| row.get(0)).optional()?)
    }

    pub fn load_document_chunks(&self, conversation_id: &str) -> AppResult<Vec<RagChunkMatch>> {
        let mut stmt = self.conn.prepare("SELECT chunks.id, chunks.file_id, files.name, chunks.chunk_index, chunks.text FROM rag_chunks chunks JOIN rag_files files ON files.id = chunks.file_id WHERE chunks.conversation_id = ?1 ORDER BY files.created_at, files.id, chunks.chunk_index")?;
        let rows = stmt.query_map([conversation_id], |row| Ok(RagChunkMatch {
            chunk_id: row.get(0)?, file_id: row.get(1)?, file_name: row.get(2)?, chunk_index: row.get(3)?, text: row.get(4)?, score: 0.0,
        }))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
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

        let mut rows = rows;
        for file in &mut rows {
            file.preparation = self.document_summary_node(&file.id, &file.content_hash, crate::document_insights::STATUS_KEY)?
                .and_then(|text| serde_json::from_str(&text).ok());
        }
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

    pub fn replace_rag_file(&self, replacement: RagFileReplacement<'_>) -> AppResult<RagFile> {
        let RagFileReplacement {
            conversation_id,
            name,
            mime,
            size,
            content_hash,
            chunks,
            embeddings,
            embedding_model,
        } = replacement;
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

        let transaction = self.conn.unchecked_transaction()?;
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
            status: if embedding_model == "fulltext" { "fulltext" } else { "ready" }.to_string(),
            error: None,
            preparation: None,
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
            if !embedding.is_empty() { self.conn.execute(
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
            )?; }
            self.conn.execute(
                "
                INSERT INTO rag_chunks_fts
                    (chunk_id, conversation_id, file_id, file_name, text)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![chunk_id, file.conversation_id, file.id, file.name, text],
            )?;
        }

        transaction.commit()?;
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
            if embedding.len() != query_embedding.len() { return Err("文档索引与查询向量维度不一致，请重新建立索引。".into()); }
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
}
