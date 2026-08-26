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

impl Database {
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
}
